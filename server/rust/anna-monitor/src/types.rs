//! Monitor-specific types.
//!
//! Mirrors `server/cpp/src/kvs/kvs_types.hpp` and `monitoring_utils.hpp`.

use anna_server_common::types::{Address, Key};
use std::collections::HashMap;

/// Per-node per-thread storage consumption in bytes.
/// Key: "public_ip/private_ip", Value: thread_id -> bytes.
pub type StorageStats = HashMap<Address, HashMap<u32, u64>>;

/// Per-node per-thread occupancy.
/// Key: "public_ip/private_ip", Value: thread_id -> (occupancy_ratio, epoch).
pub type OccupancyStats = HashMap<Address, HashMap<u32, (f64, u32)>>;

/// Per-node per-thread access count.
/// Key: "public_ip/private_ip", Value: thread_id -> access_count.
pub type AccessStats = HashMap<Address, HashMap<u32, u32>>;

/// Summary statistics aggregated from all nodes in a monitoring cycle.
#[derive(Debug, Clone, Default)]
pub struct SummaryStats {
    pub key_access_mean: f64,
    pub key_access_std: f64,
    pub total_memory_access: u64,
    pub total_disk_access: u64,
    pub total_memory_consumption: u64,
    pub total_disk_consumption: u64,
    pub max_memory_consumption_percentage: f64,
    pub max_disk_consumption_percentage: f64,
    pub avg_memory_consumption_percentage: f64,
    pub avg_disk_consumption_percentage: f64,
    pub required_memory_node: u32,
    pub required_disk_node: u32,
    pub max_memory_occupancy: f64,
    pub min_memory_occupancy: f64,
    pub avg_memory_occupancy: f64,
    pub max_disk_occupancy: f64,
    pub min_disk_occupancy: f64,
    pub avg_disk_occupancy: f64,
    pub min_occupancy_memory_public_ip: Address,
    pub min_occupancy_memory_private_ip: Address,
    pub avg_latency: f64,
    pub total_throughput: f64,
}

impl SummaryStats {
    pub fn clear(&mut self) {
        *self = Self {
            min_memory_occupancy: 1.0,
            min_disk_occupancy: 1.0,
            ..Default::default()
        };
    }
}

/// Monitor-wide tunable constants, parsed from config.
#[derive(Debug, Clone)]
pub struct MonitorParams {
    pub monitoring_threshold_s: u32,
    pub grace_period_s: u32,
    pub monitoring_response_timeout_ms: u32,
    pub node_addition_batch_size: u32,
    pub max_memory_node_consumption: f64,
    pub min_memory_node_consumption: f64,
    pub max_disk_node_consumption: f64,
    pub min_disk_node_consumption: f64,
    pub key_promotion_threshold: u32,
    pub key_demotion_threshold: u32,
    pub min_memory_tier_size: u32,
    pub min_disk_tier_size: u32,
    pub assumed_value_size_kb: u32,
    pub slo_occupancy_upper: f64,
    pub slo_occupancy_lower: f64,
    pub slo_worst_us: u32,
    pub warmup_key_count: u32,
    pub enable_elasticity: bool,
    pub enable_tiering: bool,
    pub enable_selective_rep: bool,
}

impl Default for MonitorParams {
    fn default() -> Self {
        Self {
            monitoring_threshold_s: 30,
            grace_period_s: 120,
            monitoring_response_timeout_ms: 10000,
            node_addition_batch_size: 2,
            max_memory_node_consumption: 0.6,
            min_memory_node_consumption: 0.3,
            max_disk_node_consumption: 0.75,
            min_disk_node_consumption: 0.5,
            key_promotion_threshold: 0,
            key_demotion_threshold: 1,
            min_memory_tier_size: 1,
            min_disk_tier_size: 0,
            assumed_value_size_kb: 256,
            slo_occupancy_upper: 0.15,
            slo_occupancy_lower: 0.05,
            slo_worst_us: 3000,
            warmup_key_count: 99_999_999,
            enable_elasticity: false,
            enable_tiering: false,
            enable_selective_rep: false,
        }
    }
}

/// Per-key access frequency: key -> (address:tid -> access_count).
pub type KeyAccessFrequency = HashMap<Key, HashMap<String, u32>>;

/// Per-key total access summary.
pub type KeyAccessSummary = HashMap<Key, u32>;

/// Per-key value size in KB.
pub type KeySizeMap = HashMap<Key, u32>;
