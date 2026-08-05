//! Policy engines: storage, movement, and SLO.
//!
//! Mirrors `server/cpp/src/monitor/storage_policy.cpp`,
//! `movement_policy.cpp`, and `slo_policy.cpp`.

use crate::types::*;
use anna_server_common::metadata::KeyReplication;
use anna_server_common::types::Key;
use log::info;
use std::collections::HashMap;

/// Storage-based scaling policy.
///
/// Adds nodes when storage exceeds upper threshold, removes nodes
/// when below lower threshold.
pub fn storage_policy(
    ss: &SummaryStats,
    params: &MonitorParams,
    memory_node_count: u32,
    disk_node_count: u32,
    new_memory_count: &mut u32,
    new_disk_count: &mut u32,
    removing_disk_node: &mut bool,
    grace_elapsed: bool,
) {
    if !params.enable_elasticity {
        return;
    }

    // Scale up memory if required nodes exceed current.
    if *new_memory_count == 0 && ss.required_memory_node > memory_node_count && grace_elapsed {
        let to_add = params.node_addition_batch_size;
        info!(
            "Storage policy: adding {} memory nodes (required={}, current={})",
            to_add, ss.required_memory_node, memory_node_count
        );
        *new_memory_count = to_add;
    }

    // Scale up disk.
    if params.enable_tiering
        && *new_disk_count == 0
        && ss.required_disk_node > disk_node_count
        && grace_elapsed
    {
        let to_add = params.node_addition_batch_size;
        info!(
            "Storage policy: adding {} disk nodes (required={}, current={})",
            to_add, ss.required_disk_node, disk_node_count
        );
        *new_disk_count = to_add;
    }

    // Scale down disk if under-utilized.
    if params.enable_tiering
        && !*removing_disk_node
        && disk_node_count > 0
        && ss.avg_disk_consumption_percentage < params.min_disk_node_consumption
        && disk_node_count > ss.required_disk_node.max(params.min_disk_tier_size)
        && grace_elapsed
    {
        info!(
            "Storage policy: removing a disk node (avg consumption {:.2}%)",
            ss.avg_disk_consumption_percentage * 100.0
        );
        *removing_disk_node = true;
        // Node removal initiated by monitor loop after policies run.
    }
}

/// Tier movement policy (promote/demote keys between memory and disk).
///
/// Only active when tiering is enabled. Returns replication change
/// requests for the monitor loop to execute.
pub fn movement_policy(
    ss: &SummaryStats,
    params: &MonitorParams,
    key_access_summary: &KeyAccessSummary,
    key_replication_map: &HashMap<Key, KeyReplication>,
    _key_size: &KeySizeMap,
    _memory_node_count: u32,
    _new_memory_count: &mut u32,
    _grace_elapsed: bool,
) -> HashMap<Key, KeyReplication> {
    let mut requests = HashMap::new();

    if !params.enable_tiering {
        return requests;
    }

    // Promote hot keys to memory.
    for (key, &count) in key_access_summary {
        if count > params.key_promotion_threshold {
            if let Some(rep) = key_replication_map.get(key) {
                let mem_rep = rep
                    .global_replication
                    .get(&anna_server_common::metadata::Tier::Memory)
                    .copied()
                    .unwrap_or(0);
                if mem_rep == 0 {
                    let mut new_rep = rep.clone();
                    new_rep
                        .global_replication
                        .insert(anna_server_common::metadata::Tier::Memory, 1);
                    let disk_rep = rep
                        .global_replication
                        .get(&anna_server_common::metadata::Tier::Disk)
                        .copied()
                        .unwrap_or(1);
                    if disk_rep > 0 {
                        new_rep
                            .global_replication
                            .insert(anna_server_common::metadata::Tier::Disk, disk_rep - 1);
                    }
                    requests.insert(key.clone(), new_rep);
                }
            }
        }
    }

    // Demote cold keys to disk.
    for (key, &count) in key_access_summary {
        if count < params.key_demotion_threshold {
            if let Some(rep) = key_replication_map.get(key) {
                let mem_rep = rep
                    .global_replication
                    .get(&anna_server_common::metadata::Tier::Memory)
                    .copied()
                    .unwrap_or(0);
                if mem_rep > 0 {
                    requests.insert(
                        key.clone(),
                        crate::replication::create_new_replication(0, 1, 1, 1),
                    );
                }
            }
        }
    }

    // Selective replication reduction.
    if params.enable_selective_rep {
        for (key, &count) in key_access_summary {
            if (count as f64) <= ss.key_access_mean {
                requests.insert(
                    key.clone(),
                    crate::replication::create_new_replication(1, 0, 1, 1),
                );
            }
        }
    }

    if !requests.is_empty() {
        info!(
            "Movement policy: {} replication change(s) requested",
            requests.len()
        );
    }

    requests
}

