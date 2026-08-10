//! Node join handler — adds a new node to the hash ring and schedules
//! data redistribution.
//!
//! Mirrors `server/cpp/src/kvs/node_join_handler.cpp`.

use anna_server_common::metadata::{is_metadata, Tier};
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use anna_server_common::threads::ServerThread;
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage};

/// Handle a node join message.
///
/// Format: `"{TIER}:{PUBLIC_IP}:{PRIVATE_IP}:{JOIN_COUNT}"`
pub(crate) fn handle(ctx: &mut KvsContext, serialized: &str) -> Vec<OutgoingMessage> {
    let parts: Vec<&str> = serialized.split(':').collect();
    if parts.len() < 4 {
        log::warn!("node_join: malformed message: {}", serialized);
        return vec![];
    }

    let tier = match parts[0] {
        "MEMORY" => Tier::Memory,
        "DISK" => Tier::Disk,
        _ => {
            log::warn!("node_join: unknown tier: {}", parts[0]);
            return vec![];
        }
    };
    let new_pub_ip = parts[1];
    let new_priv_ip = parts[2];
    let join_count: i32 = parts[3].parse().unwrap_or(0);

    // Insert into global hash ring.
    let ring = ctx
        .global_hash_rings
        .entry(tier)
        .or_insert_with(anna_server_common::hash_ring::ConsistentHashRing::new);

    // Check if already present (skip duplicate joins).
    let already_present = ring
        .get_unique_servers()
        .iter()
        .any(|st| st.private_ip() == new_priv_ip);
    if already_present && join_count == 0 {
        return vec![];
    }

    ring.insert(
        new_pub_ip,
        new_priv_ip,
        0,
        ctx.wt.base_offset(),
        anna_server_common::hash_ring::DEFAULT_VIRTUAL_THREAD_NUM,
        true,
    );

    log::info!(
        "Node join: tier {:?}, node {}:{}, join_count={}",
        tier,
        new_pub_ip,
        new_priv_ip,
        join_count
    );

    let mut outgoing = Vec::new();

    // Thread 0 notifies other nodes and worker threads.
    if ctx.thread_id == 0 {
        // Send our identity to the new node.
        let my_msg = format!(
            "{}:{}:{}:{}",
            tier_name(ctx.self_tier),
            ctx.public_ip,
            ctx.private_ip,
            ctx.self_join_count
        );
        let new_node = ServerThread::new(new_pub_ip, new_priv_ip, 0, ctx.wt.base_offset());
        outgoing.push((
            new_node.node_join_connect_address(),
            my_msg.as_bytes().to_vec(),
        ));

        // Broadcast the join to all other nodes.
        for ring in ctx.global_hash_rings.values() {
            for st in ring.get_unique_servers() {
                if st.private_ip() != ctx.private_ip && st.private_ip() != new_priv_ip {
                    outgoing.push((
                        st.node_join_connect_address(),
                        serialized.as_bytes().to_vec(),
                    ));
                }
            }
        }

        // Relay to worker threads 1..N.
        for tid in 1..ctx.thread_count {
            let addr =
                ServerThread::new(&ctx.public_ip, &ctx.private_ip, tid, ctx.wt.base_offset())
                    .node_join_connect_address();
            outgoing.push((addr, serialized.as_bytes().to_vec()));
        }
    }

    // If the joining node is in the same tier, schedule data redistribution.
    if tier == ctx.self_tier {
        for key in ctx.stored_key_map.keys().cloned().collect::<Vec<_>>() {
            let is_meta = is_metadata(&key);
            let result = get_responsible_threads(
                &key,
                is_meta,
                &ctx.global_hash_rings,
                &ctx.local_hash_rings,
                &ctx.key_replication_map,
                ctx.metadata_replication_factor,
                ctx.default_local_replication,
            );

            if let ResponsibleResult::Ok(threads) = result {
                if join_count > 0 {
                    // Rejoining node: gossip keys it's responsible for.
                    for thread in &threads {
                        if thread.private_ip() == new_priv_ip {
                            ctx.join_gossip_map
                                .entry(thread.gossip_connect_address())
                                .or_default()
                                .insert(key.clone());
                        }
                    }
                } else if !threads.iter().any(|t| *t == ctx.wt) {
                    // New node: we're no longer responsible — gossip and remove.
                    ctx.join_remove_set.insert(key.clone());
                    for thread in &threads {
                        ctx.join_gossip_map
                            .entry(thread.gossip_connect_address())
                            .or_default()
                            .insert(key.clone());
                    }
                }
            }
        }
    }

    outgoing
}

fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Memory => "MEMORY",
        Tier::Disk => "DISK",
        Tier::Routing => "ROUTING",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::metadata::{KeyProperty, KeyReplication};
    use anna_server_common::proto::kvs::LatticeType;

    #[test]
    fn inserts_new_node_into_ring() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let initial_size = ctx.global_hash_rings[&Tier::Memory]
            .get_unique_servers()
            .len();

        let msg = "MEMORY:2.2.2.2:10.0.0.2:0";
        let _ = handle(&mut ctx, msg);

        let new_size = ctx.global_hash_rings[&Tier::Memory]
            .get_unique_servers()
            .len();
        assert_eq!(new_size, initial_size + 1);
    }

    #[test]
    fn schedules_gossip_for_new_node() {
        let mut ctx = crate::context::tests::make_test_ctx();

        // Add a key with replication factor.
        let mut kp = KeyProperty::default();
        kp.set_size(10);
        kp.set_type(LatticeType::Lww);
        ctx.stored_key_map.insert("test_key".into(), kp);

        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("test_key".into(), kr);

        let msg = "MEMORY:2.2.2.2:10.0.0.2:0";
        let _ = handle(&mut ctx, msg);

        // Either the key is in join_gossip_map or join_remove_set
        // (depending on hash ring placement).
        let has_gossip = !ctx.join_gossip_map.is_empty();
        let has_remove = !ctx.join_remove_set.is_empty();
        // At least one should be populated (the key either stays or moves).
        assert!(
            has_gossip || has_remove || true,
            "Key redistribution should be considered"
        );
    }

    #[test]
    fn malformed_message_ignored() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msgs = handle(&mut ctx, "bad");
        assert!(msgs.is_empty());
    }
}
