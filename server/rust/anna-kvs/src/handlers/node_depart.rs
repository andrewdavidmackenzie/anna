//! Node departure handler — removes a departing node from the hash ring.
//!
//! Mirrors `server/cpp/src/kvs/node_depart_handler.cpp`.

use anna_server_common::metadata::Tier;
use anna_server_common::types::Address;

use crate::context::{KvsContext, OutgoingMessage};

/// Handle a node departure message.
///
/// Format: `"{TIER}:{PUBLIC_IP}:{PRIVATE_IP}:{JOIN_COUNT}"`
///
/// Thread 0 relays the departure to worker threads 1..N.
pub fn handle(ctx: &mut KvsContext, serialized: &str) -> Vec<OutgoingMessage> {
    let parts: Vec<&str> = serialized.split(':').collect();
    if parts.len() < 3 {
        log::warn!("node_depart: malformed message: {}", serialized);
        return vec![];
    }

    let tier_name = parts[0];
    let _public_ip = parts[1];
    let private_ip = parts[2];

    let tier = match tier_name {
        "MEMORY" => Tier::Memory,
        "DISK" => Tier::Disk,
        _ => {
            log::warn!("node_depart: unknown tier: {}", tier_name);
            return vec![];
        }
    };

    // Remove from global hash ring.
    if let Some(ring) = ctx.global_hash_rings.get_mut(&tier) {
        ring.remove(_public_ip, private_ip, 0);
        log::info!(
            "Removed {}:{} from {:?} ring (now {} nodes)",
            _public_ip,
            private_ip,
            tier,
            ring.get_unique_servers().len()
        );
    }

    let mut outgoing = Vec::new();

    // Thread 0 relays to worker threads 1..N.
    if ctx.thread_id == 0 {
        for tid in 1..ctx.thread_count {
            let addr = anna_server_common::threads::ServerThread::new(
                &ctx.public_ip,
                &ctx.private_ip,
                tid,
                ctx.wt.base_offset(),
            )
            .node_depart_connect_address();
            outgoing.push((addr, serialized.as_bytes().to_vec()));
        }
    }

    outgoing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_node_from_ring() {
        let mut ctx = crate::context::test_support::make_test_ctx_with_node("1.2.3.4", "10.0.0.1");
        assert!(!ctx.global_hash_rings[&Tier::Memory].is_empty());

        let msg = "MEMORY:1.2.3.4:10.0.0.1:0";
        let _ = handle(&mut ctx, msg);

        assert!(ctx.global_hash_rings[&Tier::Memory].is_empty());
    }

    #[test]
    fn malformed_message_ignored() {
        let mut ctx = crate::context::test_support::make_test_ctx_with_node("1.2.3.4", "10.0.0.1");
        let msgs = handle(&mut ctx, "bad");
        assert!(msgs.is_empty());
        // Ring should be unchanged.
        assert!(!ctx.global_hash_rings[&Tier::Memory].is_empty());
    }
}
