//! Replication factor management.
//!
//! Mirrors `server/cpp/src/monitor/replication_helpers.cpp`.

use anna_server_common::hash_ring::ConsistentHashRing;
use anna_server_common::metadata::{KeyReplication, Tier};
use anna_server_common::proto::kvs::{
    KeyRequest, KeyResponse, KeyTuple, LatticeType, LwwValue, RequestType,
};
use anna_server_common::proto::metadata::{
    replication_factor::ReplicationValue, ReplicationFactor, ReplicationFactorUpdate,
};
use anna_server_common::threads::{MonitoringThread, RoutingThread, ServerThread};
use anna_server_common::types::{Address, Key};
use log::{error, info, warn};
use omq_tokio::Socket as OmqSocket;
use prost::Message;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::monitor::SocketCache;

/// Create a new KeyReplication with the given factors.
pub fn create_new_replication(
    global_memory: u32,
    global_disk: u32,
    local_memory: u32,
    local_disk: u32,
) -> KeyReplication {
    let mut rep = KeyReplication::default();
    rep.global_replication.insert(Tier::Memory, global_memory);
    rep.global_replication.insert(Tier::Disk, global_disk);
    rep.local_replication.insert(Tier::Memory, local_memory);
    rep.local_replication.insert(Tier::Disk, local_disk);
    rep
}

