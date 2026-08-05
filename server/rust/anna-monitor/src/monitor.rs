//! Main monitoring event loop.
//!
//! Mirrors `server/cpp/src/monitor/monitoring.cpp`.

use anna_server_common::config::Config;
use anna_server_common::hash_ring::ConsistentHashRing;
use anna_server_common::metadata::Tier;
use anna_server_common::signal;
use anna_server_common::threads::MonitoringThread;
use anna_server_common::types::Address;
use log::{error, info, warn};
use std::collections::HashMap;
use std::time::Instant;

use crate::handlers;
use crate::policies;
use crate::stats;
use crate::types::*;

/// Run the monitoring event loop.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    signal::install_shutdown_handler();

    let base_offset = config.ports.base_offset;
    let monitoring_ip = config.monitoring.ip.clone();
    let scaling_alert_ip = config.monitoring.scaling_alert_ip.clone();

    // Parse monitor parameters from config.
    let params = MonitorParams {
        monitoring_threshold_s: config.timings.monitoring_timeout.max(1),
        grace_period_s: config.timings.grace_period,
        monitoring_response_timeout_ms: if config.timings.monitoring_response_timeout_ms > 0 {
            config.timings.monitoring_response_timeout_ms
        } else {
            10000
        },
        enable_elasticity: config.policy.elasticity,
        enable_tiering: config.policy.tiering,
        enable_selective_rep: config.policy.selective_rep,
        ..Default::default()
    };

    let memory_thread_count = if config.threads.memory > 0 {
        config.threads.memory
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1)
    };
    let disk_thread_count = if config.threads.disk > 0 {
        config.threads.disk
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1)
    };

    let memory_node_capacity = config.memory_capacity_bytes() / 1024; // in KB
    let disk_node_capacity = config.disk_capacity_bytes() / 1024;

    let virtual_nodes = 3000u32; // default, could be from config

    let mt = MonitoringThread::new(&monitoring_ip, base_offset);

    info!("Elasticity policy enabled: {}", params.enable_elasticity);
    info!("Tiering policy enabled: {}", params.enable_tiering);
    info!(
        "Selective replication policy enabled: {}",
        params.enable_selective_rep
    );

    // ── State ───────────────────────────────────────────────────────

    let mut global_hash_rings: HashMap<Tier, ConsistentHashRing> = HashMap::new();
    global_hash_rings.insert(Tier::Memory, ConsistentHashRing::new());
    global_hash_rings.insert(Tier::Disk, ConsistentHashRing::new());

    let mut routing_ips: Vec<Address> = Vec::new();
    let mut memory_storage = StorageStats::new();
    let mut disk_storage = StorageStats::new();
    let mut memory_occupancy = OccupancyStats::new();
    let mut disk_occupancy = OccupancyStats::new();
    let mut memory_accesses = AccessStats::new();
    let mut disk_accesses = AccessStats::new();
    let mut key_access_frequency = KeyAccessFrequency::new();
    let mut key_access_summary = KeyAccessSummary::new();
    let mut key_size = KeySizeMap::new();
    let mut ss = SummaryStats::default();
    let mut user_latency: HashMap<String, f64> = HashMap::new();
    let mut user_throughput: HashMap<String, f64> = HashMap::new();
    let mut latency_miss_ratio_map: HashMap<String, (f64, u32)> = HashMap::new();
    let mut departing_node_map: HashMap<Address, u32> = HashMap::new();
    let mut new_memory_count = 0u32;
    let mut new_disk_count = 0u32;
    let mut removing_memory_node = false;
    let mut removing_disk_node = false;
    let mut grace_start = Instant::now();
    let mut report_start = Instant::now();
    let mut _epoch = 0u32;
    let mut last_epoch_change: HashMap<Address, Instant> = HashMap::new();

    info!(
        "Monitor listening on {} (base_offset={})",
        monitoring_ip, base_offset
    );

    // ── Event loop ──────────────────────────────────────────────────
    // TODO: integrate ZMQ sockets (notify_puller, depart_done_puller,
    // feedback_puller, response_puller, pushers) when omq-tokio is
    // wired up. For now, the structure is in place.

    while !signal::shutdown_requested() {
        // TODO: ZMQ poll with 0ms timeout for non-blocking check
        // pollitems[0] = notify_puller → membership_handler
        // pollitems[1] = depart_done_puller → depart_done_handler
        // pollitems[2] = feedback_puller → feedback_handler

        // Periodic monitoring cycle.
        let elapsed = report_start.elapsed().as_secs() as u32;
        if elapsed >= params.monitoring_threshold_s {
            _epoch += 1;

            let memory_node_count = global_hash_rings
                .get(&Tier::Memory)
                .map(|r| r.size() as u32 / virtual_nodes)
                .unwrap_or(0);
            let disk_node_count = global_hash_rings
                .get(&Tier::Disk)
                .map(|r| r.size() as u32 / virtual_nodes)
                .unwrap_or(0);

            // Clear per-cycle state.
            key_access_frequency.clear();
            key_access_summary.clear();
            memory_storage.clear();
            disk_storage.clear();
            memory_occupancy.clear();
            disk_occupancy.clear();
            memory_accesses.clear();
            disk_accesses.clear();

            // TODO: collect_internal_stats (requires ZMQ)

            // Crash detection.
            let stale_threshold =
                std::time::Duration::from_secs(params.monitoring_threshold_s as u64);
            let mut dead_nodes = Vec::new();
            for (ip_pair, last_seen) in &last_epoch_change {
                if last_seen.elapsed() > stale_threshold {
                    dead_nodes.push(ip_pair.clone());
                }
            }
            for ip_pair in &dead_nodes {
                warn!("Detected crashed node: {}", ip_pair);
                last_epoch_change.remove(ip_pair);
                // TODO: remove from hash ring, notify routing
            }

            // Compute summary stats.
            stats::compute_summary_stats(
                &mut ss,
                &memory_storage,
                &disk_storage,
                &memory_occupancy,
                &disk_occupancy,
                &memory_accesses,
                &disk_accesses,
                &key_access_frequency,
                &mut key_access_summary,
                memory_node_capacity,
                disk_node_capacity,
                params.max_memory_node_consumption,
                params.max_disk_node_consumption,
            );

            stats::collect_external_stats(&mut ss, &user_latency, &user_throughput);

            let grace_elapsed = grace_start.elapsed().as_secs() as u32 >= params.grace_period_s;

            // Run policies.
            policies::storage_policy(
                &ss,
                &params,
                memory_node_count,
                disk_node_count,
                &mut new_memory_count,
                &mut new_disk_count,
                &mut removing_disk_node,
                grace_elapsed,
            );

            policies::movement_policy(
                &ss,
                &params,
                &key_access_summary,
                &key_size,
                memory_node_count,
                &mut new_memory_count,
                grace_elapsed,
            );

            policies::slo_policy(
                &ss,
                &params,
                memory_node_count,
                &mut new_memory_count,
                &mut removing_memory_node,
                grace_elapsed,
            );

            // Clear feedback maps.
            user_latency.clear();
            user_throughput.clear();
            latency_miss_ratio_map.clear();

            report_start = Instant::now();
        }

        // Yield to avoid busy-spinning.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    info!("Monitor shutting down");
    let _ = (
        mt,
        scaling_alert_ip,
        departing_node_map,
        key_size,
        dead_nodes_placeholder(),
    );
    Ok(())
}

fn dead_nodes_placeholder() {}
