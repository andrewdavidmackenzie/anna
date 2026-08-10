//! Routing utilities for determining which server threads are responsible
//! for a given key, based on the consistent hash ring and replication factors.
//!
//! Mirrors `server/cpp/src/hash_ring/hash_ring.cpp`.

use std::collections::HashMap;

use crate::hash_ring::ConsistentHashRing;
use crate::metadata::{
    get_metadata_key, is_metadata, KeyReplication, Tier, TierMetadata, ALL_DATA_TIERS,
};
use crate::threads::ServerThread;
use crate::types::{GlobalRingMap, Key, LocalRingMap, ServerThreadList};

/// Default metadata replication factor.
pub const DEFAULT_METADATA_REPLICATION_FACTOR: u32 = 1;

/// Default local replication factor.
pub const DEFAULT_LOCAL_REPLICATION: u32 = 1;

/// Result of `get_responsible_threads`.
#[derive(Debug)]
pub enum ResponsibleResult {
    /// Successfully determined the responsible threads.
    Ok(ServerThreadList),
    /// The replication factor for this key is not known; a replication
    /// factor request should be issued. Contains the replication metadata
    /// key that should be fetched.
    NeedReplicationFactor(Key),
}

/// Find the first tier that has nodes in the global hash ring.
pub fn first_tier_with_nodes(global_hash_rings: &GlobalRingMap) -> Option<Tier> {
    for tier in ALL_DATA_TIERS {
        if let Some(ring) = global_hash_rings.get(tier) {
            if !ring.is_empty() {
                return Some(*tier);
            }
        }
    }
    None
}

/// Determine which server threads are responsible for the given key.
///
/// For metadata keys, uses `metadata_replication_factor` and the memory
/// tier ring. For data keys, looks up the key's per-key replication
/// factors from `key_replication_map`.
///
/// Returns `ResponsibleResult::NeedReplicationFactor` if the replication
/// factor is not known for a data key.
pub fn get_responsible_threads(
    key: &str,
    is_meta: bool,
    global_hash_rings: &GlobalRingMap,
    local_hash_rings: &LocalRingMap,
    key_replication_map: &HashMap<Key, KeyReplication>,
    metadata_replication_factor: u32,
    default_local_replication: u32,
) -> ResponsibleResult {
    if is_meta {
        let threads = get_responsible_threads_metadata(
            key,
            global_hash_rings,
            local_hash_rings,
            metadata_replication_factor,
            default_local_replication,
        );
        return ResponsibleResult::Ok(threads);
    }

    // Data key: look up per-key replication factors.
    let kr = match key_replication_map.get(key) {
        Some(kr) => kr,
        None => {
            let rep_key = get_metadata_key(key, "replication");
            return ResponsibleResult::NeedReplicationFactor(rep_key);
        }
    };

    let mut result = Vec::new();

    for tier in ALL_DATA_TIERS {
        let global_ring = match global_hash_rings.get(tier) {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };
        let local_ring = match local_hash_rings.get(tier) {
            Some(r) => r,
            None => continue,
        };

        let global_rep = kr.global_replication.get(tier).copied().unwrap_or(1);
        let local_rep = kr
            .local_replication
            .get(tier)
            .copied()
            .unwrap_or(default_local_replication);

        let servers = global_ring.find_responsible(key, global_rep, true);
        for st in servers {
            let tids = local_ring.find_responsible_local(key, local_rep);
            for tid in tids {
                result.push(ServerThread::new(
                    st.public_ip(),
                    st.private_ip(),
                    tid,
                    global_ring.base_offset(),
                ));
            }
        }
    }

    ResponsibleResult::Ok(result)
}

/// Determine responsible threads for a metadata key.
/// Uses the memory tier ring with fixed replication factors.
fn get_responsible_threads_metadata(
    key: &str,
    global_hash_rings: &GlobalRingMap,
    local_hash_rings: &LocalRingMap,
    metadata_replication_factor: u32,
    default_local_replication: u32,
) -> ServerThreadList {
    let global_ring = match global_hash_rings.get(&Tier::Memory) {
        Some(r) => r,
        None => return vec![],
    };
    let local_ring = match local_hash_rings.get(&Tier::Memory) {
        Some(r) => r,
        None => return vec![],
    };

    let servers = global_ring.find_responsible(key, metadata_replication_factor, true);
    let mut result = Vec::new();
    for st in servers {
        let tids = local_ring.find_responsible_local(key, default_local_replication);
        for tid in tids {
            result.push(ServerThread::new(
                st.public_ip(),
                st.private_ip(),
                tid,
                global_ring.base_offset(),
            ));
        }
    }
    result
}

