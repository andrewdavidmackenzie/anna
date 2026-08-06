//! Key-to-server routing logic.
//!
//! Mirrors `server/cpp/src/hash_ring/hash_ring.cpp` — the two-tier
//! consistent hash lookup that maps a key to responsible server threads.

use crate::hash_ring::ConsistentHashRing;
use crate::metadata::{is_metadata, KeyReplication, Tier};
use crate::threads::ServerThread;
use crate::types::Key;
use std::collections::HashMap;

/// Per-tier global hash ring map.
pub type GlobalRingMap = HashMap<Tier, ConsistentHashRing>;

/// Per-tier local hash ring map.
pub type LocalRingMap = HashMap<Tier, ConsistentHashRing>;

/// All storage tiers.
pub const ALL_TIERS: [Tier; 2] = [Tier::Memory, Tier::Disk];

/// Result of `get_responsible_threads`.
pub enum RoutingResult {
    /// Successfully resolved to a list of server threads.
    Found(Vec<ServerThread>),
    /// Replication factor not known for this key — caller should
    /// issue a replication factor request and retry later.
    ReplicationUnknown,
}

/// Find the server threads responsible for a key.
///
/// Two-tier lookup:
/// 1. For each tier, find `global_rep` unique servers via the global ring
/// 2. For each server, find `local_rep` unique thread IDs via the local ring
/// 3. Combine into `ServerThread(public_ip, private_ip, tid)` for each pair
///
/// Metadata keys use a fixed replication factor and skip the replication map.
///
/// Mirrors `HashRingUtil::get_responsible_threads` in C++.
pub fn get_responsible_threads(
    key: &str,
    global_rings: &GlobalRingMap,
    local_rings: &LocalRingMap,
    key_replication_map: &HashMap<Key, KeyReplication>,
    tiers: &[Tier],
    base_offset: u32,
) -> RoutingResult {
    if is_metadata(key) {
        let tier = first_tier_with_nodes(global_rings);
        let threads = get_responsible_threads_metadata(
            key,
            global_rings.get(&tier),
            local_rings.get(&tier),
            base_offset,
        );
        return RoutingResult::Found(threads);
    }

    match key_replication_map.get(key) {
        None => RoutingResult::ReplicationUnknown,
        Some(rep) => {
            let mut result = Vec::new();

            for &tier in tiers {
                let global_rep = rep.global_replication.get(&tier).copied().unwrap_or(0);
                let local_rep = rep.local_replication.get(&tier).copied().unwrap_or(1);

                let global_ring = match global_rings.get(&tier) {
                    Some(r) => r,
                    None => continue,
                };
                let local_ring = match local_rings.get(&tier) {
                    Some(r) => r,
                    None => continue,
                };

                let servers = global_ring.find_responsible(key, global_rep, true);

                for st in &servers {
                    let tids = local_ring.find_responsible_local(key, local_rep);

                    for tid in tids {
                        result.push(ServerThread::new(
                            st.public_ip(),
                            st.private_ip(),
                            tid,
                            base_offset,
                        ));
                    }
                }
            }

            RoutingResult::Found(result)
        }
    }
}

/// Find responsible threads for a metadata key.
///
/// Metadata keys use a fixed replication factor (1 global, 1 local).
fn get_responsible_threads_metadata(
    key: &str,
    global_ring: Option<&ConsistentHashRing>,
    local_ring: Option<&ConsistentHashRing>,
    base_offset: u32,
) -> Vec<ServerThread> {
    let mut result = Vec::new();

    let global_ring = match global_ring {
        Some(r) if !r.is_empty() => r,
        _ => return result,
    };
    let local_ring = match local_ring {
        Some(r) if !r.is_empty() => r,
        _ => return result,
    };

    // Metadata uses replication factor 1 (single responsible server + thread).
    let servers = global_ring.find_responsible(key, 1, true);

    for st in &servers {
        let tids = local_ring.find_responsible_local(key, 1);
        for tid in tids {
            result.push(ServerThread::new(
                st.public_ip(),
                st.private_ip(),
                tid,
                base_offset,
            ));
        }
    }

    result
}

