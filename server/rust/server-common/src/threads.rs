//! Server thread types with address generation methods.
//!
//! Mirrors `server/cpp/src/kvs/kvs_threads.hpp`. Each thread type generates
//! ZMQ addresses based on its IP, thread ID, and the cluster's base port offset.

use crate::ports::*;
use crate::types::Address;

/// Maximum thread ID. Port groups are spaced 50 apart, so tid must be < 50
/// to avoid overlapping with the next port group.
pub const MAX_TID: u32 = 50;

/// A KVS server thread. Each KVS node runs multiple threads, each with
/// its own set of ZMQ sockets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerThread {
    public_ip: String,
    private_ip: String,
    tid: u32,
    virtual_num: u32,
    base_offset: u32,
}

impl ServerThread {
    pub fn new(public_ip: &str, private_ip: &str, tid: u32, base_offset: u32) -> Self {
        assert!(tid < MAX_TID, "tid {} exceeds maximum {}", tid, MAX_TID);
        Self {
            public_ip: public_ip.to_string(),
            private_ip: private_ip.to_string(),
            tid,
            virtual_num: 0,
            base_offset,
        }
    }

    pub fn with_virtual(
        public_ip: &str,
        private_ip: &str,
        tid: u32,
        virtual_num: u32,
        base_offset: u32,
    ) -> Self {
        assert!(tid < MAX_TID, "tid {} exceeds maximum {}", tid, MAX_TID);
        Self {
            public_ip: public_ip.to_string(),
            private_ip: private_ip.to_string(),
            tid,
            virtual_num,
            base_offset,
        }
    }

    pub fn public_ip(&self) -> &str {
        &self.public_ip
    }
    pub fn private_ip(&self) -> &str {
        &self.private_ip
    }
    pub fn tid(&self) -> u32 {
        self.tid
    }
    pub fn virtual_num(&self) -> u32 {
        self.virtual_num
    }
    pub fn base_offset(&self) -> u32 {
        self.base_offset
    }

    pub fn id(&self) -> String {
        format!("{}:{}", self.private_ip, self.tid)
    }

    pub fn virtual_id(&self) -> String {
        format!("{}:{}_{}", self.private_ip, self.tid, self.virtual_num)
    }

    fn public_addr(&self, port: u32) -> Address {
        format!(
            "tcp://{}:{}",
            self.public_ip,
            self.tid + port + self.base_offset
        )
    }

    fn private_addr(&self, port: u32) -> Address {
        format!(
            "tcp://{}:{}",
            self.private_ip,
            self.tid + port + self.base_offset
        )
    }

    pub fn node_join_connect_address(&self) -> Address {
        self.private_addr(NODE_JOIN_PORT)
    }
    pub fn node_depart_connect_address(&self) -> Address {
        self.private_addr(NODE_DEPART_PORT)
    }
    pub fn self_depart_connect_address(&self) -> Address {
        self.private_addr(SELF_DEPART_PORT)
    }
    pub fn key_request_connect_address(&self) -> Address {
        self.public_addr(KEY_REQUEST_PORT)
    }
    pub fn key_request_bind_address(&self) -> Address {
        self.private_addr(KEY_REQUEST_PORT)
    }
    pub fn replication_response_connect_address(&self) -> Address {
        self.private_addr(SERVER_REPLICATION_RESPONSE_PORT)
    }
    pub fn gossip_connect_address(&self) -> Address {
        self.private_addr(GOSSIP_PORT)
    }
    pub fn replication_change_connect_address(&self) -> Address {
        self.private_addr(SERVER_REPLICATION_CHANGE_PORT)
    }
    pub fn cache_ip_response_connect_address(&self) -> Address {
        self.private_addr(CACHE_IP_RESPONSE_PORT)
    }
    pub fn management_node_response_connect_address(&self) -> Address {
        self.private_addr(MANAGEMENT_NODE_RESPONSE_PORT)
    }
    pub fn cache_registration_connect_address(&self) -> Address {
        self.public_addr(CACHE_REGISTRATION_PORT)
    }
}

/// A routing tier thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoutingThread {
    ip: String,
    tid: u32,
    base_offset: u32,
}

impl RoutingThread {
    pub fn new(ip: &str, tid: u32, base_offset: u32) -> Self {
        assert!(tid < MAX_TID, "tid {} exceeds maximum {}", tid, MAX_TID);
        Self {
            ip: ip.to_string(),
            tid,
            base_offset,
        }
    }

    pub fn ip(&self) -> &str {
        &self.ip
    }
    pub fn tid(&self) -> u32 {
        self.tid
    }
    pub fn base_offset(&self) -> u32 {
        self.base_offset
    }

    fn addr(&self, port: u32) -> Address {
        format!("tcp://{}:{}", self.ip, self.tid + port + self.base_offset)
    }

    pub fn seed_connect_address(&self) -> Address {
        self.addr(SEED_PORT)
    }
    pub fn notify_connect_address(&self) -> Address {
        self.addr(ROUTING_NOTIFY_PORT)
    }
    pub fn key_address_connect_address(&self) -> Address {
        self.addr(KEY_ADDRESS_PORT)
    }
    pub fn replication_response_connect_address(&self) -> Address {
        self.addr(ROUTING_REPLICATION_RESPONSE_PORT)
    }
    pub fn replication_change_connect_address(&self) -> Address {
        self.addr(ROUTING_REPLICATION_CHANGE_PORT)
    }
}

/// A monitoring thread (singleton per cluster).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonitoringThread {
    ip: String,
    base_offset: u32,
}

impl MonitoringThread {
    pub fn new(ip: &str, base_offset: u32) -> Self {
        Self {
            ip: ip.to_string(),
            base_offset,
        }
    }

