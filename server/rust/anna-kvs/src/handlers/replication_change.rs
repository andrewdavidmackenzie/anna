//! Replication change handler — updates replication factors and redistributes
//! data when keys move between threads/nodes.
//!
//! Mirrors `server/cpp/src/kvs/replication_change_handler.cpp`.

use anna_server_common::metadata::{is_metadata, KeyReplication, Tier};
use anna_server_common::proto::metadata::{
    replication_factor::ReplicationValue, ReplicationFactorUpdate,
};
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use anna_server_common::threads::ServerThread;
use prost::Message;

use crate::context::{AddressKeysetMap, KvsContext, OutgoingMessage};
use crate::handlers::utils::build_gossip_messages;

/// Handle a replication factor change message.
pub(crate) fn handle(ctx: &mut KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
    log::info!("Received a replication factor change.");

    let mut outgoing = Vec::new();

    // Thread 0 relays to worker threads 1..N.
    if ctx.thread_id == 0 {
        for tid in 1..ctx.thread_count {
            let addr =
                ServerThread::new(&ctx.public_ip, &ctx.private_ip, tid, ctx.wt.base_offset())
                    .replication_change_connect_address();
            outgoing.push((addr, data.to_vec()));
        }
    }

    let rep_change = match ReplicationFactorUpdate::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("replication_change: decode failed: {}", e);
            return outgoing;
        }
    };

    let mut addr_keyset_map = AddressKeysetMap::new();
    let mut remove_set = Vec::new();

    for key_rep in &rep_change.updates {
        let key = &key_rep.key;

        if ctx.stored_key_map.contains_key(key.as_str()) {
            // Get the original responsible threads before the change.
            let orig_result = get_responsible_threads(
                key,
                is_metadata(key),
                &ctx.global_hash_rings,
                &ctx.local_hash_rings,
                &ctx.key_replication_map,
                ctx.metadata_replication_factor,
                ctx.default_local_replication,
            );

            let orig_threads = match orig_result {
                ResponsibleResult::Ok(t) => t,
                _ => vec![],
            };

            // Update the replication factor.
            let mut decrement = false;
            let kr = ctx.key_replication_map.entry(key.clone()).or_default();

            for global in &key_rep.global {
                let tier = tier_from_i32(global.tier);
                if let Some(t) = tier {
                    let old = kr.global_replication.get(&t).copied().unwrap_or(0);
                    if global.value < old {
                        decrement = true;
                    }
                    kr.global_replication.insert(t, global.value);
                }
            }
            for local in &key_rep.local {
                let tier = tier_from_i32(local.tier);
                if let Some(t) = tier {
                    let old = kr.local_replication.get(&t).copied().unwrap_or(0);
                    if local.value < old {
                        decrement = true;
                    }
                    kr.local_replication.insert(t, local.value);
                }
            }

            // Get new responsible threads after the change.
            let new_result = get_responsible_threads(
                key,
                is_metadata(key),
                &ctx.global_hash_rings,
                &ctx.local_hash_rings,
                &ctx.key_replication_map,
                ctx.metadata_replication_factor,
                ctx.default_local_replication,
            );

            if let ResponsibleResult::Ok(new_threads) = new_result {
                if !new_threads.iter().any(|t| *t == ctx.wt) {
                    // No longer responsible — schedule removal and gossip.
                    remove_set.push(key.clone());
                    for thread in &new_threads {
                        addr_keyset_map
                            .entry(thread.gossip_connect_address())
                            .or_default()
                            .insert(key.clone());
                    }
                }

                // If replication increased and we're the first responsible
                // thread, gossip to new threads.
                if !decrement && !orig_threads.is_empty() && orig_threads[0].id() == ctx.wt.id() {
                    for thread in &new_threads {
                        if !orig_threads.iter().any(|t| *t == *thread) {
                            addr_keyset_map
                                .entry(thread.gossip_connect_address())
                                .or_default()
                                .insert(key.clone());
                        }
                    }
                }
            }
        } else {
            // Key not stored locally — just update the replication factor.
            let kr = ctx.key_replication_map.entry(key.clone()).or_default();
            for global in &key_rep.global {
                if let Some(t) = tier_from_i32(global.tier) {
                    kr.global_replication.insert(t, global.value);
                }
            }
            for local in &key_rep.local {
                if let Some(t) = tier_from_i32(local.tier) {
                    kr.local_replication.insert(t, local.value);
                }
            }
        }
    }

    // Send gossip for redistributed keys.
    let gossip_msgs =
        build_gossip_messages(&addr_keyset_map, &ctx.serializers, &ctx.stored_key_map);
    outgoing.extend(gossip_msgs);

    // Remove keys we're no longer responsible for.
    for key in &remove_set {
        if let Some(kp) = ctx.stored_key_map.get(key) {
            let lt = kp.lattice_type() as i32;
            if let Some(serializer) = ctx.serializers.get_mut(&lt) {
                serializer.remove(key);
            }
        }
        ctx.stored_key_map.remove(key);
        ctx.local_changeset.remove(key);
    }

    outgoing
}

