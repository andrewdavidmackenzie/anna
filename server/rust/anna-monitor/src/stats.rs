//! Statistics collection and aggregation.
//!
//! Mirrors `server/cpp/src/monitor/stats_helpers.cpp`.

use crate::types::*;
use anna_server_common::metadata::Tier;
use std::collections::HashMap;

/// Compute summary statistics from raw per-node stats.
///
/// Mirrors `compute_summary_stats()` in C++.
pub fn compute_summary_stats(
    ss: &mut SummaryStats,
    memory_storage: &StorageStats,
    disk_storage: &StorageStats,
    memory_occupancy: &OccupancyStats,
    disk_occupancy: &OccupancyStats,
    memory_accesses: &AccessStats,
    disk_accesses: &AccessStats,
    key_access_frequency: &KeyAccessFrequency,
    key_access_summary: &mut KeyAccessSummary,
    memory_node_capacity: u64,
    disk_node_capacity: u64,
    max_memory_consumption: f64,
    max_disk_consumption: f64,
) {
    ss.clear();

    // Key access statistics using Welford's online algorithm.
    let mut n: u64 = 0;
    let mut mean = 0.0f64;
    let mut m2 = 0.0f64;

    for (key, freq_map) in key_access_frequency {
        let total: u32 = freq_map.values().sum();
        if total > 0 {
            key_access_summary.insert(key.clone(), total);
            n += 1;
            let delta = total as f64 - mean;
            mean += delta / n as f64;
            let delta2 = total as f64 - mean;
            m2 += delta * delta2;
        }
    }

    ss.key_access_mean = mean;
    ss.key_access_std = if n > 1 {
        (m2 / (n - 1) as f64).sqrt()
    } else {
        0.0
    };

    // Tier access totals.
    ss.total_memory_access = memory_accesses
        .values()
        .flat_map(|m| m.values())
        .map(|&v| v as u64)
        .sum();
    ss.total_disk_access = disk_accesses
        .values()
        .flat_map(|m| m.values())
        .map(|&v| v as u64)
        .sum();

    // Memory storage consumption.
    compute_storage_stats(
        memory_storage,
        memory_node_capacity,
        max_memory_consumption,
        &mut ss.total_memory_consumption,
        &mut ss.max_memory_consumption_percentage,
        &mut ss.avg_memory_consumption_percentage,
        &mut ss.required_memory_node,
    );

    // Disk storage consumption.
    compute_storage_stats(
        disk_storage,
        disk_node_capacity,
        max_disk_consumption,
        &mut ss.total_disk_consumption,
        &mut ss.max_disk_consumption_percentage,
        &mut ss.avg_disk_consumption_percentage,
        &mut ss.required_disk_node,
    );

    // Memory occupancy.
    compute_occupancy_stats(
        memory_occupancy,
        &mut ss.max_memory_occupancy,
        &mut ss.min_memory_occupancy,
        &mut ss.avg_memory_occupancy,
        &mut ss.min_occupancy_memory_public_ip,
        &mut ss.min_occupancy_memory_private_ip,
    );

    // Disk occupancy.
    compute_occupancy_stats(
        disk_occupancy,
        &mut ss.max_disk_occupancy,
        &mut ss.min_disk_occupancy,
        &mut ss.avg_disk_occupancy,
        &mut String::new(), // disk doesn't track min-occupancy IP
        &mut String::new(),
    );
}

fn compute_storage_stats(
    storage: &StorageStats,
    node_capacity: u64,
    max_consumption: f64,
    total: &mut u64,
    max_pct: &mut f64,
    avg_pct: &mut f64,
    required_nodes: &mut u32,
) {
    let mut node_count = 0u32;
    *total = 0;
    *max_pct = 0.0;

    for threads in storage.values() {
        let node_total: u64 = threads.values().sum();
        *total += node_total;
        node_count += 1;

        if node_capacity > 0 {
            let pct = node_total as f64 / node_capacity as f64;
            if pct > *max_pct {
                *max_pct = pct;
            }
        }
    }

    if node_count > 0 && node_capacity > 0 {
        *avg_pct = *total as f64 / (node_count as f64 * node_capacity as f64);
    }

    if node_capacity > 0 && max_consumption > 0.0 {
        let effective_capacity = (max_consumption * node_capacity as f64) as u64;
        if effective_capacity > 0 {
            *required_nodes = ((*total + effective_capacity - 1) / effective_capacity) as u32;
        }
    }
}

