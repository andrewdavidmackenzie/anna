//! Membership handler — processes node join/depart notifications.
//!
//! Mirrors `server/cpp/src/route/membership_handler.cpp`.

use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
use anna_server_common::metadata::Tier;
use anna_server_common::threads::{RoutingThread, ServerThread};

use crate::context::{OutgoingMessage, RouteContext};

/// Handle a membership notification (join or depart).
///
/// Format: `"join:{TIER}:{PUBLIC_IP}:{PRIVATE_IP}:{JOIN_COUNT}"` or
///         `"depart:{TIER}:{PUBLIC_IP}:{PRIVATE_IP}"`
pub(crate) fn handle(ctx: &mut RouteContext, serialized: &str) -> Vec<OutgoingMessage> {
    let parts: Vec<&str> = serialized.split(':').collect();

    let is_join = parts[0] == "join";
    let tier_offset = if is_join || parts[0] == "depart" {
        1
    } else {
        0
    };

    if parts.len() < tier_offset + 3 {
        log::warn!("membership: malformed message: {}", serialized);
        return vec![];
    }

    let tier_name = parts[tier_offset];
    let public_ip = parts[tier_offset + 1];
    let private_ip = parts[tier_offset + 2];
    let join_count: i32 = if parts.len() > tier_offset + 3 {
        parts[tier_offset + 3].parse().unwrap_or(0)
    } else {
        0
    };

    let tier = match tier_name {
        "MEMORY" => Tier::Memory,
        "DISK" => Tier::Disk,
        _ => {
            log::warn!("membership: unknown tier: {}", tier_name);
            return vec![];
        }
    };

    let mut outgoing = Vec::new();

    if is_join {
        let ring = ctx
            .global_hash_rings
            .entry(tier)
            .or_insert_with(ConsistentHashRing::new);

        ring.insert(
            public_ip,
            private_ip,
            0,
            ctx.rt.base_offset(),
            DEFAULT_VIRTUAL_THREAD_NUM,
            true,
        );

        log::info!(
            "Node join: tier {:?}, {}:{}, join_count={}",
            tier,
            public_ip,
            private_ip,
            join_count
        );

        // Thread 0: gossip join to all KVS servers and relay to sibling routing threads.
        if ctx.thread_id == 0 {
            // Notify all KVS servers about the new member.
            let kvs_msg = format!("{}:{}:{}:{}", tier_name, public_ip, private_ip, join_count);
            for ring in ctx.global_hash_rings.values() {
                for st in ring.get_unique_servers() {
                    if st.private_ip() != private_ip {
                        outgoing
                            .push((st.node_join_connect_address(), kvs_msg.as_bytes().to_vec()));
                    }
                }
            }

            // Relay to sibling routing threads.
            for tid in 1..ctx.thread_count {
                let rt = RoutingThread::new(&ctx.ip, tid, ctx.rt.base_offset());
                outgoing.push((rt.notify_connect_address(), serialized.as_bytes().to_vec()));
            }
        }
    } else {
        // Depart
        if let Some(ring) = ctx.global_hash_rings.get_mut(&tier) {
            ring.remove(public_ip, private_ip, 0);
            log::info!("Node depart: tier {:?}, {}:{}", tier, public_ip, private_ip);
        }

        // Thread 0: relay to sibling routing threads.
        if ctx.thread_id == 0 {
            for tid in 1..ctx.thread_count {
                let rt = RoutingThread::new(&ctx.ip, tid, ctx.rt.base_offset());
                outgoing.push((rt.notify_connect_address(), serialized.as_bytes().to_vec()));
            }
        }
    }

    outgoing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_adds_to_ring() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msg = "join:MEMORY:2.2.2.2:10.0.0.2:0";
        let _ = handle(&mut ctx, msg);
        assert!(!ctx.global_hash_rings[&Tier::Memory].is_empty());
    }

    #[test]
    fn depart_removes_from_ring() {
        let mut ctx = crate::context::tests::make_test_ctx();
        // First add a node.
        let _ = handle(&mut ctx, "join:MEMORY:2.2.2.2:10.0.0.2:0");
        assert!(!ctx.global_hash_rings[&Tier::Memory].is_empty());

        // Then remove it.
        let _ = handle(&mut ctx, "depart:MEMORY:2.2.2.2:10.0.0.2");
        assert!(ctx.global_hash_rings[&Tier::Memory].is_empty());
    }

    #[test]
    fn malformed_message_ignored() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msgs = handle(&mut ctx, "bad");
        assert!(msgs.is_empty());
    }

    #[test]
    fn thread0_relays_join() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.thread_id = 0;
        ctx.thread_count = 2;
        let msgs = handle(&mut ctx, "join:MEMORY:2.2.2.2:10.0.0.2:0");
        // Should relay to thread 1.
        assert!(!msgs.is_empty());
    }
}
