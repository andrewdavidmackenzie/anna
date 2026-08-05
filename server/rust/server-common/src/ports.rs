//! Port constants for all Anna server components.
//!
//! Mirrors `server/cpp/src/kvs/kvs_threads.hpp` and `server/cpp/src/threads.hpp`.
//!
//! Every port follows the formula:
//!   `base_port + tid + base_offset`  (per-thread ports)
//!   `base_port + base_offset`        (singleton ports)
//!
//! See `docs/ports.md` for the complete port layout.

// ── KVS Server (per-thread ports) ───────────────────────────────────

/// Port on which KVS servers listen for new node join notifications.
pub const NODE_JOIN_PORT: u32 = 6000;

/// Port on which KVS servers listen for node departure notifications.
pub const NODE_DEPART_PORT: u32 = 6050;

/// Port on which KVS servers are asked to self-depart by monitoring.
pub const SELF_DEPART_PORT: u32 = 6100;

/// Port on which KVS servers listen for replication factor responses.
pub const SERVER_REPLICATION_RESPONSE_PORT: u32 = 6150;

/// Port on which KVS servers listen for client GET/PUT/SCAN requests.
pub const KEY_REQUEST_PORT: u32 = 6200;

/// Port on which KVS servers listen for gossip from other KVS nodes.
pub const GOSSIP_PORT: u32 = 6250;

/// Port on which KVS servers listen for replication factor changes.
pub const SERVER_REPLICATION_CHANGE_PORT: u32 = 6300;

// ── Routing Server (per-thread ports) ───────────────────────────────

/// Port on which routing servers listen for cluster membership requests.
pub const SEED_PORT: u32 = 6350;

/// Port on which routing servers listen for cluster membership changes.
pub const ROUTING_NOTIFY_PORT: u32 = 6400;

/// Port on which routing servers listen for replication factor responses.
pub const ROUTING_REPLICATION_RESPONSE_PORT: u32 = 6500;

/// Port on which routing servers listen for replication factor changes.
pub const ROUTING_REPLICATION_CHANGE_PORT: u32 = 6550;

// ── Client-facing ports ─────────────────────────────────────────────

/// Port on which clients send key address requests to routing nodes.
pub const KEY_ADDRESS_PORT: u32 = 6450;

/// Port on which clients receive responses from the KVS.
pub const USER_RESPONSE_PORT: u32 = 6600;

/// Port on which clients receive responses from the routing tier.
pub const USER_KEY_ADDRESS_PORT: u32 = 6650;

// ── Benchmark ───────────────────────────────────────────────────────

/// Port on which benchmark nodes listen for triggers.
pub const BENCHMARK_COMMAND_PORT: u32 = 6700;

// ── Cache ───────────────────────────────────────────────────────────

/// Port on which KVS servers listen for cache IP lookup responses.
pub const CACHE_IP_RESPONSE_PORT: u32 = 6750;

/// Port on which KVS servers listen for management node responses.
pub const MANAGEMENT_NODE_RESPONSE_PORT: u32 = 6800;

/// Port on which cache nodes listen for updates from the KVS.
pub const CACHE_UPDATE_PORT: u32 = 6850;

/// Port on which KVS servers listen for direct cache registrations.
pub const CACHE_REGISTRATION_PORT: u32 = 6900;

// ── Monitoring (singleton ports) ────────────────────────────────────

/// Port on which monitoring listens for cluster membership changes.
pub const MONITORING_NOTIFY_PORT: u32 = 6950;

/// Port on which monitoring listens for KVS responses (metadata queries).
pub const MONITORING_RESPONSE_PORT: u32 = 6951;

/// Port on which monitoring waits for depart-done confirmations.
pub const DEPART_DONE_PORT: u32 = 6952;

/// Port on which monitoring listens for performance feedback from clients.
pub const FEEDBACK_REPORT_PORT: u32 = 6953;

// ── Management (singleton ports) ────────────────────────────────────

/// Port for restart count requests from KVS to management.
pub const MANAGEMENT_RESTART_COUNT_PORT: u32 = 6954;

/// Port for scaling alerts from monitoring to management.
pub const SCALING_ALERT_PORT: u32 = 6955;

/// Port for function/executor node list requests.
pub const MANAGEMENT_FUNC_NODES_PORT: u32 = 6956;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_match_cpp_constants() {
        // Verify critical port values match the C++ constants.
        assert_eq!(NODE_JOIN_PORT, 6000);
        assert_eq!(KEY_REQUEST_PORT, 6200);
        assert_eq!(GOSSIP_PORT, 6250);
        assert_eq!(SEED_PORT, 6350);
        assert_eq!(KEY_ADDRESS_PORT, 6450);
        assert_eq!(MONITORING_NOTIFY_PORT, 6950);
        assert_eq!(SCALING_ALERT_PORT, 6955);
    }

    #[test]
    fn port_groups_are_non_overlapping() {
        // Per-thread ports must be spaced at least 50 apart
        // to support up to 50 threads without collision.
        let per_thread = [
            NODE_JOIN_PORT,
            NODE_DEPART_PORT,
            SELF_DEPART_PORT,
            SERVER_REPLICATION_RESPONSE_PORT,
            KEY_REQUEST_PORT,
            GOSSIP_PORT,
            SERVER_REPLICATION_CHANGE_PORT,
            SEED_PORT,
            ROUTING_NOTIFY_PORT,
            KEY_ADDRESS_PORT,
            ROUTING_REPLICATION_RESPONSE_PORT,
            ROUTING_REPLICATION_CHANGE_PORT,
            USER_RESPONSE_PORT,
            USER_KEY_ADDRESS_PORT,
            BENCHMARK_COMMAND_PORT,
            CACHE_IP_RESPONSE_PORT,
            MANAGEMENT_NODE_RESPONSE_PORT,
            CACHE_UPDATE_PORT,
            CACHE_REGISTRATION_PORT,
        ];

        for i in 0..per_thread.len() {
            for j in (i + 1)..per_thread.len() {
                let diff = per_thread[i].abs_diff(per_thread[j]);
                assert!(
                    diff >= 50,
                    "Ports {} and {} are only {} apart (need >= 50)",
                    per_thread[i],
                    per_thread[j],
                    diff
                );
            }
        }
    }
}