/// Return the first tier that has nodes in the global hash rings.
pub fn first_tier_with_nodes(global_rings: &GlobalRingMap) -> Tier {
    for &tier in &ALL_TIERS {
        if let Some(ring) = global_rings.get(&tier) {
            if !ring.is_empty() {
                return tier;
            }
        }
    }
    Tier::Memory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_ring::DEFAULT_VIRTUAL_THREAD_NUM;

    fn make_rings(base_offset: u32) -> (GlobalRingMap, LocalRingMap) {
        let mut global = GlobalRingMap::new();
        let mut local = LocalRingMap::new();

        let mut gr = ConsistentHashRing::new();
        gr.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            base_offset,
            DEFAULT_VIRTUAL_THREAD_NUM,
            true,
        );
        global.insert(Tier::Memory, gr);

        let mut lr = ConsistentHashRing::new();
        lr.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            base_offset,
            DEFAULT_VIRTUAL_THREAD_NUM,
            false,
        );
        lr.insert(
            "1.2.3.4",
            "10.0.0.1",
            1,
            base_offset,
            DEFAULT_VIRTUAL_THREAD_NUM,
            false,
        );
        local.insert(Tier::Memory, lr);

        (global, local)
    }

    #[test]
    fn metadata_key_routes_to_single_thread() {
        let (global, local) = make_rings(0);
        let rep_map = HashMap::new();

        match get_responsible_threads(
            "ANNA_METADATA|replication|mykey",
            &global,
            &local,
            &rep_map,
            &ALL_TIERS,
            0,
        ) {
            RoutingResult::Found(threads) => {
                assert_eq!(threads.len(), 1);
                assert_eq!(threads[0].public_ip(), "1.2.3.4");
            }
            RoutingResult::ReplicationUnknown => panic!("metadata should not need replication map"),
        }
    }

    #[test]
    fn unknown_replication_returns_unknown() {
        let (global, local) = make_rings(0);
        let rep_map = HashMap::new();

        match get_responsible_threads("user_key", &global, &local, &rep_map, &ALL_TIERS, 0) {
            RoutingResult::ReplicationUnknown => {} // expected
            RoutingResult::Found(_) => panic!("should return ReplicationUnknown"),
        }
    }

    #[test]
    fn known_replication_routes_correctly() {
        let (global, local) = make_rings(0);
        let mut rep_map = HashMap::new();

        let mut rep = KeyReplication::default();
        rep.global_replication.insert(Tier::Memory, 1);
        rep.local_replication.insert(Tier::Memory, 1);
        rep_map.insert("user_key".to_string(), rep);

        match get_responsible_threads("user_key", &global, &local, &rep_map, &ALL_TIERS, 0) {
            RoutingResult::Found(threads) => {
                assert_eq!(threads.len(), 1);
                assert_eq!(threads[0].public_ip(), "1.2.3.4");
            }
            RoutingResult::ReplicationUnknown => panic!("should find threads"),
        }
    }

    #[test]
    fn replication_factor_2_returns_2_threads() {
        let (global, local) = make_rings(0);
        let mut rep_map = HashMap::new();

        let mut rep = KeyReplication::default();
        rep.global_replication.insert(Tier::Memory, 1);
        rep.local_replication.insert(Tier::Memory, 2); // 2 local replicas
        rep_map.insert("multi_key".to_string(), rep);

        match get_responsible_threads("multi_key", &global, &local, &rep_map, &ALL_TIERS, 0) {
            RoutingResult::Found(threads) => {
                assert_eq!(threads.len(), 2, "expected 2 threads (local_rep=2)");
                // Both should be on the same server but different tids
                assert_eq!(threads[0].public_ip(), threads[1].public_ip());
                assert_ne!(threads[0].tid(), threads[1].tid());
            }
            RoutingResult::ReplicationUnknown => panic!("should find threads"),
        }
    }

    #[test]
    fn first_tier_with_nodes_returns_memory() {
        let (global, _) = make_rings(0);
        assert_eq!(first_tier_with_nodes(&global), Tier::Memory);
    }

    #[test]
    fn first_tier_empty_returns_memory_default() {
        let global = GlobalRingMap::new();
        assert_eq!(first_tier_with_nodes(&global), Tier::Memory);
    }

    #[test]
    fn empty_rings_return_empty_threads() {
        let global = GlobalRingMap::new();
        let local = LocalRingMap::new();
        let mut rep_map = HashMap::new();

        let mut rep = KeyReplication::default();
        rep.global_replication.insert(Tier::Memory, 1);
        rep.local_replication.insert(Tier::Memory, 1);
        rep_map.insert("key".to_string(), rep);

        match get_responsible_threads("key", &global, &local, &rep_map, &ALL_TIERS, 0) {
            RoutingResult::Found(threads) => {
                assert!(threads.is_empty(), "empty rings should return no threads");
            }
            _ => panic!("should return Found with empty vec"),
        }
    }
}