    pub fn ip(&self) -> &str {
        &self.ip
    }

    fn addr(&self, port: u32) -> Address {
        format!("tcp://{}:{}", self.ip, port + self.base_offset)
    }

    pub fn notify_connect_address(&self) -> Address {
        self.addr(MONITORING_NOTIFY_PORT)
    }
    pub fn response_connect_address(&self) -> Address {
        self.addr(MONITORING_RESPONSE_PORT)
    }
    pub fn depart_done_connect_address(&self) -> Address {
        self.addr(DEPART_DONE_PORT)
    }
    pub fn feedback_report_connect_address(&self) -> Address {
        self.addr(FEEDBACK_REPORT_PORT)
    }
}

/// Helper: get the management restart count request address.
pub fn join_count_req_address(scaling_alert_ip: &str, base_offset: u32) -> Address {
    format!(
        "tcp://{}:{}",
        scaling_alert_ip,
        MANAGEMENT_RESTART_COUNT_PORT + base_offset
    )
}

/// Helper: get the scaling alert address.
pub fn scaling_alert_address(scaling_alert_ip: &str, base_offset: u32) -> Address {
    format!(
        "tcp://{}:{}",
        scaling_alert_ip,
        SCALING_ALERT_PORT + base_offset
    )
}

/// Represents a cache node thread.
/// Mirrors `CacheThread` in `server/cpp/src/threads.hpp`.
#[derive(Debug, Clone)]
pub struct CacheThread {
    ip: Address,
    tid: u32,
    base_offset: u32,
}

impl CacheThread {
    pub fn new(ip: &str, tid: u32, base_offset: u32) -> Self {
        assert!(tid < MAX_TID, "tid {} exceeds maximum {}", tid, MAX_TID - 1);
        CacheThread {
            ip: ip.to_string(),
            tid,
            base_offset,
        }
    }

    pub fn ip(&self) -> &str {
        &self.ip
    }

    pub fn tid(&self) -> u32 {
        self.tid
    }

    pub fn cache_update_connect_address(&self) -> Address {
        format!(
            "tcp://{}:{}",
            self.ip,
            self.tid + CACHE_UPDATE_PORT + self.base_offset
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_thread_addresses_match_cpp() {
        // C++ with base_offset=0, tid=0: key_request = tcp://1.2.3.4:6200
        let st = ServerThread::new("1.2.3.4", "10.0.0.1", 0, 0);
        assert_eq!(st.key_request_connect_address(), "tcp://1.2.3.4:6200");
        assert_eq!(st.gossip_connect_address(), "tcp://10.0.0.1:6250");
        assert_eq!(st.node_join_connect_address(), "tcp://10.0.0.1:6000");

        // With tid=2, base_offset=100
        let st2 = ServerThread::new("1.2.3.4", "10.0.0.1", 2, 100);
        assert_eq!(st2.key_request_connect_address(), "tcp://1.2.3.4:6302");
        assert_eq!(st2.gossip_connect_address(), "tcp://10.0.0.1:6352");
    }

    #[test]
    fn server_thread_id() {
        let st = ServerThread::new("1.2.3.4", "10.0.0.1", 3, 0);
        assert_eq!(st.id(), "10.0.0.1:3");
    }

    #[test]
    fn server_thread_virtual_id() {
        let st = ServerThread::with_virtual("1.2.3.4", "10.0.0.1", 2, 5, 0);
        assert_eq!(st.virtual_id(), "10.0.0.1:2_5");
    }

    #[test]
    fn routing_thread_addresses() {
        let rt = RoutingThread::new("10.0.0.1", 0, 0);
        assert_eq!(rt.seed_connect_address(), "tcp://10.0.0.1:6350");
        assert_eq!(rt.key_address_connect_address(), "tcp://10.0.0.1:6450");
        assert_eq!(rt.notify_connect_address(), "tcp://10.0.0.1:6400");
    }

    #[test]
    fn monitoring_thread_addresses() {
        let mt = MonitoringThread::new("10.0.0.1", 0);
        assert_eq!(mt.notify_connect_address(), "tcp://10.0.0.1:6950");
        assert_eq!(mt.response_connect_address(), "tcp://10.0.0.1:6951");
        assert_eq!(mt.depart_done_connect_address(), "tcp://10.0.0.1:6952");
        assert_eq!(mt.feedback_report_connect_address(), "tcp://10.0.0.1:6953");
    }

    #[test]
    fn monitoring_thread_with_offset() {
        let mt = MonitoringThread::new("10.0.0.1", 100);
        assert_eq!(mt.notify_connect_address(), "tcp://10.0.0.1:7050");
    }

    #[test]
    fn server_thread_equality() {
        let a = ServerThread::new("1.2.3.4", "10.0.0.1", 0, 0);
        let b = ServerThread::new("1.2.3.4", "10.0.0.1", 0, 0);
        let c = ServerThread::new("1.2.3.4", "10.0.0.1", 1, 0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    #[should_panic(expected = "tid 50 exceeds maximum")]
    fn server_thread_tid_too_large() {
        ServerThread::new("1.2.3.4", "10.0.0.1", 50, 0);
    }

    #[test]
    fn cache_thread_update_address() {
        let ct = CacheThread::new("10.0.0.1", 0, 0);
        assert_eq!(ct.cache_update_connect_address(), "tcp://10.0.0.1:6850");
    }

    #[should_panic(expected = "tid 50 exceeds maximum")]
    fn routing_thread_tid_too_large() {
        RoutingThread::new("10.0.0.1", 50, 0);
    }

    #[test]
    fn max_valid_tid() {
        // tid=49 is the maximum valid value.
        let st = ServerThread::new("1.2.3.4", "10.0.0.1", 49, 0);
        assert_eq!(st.tid(), 49);
    }
}
