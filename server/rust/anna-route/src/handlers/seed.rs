//! Seed handler — responds to seed membership queries from new KVS nodes.
//!
//! Mirrors `server/cpp/src/route/seed_handler.cpp`.

use anna_server_common::proto::metadata::cluster_membership::{tier_membership, TierMembership};
use anna_server_common::proto::metadata::ClusterMembership;
use prost::Message;

use crate::context::RouteContext;

/// Build a seed response containing all known cluster members.
/// Returns serialized `ClusterMembership` protobuf.
pub(crate) fn handle(ctx: &RouteContext) -> Vec<u8> {
    let mut membership = ClusterMembership::default();

    for (tier, ring) in &ctx.global_hash_rings {
        let mut tier_mem = TierMembership {
            tier_id: *tier as i32,
            ..Default::default()
        };
        for st in ring.get_unique_servers() {
            tier_mem.servers.push(tier_membership::Server {
                public_ip: st.public_ip().to_string(),
                private_ip: st.private_ip().to_string(),
            });
        }
        membership.tiers.push(tier_mem);
    }

    membership.encode_to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
    use anna_server_common::metadata::Tier;

    #[test]
    fn seed_returns_membership() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let mut ring = ConsistentHashRing::new();
        ring.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            0,
            DEFAULT_VIRTUAL_THREAD_NUM,
            true,
        );
        ctx.global_hash_rings.insert(Tier::Memory, ring);

        let data = handle(&ctx);
        let membership = ClusterMembership::decode(data.as_slice()).unwrap();
        assert!(!membership.tiers.is_empty());
        assert!(!membership.tiers[0].servers.is_empty());
        assert_eq!(membership.tiers[0].servers[0].public_ip, "1.2.3.4");
    }

    #[test]
    fn seed_empty_ring() {
        let ctx = crate::context::tests::make_test_ctx();
        let data = handle(&ctx);
        let membership = ClusterMembership::decode(data.as_slice()).unwrap();
        assert!(membership.tiers.is_empty());
    }
}