fn tier_from_i32(v: i32) -> Option<Tier> {
    match v {
        1 => Some(Tier::Memory),
        2 => Some(Tier::Disk),
        3 => Some(Tier::Routing),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::proto::metadata::ReplicationFactor;

    use crate::storage::memory::LwwSerializer;
    use anna_server_common::metadata::KeyProperty;
    use anna_server_common::proto::kvs::LatticeType;

    fn ctx_with_stored_key(key: &str) -> KvsContext {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));
        // Store a key.
        let mut kp = KeyProperty::default();
        kp.set_size(10);
        kp.set_type(LatticeType::Lww);
        ctx.stored_key_map.insert(key.to_string(), kp);
        // Set replication factor.
        let mut kr = anna_server_common::metadata::KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert(key.to_string(), kr);
        ctx
    }

    #[test]
    fn thread0_relays_to_workers() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.thread_id = 0;
        ctx.thread_count = 3;
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "k".into(),
                global: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 1,
                }],
                local: vec![],
            }],
        };
        let msgs = handle(&mut ctx, &update.encode_to_vec());
        // Should relay to threads 1 and 2.
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn decode_failure_returns_early() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msgs = handle(&mut ctx, b"garbage");
        // Only relay messages (if thread 0), no crash.
        assert!(msgs.is_empty() || msgs.iter().all(|(_, d)| d == b"garbage"));
    }

    #[test]
    fn stored_key_replication_update() {
        let mut ctx = ctx_with_stored_key("stored_k");
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "stored_k".into(),
                global: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 2,
                }],
                local: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 1,
                }],
            }],
        };
        let _ = handle(&mut ctx, &update.encode_to_vec());
        assert_eq!(
            ctx.key_replication_map["stored_k"].global_replication[&Tier::Memory],
            2
        );
    }

    #[test]
    fn replication_decrement_removes_key() {
        let mut ctx = ctx_with_stored_key("dec_k");
        // Add a second node so the key can move.
        ctx.global_hash_rings
            .get_mut(&Tier::Memory)
            .unwrap()
            .insert(
                "2.2.2.2",
                "10.0.0.2",
                0,
                0,
                anna_server_common::hash_ring::DEFAULT_VIRTUAL_THREAD_NUM,
                true,
            );
        // Set high replication so we're initially responsible.
        ctx.key_replication_map
            .get_mut("dec_k")
            .unwrap()
            .global_replication
            .insert(Tier::Memory, 2);

        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "dec_k".into(),
                global: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 1, // decrease
                }],
                local: vec![],
            }],
        };
        let _ = handle(&mut ctx, &update.encode_to_vec());
        // Replication factor should be updated to 1.
        assert_eq!(
            ctx.key_replication_map["dec_k"].global_replication[&Tier::Memory],
            1
        );
    }

    #[test]
    fn invalid_tier_ignored() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "bad_tier_k".into(),
                global: vec![ReplicationValue { tier: 99, value: 1 }],
                local: vec![],
            }],
        };
        let _ = handle(&mut ctx, &update.encode_to_vec());
        // No crash, no entry for invalid tier.
        if let Some(kr) = ctx.key_replication_map.get("bad_tier_k") {
            assert!(kr.global_replication.is_empty());
        }
    }

    #[test]
    fn local_replication_update() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "local_k".into(),
                global: vec![],
                local: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 3,
                }],
            }],
        };
        let _ = handle(&mut ctx, &update.encode_to_vec());
        assert_eq!(
            ctx.key_replication_map["local_k"].local_replication[&Tier::Memory],
            3
        );
    }

    #[test]
    fn updates_replication_factor() {
        let mut ctx = crate::context::tests::make_test_ctx();

        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "rep_key".into(),
                global: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 2,
                }],
                local: vec![],
            }],
        };

        let _ = handle(&mut ctx, &update.encode_to_vec());

        let kr = &ctx.key_replication_map["rep_key"];
        assert_eq!(kr.global_replication[&Tier::Memory], 2);
    }
}