/// Build a replication factor GET request key for the given data key,
/// and find which server thread should receive it.
///
/// Returns `(target_thread, replication_metadata_key)` or `None` if no
/// servers are available.
pub fn replication_request_target(
    key: &str,
    global_hash_rings: &GlobalRingMap,
    local_hash_rings: &LocalRingMap,
    metadata_replication_factor: u32,
    default_local_replication: u32,
) -> Option<(ServerThread, Key)> {
    let rep_key = get_metadata_key(key, "replication");
    let threads = get_responsible_threads_metadata(
        &rep_key,
        global_hash_rings,
        local_hash_rings,
        metadata_replication_factor,
        default_local_replication,
    );
    threads.into_iter().next().map(|t| (t, rep_key))
}

/// Find the target thread for a metadata request and return its address.
/// Returns `None` if no memory-tier servers exist.
pub fn metadata_request_target(
    key: &str,
    global_hash_rings: &GlobalRingMap,
    local_hash_rings: &LocalRingMap,
    metadata_replication_factor: u32,
    default_local_replication: u32,
) -> Option<ServerThread> {
    let threads = get_responsible_threads_metadata(
        key,
        global_hash_rings,
        local_hash_rings,
        metadata_replication_factor,
        default_local_replication,
    );
    threads.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};

    fn make_single_node_rings() -> (GlobalRingMap, LocalRingMap) {
        let mut global = HashMap::new();
        let mut local = HashMap::new();

        let mut g_ring = ConsistentHashRing::new();
        g_ring.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            0,
            DEFAULT_VIRTUAL_THREAD_NUM,
            true,
        );

        let mut l_ring = ConsistentHashRing::new();
        l_ring.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            0,
            DEFAULT_VIRTUAL_THREAD_NUM,
            false,
        );

        global.insert(Tier::Memory, g_ring);
        local.insert(Tier::Memory, l_ring);

        (global, local)
    }

    #[test]
    fn metadata_key_routes_to_memory_tier() {
        let (global, local) = make_single_node_rings();
        let kr_map = HashMap::new();

        match get_responsible_threads(
            "ANNA_METADATA|replication|mykey",
            true,
            &global,
            &local,
            &kr_map,
            1,
            1,
        ) {
            ResponsibleResult::Ok(threads) => {
                assert!(!threads.is_empty());
                assert_eq!(threads[0].private_ip(), "10.0.0.1");
            }
            _ => panic!("Expected Ok"),
        }
    }

    #[test]
    fn data_key_without_replication_factor() {
        let (global, local) = make_single_node_rings();
        let kr_map = HashMap::new();

        match get_responsible_threads("user_key", false, &global, &local, &kr_map, 1, 1) {
            ResponsibleResult::NeedReplicationFactor(rep_key) => {
                assert_eq!(rep_key, "ANNA_METADATA|replication|user_key");
            }
            _ => panic!("Expected NeedReplicationFactor"),
        }
    }

    #[test]
    fn data_key_with_replication_factor() {
        let (global, local) = make_single_node_rings();
        let mut kr_map = HashMap::new();
        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        kr_map.insert("user_key".to_string(), kr);

        match get_responsible_threads("user_key", false, &global, &local, &kr_map, 1, 1) {
            ResponsibleResult::Ok(threads) => {
                assert!(!threads.is_empty());
                assert_eq!(threads[0].private_ip(), "10.0.0.1");
            }
            _ => panic!("Expected Ok"),
        }
    }

    #[test]
    fn replication_request_target_returns_thread() {
        let (global, local) = make_single_node_rings();
        let result = replication_request_target("user_key", &global, &local, 1, 1);
        assert!(result.is_some());
        let (thread, rep_key) = result.unwrap();
        assert_eq!(thread.private_ip(), "10.0.0.1");
        assert_eq!(rep_key, "ANNA_METADATA|replication|user_key");
    }

    #[test]
    fn empty_ring_returns_empty() {
        let global = HashMap::new();
        let local = HashMap::new();
        let kr_map = HashMap::new();

        match get_responsible_threads(
            "ANNA_METADATA|stats|key",
            true,
            &global,
            &local,
            &kr_map,
            1,
            1,
        ) {
            ResponsibleResult::Ok(threads) => assert!(threads.is_empty()),
            _ => panic!("Expected Ok with empty list"),
        }
    }

    #[test]
    fn first_tier_with_nodes_finds_memory() {
        let (global, _) = make_single_node_rings();
        assert_eq!(first_tier_with_nodes(&global), Some(Tier::Memory));
    }

    #[test]
    fn first_tier_with_nodes_empty() {
        let global = HashMap::new();
        assert_eq!(first_tier_with_nodes(&global), None);
    }
}
