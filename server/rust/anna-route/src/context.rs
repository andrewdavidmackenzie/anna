//! Routing server context — shared state passed to all handlers.

use std::collections::HashMap;

use anna_server_common::metadata::KeyReplication;
use anna_server_common::threads::RoutingThread;
use anna_server_common::types::{Address, GlobalRingMap, Key, LocalRingMap};

/// Pending client request waiting for replication factor resolution.
/// Stores (response_address, request_id).
pub(crate) type PendingRequest = (Address, String);

/// All mutable routing state shared across handlers.
pub(crate) struct RouteContext {
    pub thread_id: u32,
    pub ip: Address,
    pub rt: RoutingThread,
    pub thread_count: u32,

    pub global_hash_rings: GlobalRingMap,
    pub local_hash_rings: LocalRingMap,
    pub key_replication_map: HashMap<Key, KeyReplication>,
    pub pending_requests: HashMap<Key, Vec<PendingRequest>>,

    pub default_local_replication: u32,
    pub metadata_replication_factor: u32,

    pub monitoring_ips: Vec<Address>,

    pub seed: u32,
}

/// Outgoing ZMQ message: (target_address, serialized_payload).
pub(crate) type OutgoingMessage = (Address, Vec<u8>);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn make_test_ctx() -> RouteContext {
        RouteContext {
            thread_id: 0,
            ip: "127.0.0.1".to_string(),
            rt: RoutingThread::new("127.0.0.1", 0, 0),
            thread_count: 1,
            global_hash_rings: HashMap::new(),
            local_hash_rings: HashMap::new(),
            key_replication_map: HashMap::new(),
            pending_requests: HashMap::new(),
            default_local_replication: 1,
            metadata_replication_factor: 1,
            monitoring_ips: vec![],
            seed: 42,
        }
    }
}