fn compute_occupancy_stats(
    occupancy: &OccupancyStats,
    max_occ: &mut f64,
    min_occ: &mut f64,
    avg_occ: &mut f64,
    min_public_ip: &mut String,
    min_private_ip: &mut String,
) {
    let mut node_count = 0u32;
    let mut total_occ = 0.0f64;

    for (ip_pair, threads) in occupancy {
        if threads.is_empty() {
            continue;
        }
        let node_avg: f64 =
            threads.values().map(|(occ, _)| occ).sum::<f64>() / threads.len() as f64;

        if node_avg > *max_occ {
            *max_occ = node_avg;
        }
        if node_avg < *min_occ {
            *min_occ = node_avg;
            // Extract public/private IP from "public/private" format.
            let parts: Vec<&str> = ip_pair.split('/').collect();
            if parts.len() == 2 {
                *min_public_ip = parts[0].to_string();
                *min_private_ip = parts[1].to_string();
            }
        }

        total_occ += node_avg;
        node_count += 1;
    }

    if node_count > 0 {
        *avg_occ = total_occ / node_count as f64;
    }
}

/// Compute external stats from user feedback.
///
/// Mirrors `collect_external_stats()` in C++.
pub fn collect_external_stats(
    ss: &mut SummaryStats,
    user_latency: &HashMap<String, f64>,
    user_throughput: &HashMap<String, f64>,
) {
    if !user_latency.is_empty() {
        ss.avg_latency = user_latency.values().sum::<f64>() / user_latency.len() as f64;
    }
    ss.total_throughput = user_throughput.values().sum();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_summary_empty() {
        let mut ss = SummaryStats::default();
        let mut kas = KeyAccessSummary::new();

        compute_summary_stats(
            &mut ss,
            &StorageStats::new(),
            &StorageStats::new(),
            &OccupancyStats::new(),
            &OccupancyStats::new(),
            &AccessStats::new(),
            &AccessStats::new(),
            &KeyAccessFrequency::new(),
            &mut kas,
            1_000_000,
            1_000_000,
            0.6,
            0.75,
        );

        assert_eq!(ss.total_memory_consumption, 0);
        assert_eq!(ss.required_memory_node, 0);
    }

    #[test]
    fn compute_summary_with_data() {
        let mut ss = SummaryStats::default();
        let mut kas = KeyAccessSummary::new();

        let mut ms = StorageStats::new();
        let mut node = HashMap::new();
        node.insert(0, 500_000u64);
        ms.insert("10.0.0.1/10.0.0.1".into(), node);

        let mut mo = OccupancyStats::new();
        let mut occ = HashMap::new();
        occ.insert(0, (0.5, 1));
        mo.insert("10.0.0.1/10.0.0.1".into(), occ);

        let mut ma = AccessStats::new();
        let mut acc = HashMap::new();
        acc.insert(0, 100);
        ma.insert("10.0.0.1/10.0.0.1".into(), acc);

        let mut kaf = KeyAccessFrequency::new();
        let mut freq = HashMap::new();
        freq.insert("10.0.0.1/10.0.0.1:0".into(), 50);
        kaf.insert("key1".into(), freq);

        compute_summary_stats(
            &mut ss,
            &ms,
            &StorageStats::new(),
            &mo,
            &OccupancyStats::new(),
            &ma,
            &AccessStats::new(),
            &kaf,
            &mut kas,
            1_000_000,
            1_000_000,
            0.6,
            0.75,
        );

        assert_eq!(ss.total_memory_consumption, 500_000);
        assert_eq!(ss.total_memory_access, 100);
        assert_eq!(ss.max_memory_occupancy, 0.5);
        assert_eq!(*kas.get("key1").unwrap(), 50);
    }

    #[test]
    fn external_stats_averages() {
        let mut ss = SummaryStats::default();
        let mut ul = HashMap::new();
        ul.insert("u1".into(), 1000.0);
        ul.insert("u2".into(), 2000.0);
        let mut ut = HashMap::new();
        ut.insert("u1".into(), 50.0);
        ut.insert("u2".into(), 100.0);

        collect_external_stats(&mut ss, &ul, &ut);

        assert_eq!(ss.avg_latency, 1500.0);
        assert_eq!(ss.total_throughput, 150.0);
    }
}
