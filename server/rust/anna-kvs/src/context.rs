//! KVS server context — shared state passed to all handlers.
//!
//! Mirrors the scattered local variables in `server/cpp/src/kvs/server.cpp`'s
//! `run()` function. Collecting them here makes handler signatures cleaner
//! and ownership explicit.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use anna_server_common::metadata::{KeyProperty, KeyReplication, Tier, TierMetadata};
use anna_server_common::threads::ServerThread;
use anna_server_common::types::{Address, GlobalRingMap, Key, LocalRingMap};

use crate::storage::SerializerMap;

/// Pending client request waiting for replication factor resolution.
#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub r#type: i32,
    pub lattice_type: i32,
    pub payload: Vec<u8>,
    pub addr: Address,
    pub response_id: String,
    pub expiry_epoch_ms: u64,
}

/// Pending gossip waiting for replication factor resolution.
#[derive(Debug, Clone)]
pub struct PendingGossip {
    pub lattice_type: i32,
    pub payload: Vec<u8>,
    pub expiry_epoch_ms: u64,
}

/// Map from target address to set of keys to gossip to that address.
pub type AddressKeysetMap = HashMap<Address, HashSet<Key>>;

/// Outgoing message: (target_address, serialized_payload).
pub type OutgoingMessage = (Address, Vec<u8>);

/// All mutable KVS state shared across handlers.
pub struct KvsContext {
    // ── Identity ──
    pub thread_id: u32,
    pub public_ip: Address,
    pub private_ip: Address,
    pub wt: ServerThread,
    pub self_tier: Tier,
    pub thread_count: u32,

    // ── Hash rings ──
    pub global_hash_rings: GlobalRingMap,
    pub local_hash_rings: LocalRingMap,

    // ── Storage ──
    pub stored_key_map: HashMap<Key, KeyProperty>,
    pub serializers: SerializerMap,

    // ── Replication ──
    pub key_replication_map: HashMap<Key, KeyReplication>,
    pub tier_metadata: HashMap<Tier, TierMetadata>,
    pub default_local_replication: u32,
    pub metadata_replication_factor: u32,
    pub self_join_count: i32,

    // ── Pending requests/gossip (waiting for replication factor) ──
    pub pending_requests: HashMap<Key, Vec<PendingRequest>>,
    pub pending_gossip: HashMap<Key, Vec<PendingGossip>>,

    // ── Tracking ──
    pub key_access_tracker: HashMap<Key, BTreeSet<Instant>>,
    pub local_changeset: HashSet<Key>,
    pub access_count: u64,

    // ── Join/depart state ──
    pub join_gossip_map: AddressKeysetMap,
    pub join_remove_set: HashSet<Key>,

    // ── Cache state ──
    pub extant_caches: HashSet<Address>,
    pub cache_ip_to_keys: HashMap<Address, HashSet<Key>>,
    pub key_to_cache_ips: HashMap<Key, HashSet<Address>>,

    // ── Network ──
    pub routing_ips: Vec<Address>,
    pub monitoring_ips: Vec<Address>,

    // ── Counters ──
    pub rid: u32,
    pub seed: u32,
}

/// Test support utilities — available to downstream crates for testing.
pub mod test_support {
    use super::*;
    use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};

    /// Create a minimal KvsContext for testing with a single memory-tier node.
    pub fn make_test_ctx() -> KvsContext {
        make_test_ctx_with_node("1.1.1.1", "1.1.1.1")
    }

    /// Create a minimal KvsContext with a specific node in the ring.
    pub fn make_test_ctx_with_node(pub_ip: &str, priv_ip: &str) -> KvsContext {
        let mut global = HashMap::new();
        let mut local = HashMap::new();

        let mut g_ring = ConsistentHashRing::new();
        g_ring.insert(pub_ip, priv_ip, 0, 0, DEFAULT_VIRTUAL_THREAD_NUM, true);

        let mut l_ring = ConsistentHashRing::new();
        l_ring.insert(pub_ip, priv_ip, 0, 0, DEFAULT_VIRTUAL_THREAD_NUM, false);

        global.insert(Tier::Memory, g_ring);
        local.insert(Tier::Memory, l_ring);

        KvsContext {
            thread_id: 0,
            public_ip: pub_ip.to_string(),
            private_ip: priv_ip.to_string(),
            wt: ServerThread::new(pub_ip, priv_ip, 0, 0),
            self_tier: Tier::Memory,
            thread_count: 1,
            global_hash_rings: global,
            local_hash_rings: local,
            stored_key_map: HashMap::new(),
            serializers: HashMap::new(),
            key_replication_map: HashMap::new(),
            tier_metadata: HashMap::new(),
            default_local_replication: 1,
            metadata_replication_factor: 1,
            self_join_count: 0,
            pending_requests: HashMap::new(),
            pending_gossip: HashMap::new(),
            key_access_tracker: HashMap::new(),
            local_changeset: HashSet::new(),
            access_count: 0,
            join_gossip_map: HashMap::new(),
            join_remove_set: HashSet::new(),
            extant_caches: HashSet::new(),
            cache_ip_to_keys: HashMap::new(),
            key_to_cache_ips: HashMap::new(),
            routing_ips: vec![],
            monitoring_ips: vec![],
            rid: 0,
            seed: 42,
        }
    }
}