/// SLO-based scaling policy.
///
/// Scales nodes based on latency SLO violations and occupancy.
/// Returns replication change requests for the monitor loop.
pub fn slo_policy(
    ss: &SummaryStats,
    params: &MonitorParams,
    key_access_summary: &KeyAccessSummary,
    latency_miss_ratio_map: &HashMap<String, (f64, u32)>,
    key_replication_map: &HashMap<Key, KeyReplication>,
    memory_node_count: u32,
    new_memory_count: &mut u32,
    _removing_memory_node: &mut bool,
    grace_elapsed: bool,
) -> HashMap<Key, KeyReplication> {
    let mut requests = HashMap::new();

    // Branch 1: Latency SLO violated.
    if ss.avg_latency > params.slo_worst_us as f64 && *new_memory_count == 0 {
        if params.enable_elasticity
            && ss.min_memory_occupancy > params.slo_occupancy_upper
            && grace_elapsed
        {
            let ratio = ss.avg_latency / params.slo_worst_us as f64;
            let nodes_to_add = ((ratio - 1.0) * memory_node_count as f64).ceil() as u32;
            let nodes_to_add = nodes_to_add.max(1);
            info!(
                "SLO policy: adding {} memory nodes (latency {:.0}us > SLO {}us)",
                nodes_to_add, ss.avg_latency, params.slo_worst_us
            );
            *new_memory_count = nodes_to_add;
        }

        // Selective replication increase for hot keys with high latency.
        if params.enable_selective_rep {
            let threshold = ss.key_access_mean + ss.key_access_std;
            for (key, &count) in key_access_summary {
                if (count as f64) > threshold {
                    if let Some((ratio, _)) = latency_miss_ratio_map.get(key) {
                        if let Some(rep) = key_replication_map.get(key) {
                            let mem_rep = rep
                                .global_replication
                                .get(&anna_server_common::metadata::Tier::Memory)
                                .copied()
                                .unwrap_or(1);
                            let target = ((mem_rep as f64) * ratio).ceil() as u32;
                            let target = target.max(mem_rep + 1).min(memory_node_count);
                            if target > mem_rep {
                                let mut new_rep = rep.clone();
                                new_rep
                                    .global_replication
                                    .insert(anna_server_common::metadata::Tier::Memory, target);
                                requests.insert(key.clone(), new_rep);
                            }
                        }
                    }
                }
            }
            if !requests.is_empty() {
                info!(
                    "SLO policy: {} selective replication increase(s)",
                    requests.len()
                );
            }
        }
    }

    // Branch 2: Under-utilized, can shrink.
    if params.enable_elasticity
        && ss.min_memory_occupancy < params.slo_occupancy_lower
        && memory_node_count > ss.required_memory_node.max(params.min_memory_tier_size)
        && grace_elapsed
    {
        info!(
            "SLO policy: removing memory node (min occupancy {:.2}%)",
            ss.min_memory_occupancy * 100.0
        );
    }

    requests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ss() -> SummaryStats {
        SummaryStats::default()
    }

    #[test]
    fn storage_policy_noop_when_disabled() {
        let ss = default_ss();
        let params = MonitorParams {
            enable_elasticity: false,
            ..Default::default()
        };
        let mut nmc = 0u32;
        let mut ndc = 0u32;
        let mut rdn = false;

        storage_policy(&ss, &params, 1, 0, &mut nmc, &mut ndc, &mut rdn, true);

        assert_eq!(nmc, 0);
        assert_eq!(ndc, 0);
    }

    #[test]
    fn storage_policy_scales_up_memory() {
        let mut ss = default_ss();
        ss.required_memory_node = 3;
        let params = MonitorParams {
            enable_elasticity: true,
            node_addition_batch_size: 2,
            ..Default::default()
        };
        let mut nmc = 0u32;
        let mut ndc = 0u32;
        let mut rdn = false;

        storage_policy(&ss, &params, 1, 0, &mut nmc, &mut ndc, &mut rdn, true);

        assert_eq!(nmc, 2);
    }

    #[test]
    fn storage_policy_respects_grace_period() {
        let mut ss = default_ss();
        ss.required_memory_node = 3;
        let params = MonitorParams {
            enable_elasticity: true,
            ..Default::default()
        };
        let mut nmc = 0u32;
        let mut ndc = 0u32;
        let mut rdn = false;

        // Grace period not elapsed.
        storage_policy(&ss, &params, 1, 0, &mut nmc, &mut ndc, &mut rdn, false);

        assert_eq!(nmc, 0);
    }

    #[test]
    fn slo_policy_noop_when_latency_ok() {
        let ss = default_ss();
        let params = MonitorParams::default();
        let mut nmc = 0u32;
        let mut rmn = false;
        let kas = KeyAccessSummary::new();
        let lmr = HashMap::new();
        let krm = HashMap::new();

        let reqs = slo_policy(&ss, &params, &kas, &lmr, &krm, 1, &mut nmc, &mut rmn, true);

        assert_eq!(nmc, 0);
        assert!(reqs.is_empty());
    }

    #[test]
    fn slo_policy_scales_up_on_latency_violation() {
        let mut ss = default_ss();
        ss.avg_latency = 6000.0;
        ss.min_memory_occupancy = 0.5;

        let params = MonitorParams {
            enable_elasticity: true,
            slo_worst_us: 3000,
            slo_occupancy_upper: 0.15,
            ..Default::default()
        };
        let mut nmc = 0u32;
        let mut rmn = false;
        let kas = KeyAccessSummary::new();
        let lmr = HashMap::new();
        let krm = HashMap::new();

        slo_policy(&ss, &params, &kas, &lmr, &krm, 2, &mut nmc, &mut rmn, true);

        assert!(nmc > 0, "should scale up on latency violation");
    }
}