/// Change replication factors for a set of keys.
///
/// For each key:
/// 1. Update `key_replication_map` with the new factors
/// 2. PUT the replication metadata to a KVS node
/// 3. Send ReplicationFactorUpdate to affected KVS and routing nodes
///
/// Keys that fail the PUT are restored to their original replication.
pub async fn change_replication_factor(
    requests: &HashMap<Key, KeyReplication>,
    global_hash_rings: &HashMap<Tier, ConsistentHashRing>,
    routing_ips: &[Address],
    key_replication_map: &mut HashMap<Key, KeyReplication>,
    pushers: &mut SocketCache,
    mt: &MonitoringThread,
    response_puller: &OmqSocket,
    rid: &mut u32,
    monitor_ip: &str,
    base_offset: u32,
    timeout: Duration,
) {
    if requests.is_empty() {
        return;
    }

    // Save original replication factors for rollback on failure.
    let mut orig_replication: HashMap<Key, KeyReplication> = HashMap::new();
    let mut keys_to_update: Vec<Key> = Vec::new();

    for (key, new_rep) in requests {
        let current = key_replication_map.get(key);
        if current.map(|c| c == new_rep).unwrap_or(false) {
            continue; // No change needed.
        }

        if let Some(current) = current {
            orig_replication.insert(key.clone(), current.clone());
        }

        // Update the in-memory map.
        key_replication_map.insert(key.clone(), new_rep.clone());
        keys_to_update.push(key.clone());
    }

    if keys_to_update.is_empty() {
        return;
    }

    info!(
        "Changing replication factor for {} key(s)",
        keys_to_update.len()
    );

    // PUT replication metadata to KVS nodes.
    let mut failed_keys: HashSet<Key> = HashSet::new();

    for key in &keys_to_update {
        let rep = &key_replication_map[key];
        let rep_factor = build_replication_factor(key, rep);
        let rep_key = format!("ANNA_METADATA|replication|{}", key);

        let mut rep_data = Vec::new();
        rep_factor.encode(&mut rep_data).ok();

        // Wrap in LWW.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;
        let lww = LwwValue {
            timestamp: ts,
            value: rep_data,
        };
        let payload = lww.encode_to_vec();

        *rid += 1;
        let request = KeyRequest {
            r#type: RequestType::Put as i32,
            response_address: mt.response_connect_address(),
            request_id: format!("{}:{}", monitor_ip, rid),
            tuples: vec![KeyTuple {
                key: rep_key,
                lattice_type: LatticeType::Lww as i32,
                payload,
                ..Default::default()
            }],
            ..Default::default()
        };

        // Find a KVS node to send to. Use the first memory node.
        let target = global_hash_rings
            .get(&Tier::Memory)
            .and_then(|ring| ring.get_unique_servers().into_iter().next())
            .map(|st| {
                ServerThread::new(st.public_ip(), st.private_ip(), 0, base_offset)
                    .key_request_connect_address()
            });

        let target_addr = match target {
            Some(addr) => addr,
            None => {
                warn!("No KVS nodes available for replication PUT");
                failed_keys.insert(key.clone());
                continue;
            }
        };

        let encoded = request.encode_to_vec();
        if let Err(e) = pushers.send(&target_addr, &encoded).await {
            warn!("Failed to send replication PUT for key {}: {}", key, e);
            failed_keys.insert(key.clone());
            continue;
        }

        // Wait for response.
        match tokio::time::timeout(timeout, response_puller.recv()).await {
            Ok(Ok(msg)) => {
                let bytes: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
                if let Ok(response) = KeyResponse::decode(bytes.as_slice()) {
                    for tuple in &response.tuples {
                        if tuple.error == 2 {
                            // Wrong address — key routed incorrectly.
                            warn!("Replication PUT for key {} rejected (wrong address)", key);
                            failed_keys.insert(key.clone());
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("ZMQ error receiving replication PUT response: {}", e);
                failed_keys.insert(key.clone());
            }
            Err(_) => {
                warn!("Replication PUT timed out for key {}", key);
                failed_keys.insert(key.clone());
            }
        }
    }

    // Send ReplicationFactorUpdate to KVS and routing nodes for successful keys.
    let mut update_map: HashMap<Address, ReplicationFactorUpdate> = HashMap::new();

    for key in &keys_to_update {
        if failed_keys.contains(key) {
            continue;
        }

        let rep = &key_replication_map[key];
        let orig = orig_replication.get(key);

        // Notify KVS nodes: use the max of old and new rep to cover all nodes.
        for tier in [Tier::Memory, Tier::Disk] {
            if let Some(ring) = global_hash_rings.get(&tier) {
                let new_global = rep.global_replication.get(&tier).copied().unwrap_or(0);
                let old_global = orig
                    .and_then(|o| o.global_replication.get(&tier))
                    .copied()
                    .unwrap_or(0);
                let max_rep = new_global.max(old_global);

                if max_rep == 0 {
                    continue;
                }

                // Get responsible servers for this replication level.
                // Since our hash ring may differ from C++, send to all
                // unique servers as a safe default.
                for st in ring.get_unique_servers() {
                    let addr = ServerThread::new(st.public_ip(), st.private_ip(), 0, base_offset)
                        .replication_change_connect_address();

                    let update = update_map.entry(addr).or_default();
                    add_replication_factor_to_update(update, key, rep);
                }
            }
        }

        // Notify all routing nodes.
        for rt_ip in routing_ips {
            let addr =
                RoutingThread::new(rt_ip, 0, base_offset).replication_change_connect_address();
            let update = update_map.entry(addr).or_default();
            add_replication_factor_to_update(update, key, rep);
        }
    }

    // Send all ReplicationFactorUpdate messages.
    for (addr, update) in &update_map {
        let encoded = update.encode_to_vec();
        if let Err(e) = pushers.send(addr, &encoded).await {
            error!("Failed to send ReplicationFactorUpdate to {}: {}", addr, e);
        }
    }

    // Restore original replication for failed keys.
    for key in &failed_keys {
        if let Some(orig) = orig_replication.get(key) {
            key_replication_map.insert(key.clone(), orig.clone());
        } else {
            key_replication_map.remove(key);
        }
    }

    if !failed_keys.is_empty() {
        warn!(
            "Replication change failed for {} key(s), restored originals",
            failed_keys.len()
        );
    }
}

fn build_replication_factor(key: &str, rep: &KeyReplication) -> ReplicationFactor {
    let mut rf = ReplicationFactor {
        key: key.to_string(),
        ..Default::default()
    };

    for (&tier, &value) in &rep.global_replication {
        rf.global.push(ReplicationValue {
            tier: tier as i32,
            value,
        });
    }

    for (&tier, &value) in &rep.local_replication {
        rf.local.push(ReplicationValue {
            tier: tier as i32,
            value,
        });
    }

    rf
}

fn add_replication_factor_to_update(
    update: &mut ReplicationFactorUpdate,
    key: &str,
    rep: &KeyReplication,
) {
    update.updates.push(build_replication_factor(key, rep));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_new_replication_sets_all_fields() {
        let rep = create_new_replication(2, 1, 1, 1);
        assert_eq!(rep.global_replication[&Tier::Memory], 2);
        assert_eq!(rep.global_replication[&Tier::Disk], 1);
        assert_eq!(rep.local_replication[&Tier::Memory], 1);
        assert_eq!(rep.local_replication[&Tier::Disk], 1);
    }

    #[test]
    fn build_replication_factor_creates_protobuf() {
        let rep = create_new_replication(3, 1, 1, 1);
        let rf = build_replication_factor("test_key", &rep);
        assert_eq!(rf.key, "test_key");
        assert_eq!(rf.global.len(), 2);
        assert_eq!(rf.local.len(), 2);
    }
}
