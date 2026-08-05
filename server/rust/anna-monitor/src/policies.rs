//! Policy engines: storage, movement, and SLO.
//!
//! Mirrors `server/cpp/src/monitor/storage_policy.cpp`,
//! `movement_policy.cpp`, and `slo_policy.cpp`.

use crate::types::*;
use log::info;

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
/// Only active when tiering is enabled.
pub fn movement_policy(
    ss: &SummaryStats,
    params: &MonitorParams,
    key_access_summary: &KeyAccessSummary,
    _key_size: &KeySizeMap,
    memory_node_count: u32,
    _new_memory_count: &mut u32,
    grace_elapsed: bool,
) {
    if !params.enable_tiering {
        return;
    }

    // Count hot keys that should be promoted to memory.
    let hot_keys: Vec<&String> = key_access_summary
        .iter()
        .filter(|(_, &count)| count > params.key_promotion_threshold)
        .map(|(key, _)| key)
        .collect();

    if !hot_keys.is_empty() {
        info!(
            "Movement policy: {} hot keys above promotion threshold",
            hot_keys.len()
        );
        // TODO: change_replication_factor for hot keys (requires replication_helpers port)
    }

    // Count cold keys that should be demoted to disk.
    let cold_keys: Vec<&String> = key_access_summary
        .iter()
        .filter(|(_, &count)| count < params.key_demotion_threshold)
        .map(|(key, _)| key)
        .collect();

    if !cold_keys.is_empty() {
        info!(
            "Movement policy: {} cold keys below demotion threshold",
            cold_keys.len()
        );
        // TODO: change_replication_factor for cold keys (requires replication_helpers port)
    }

    // Selective replication reduction.
    if params.enable_selective_rep {
        let cool_keys: Vec<&String> = key_access_summary
            .iter()
            .filter(|(_, &count)| (count as f64) <= ss.key_access_mean)
            .map(|(key, _)| key)
            .collect();

        if !cool_keys.is_empty() {
            info!(
                "Movement policy: {} keys below mean for replication reduction",
                cool_keys.len()
            );
            // TODO: change_replication_factor to reduce (requires replication_helpers port)
        }
    }

    let _ = (memory_node_count, grace_elapsed);
}

/// SLO-based scaling policy.
///
/// Scales nodes based on latency SLO violations and occupancy.
pub fn slo_policy(
    ss: &SummaryStats,
    params: &MonitorParams,
    memory_node_count: u32,
    new_memory_count: &mut u32,
    _removing_memory_node: &mut bool,
    grace_elapsed: bool,
) {
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
            // Scaling alert sent by monitor loop after policies run.
        }

        if params.enable_selective_rep {
            info!("SLO policy: would increase replication for hot keys");
            // TODO: selective replication increase (requires replication_helpers port)
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
        // Node removal initiated by monitor loop after policies run.
    }
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
        let ss = default_ss(); // avg_latency = 0
        let params = MonitorParams::default();
        let mut nmc = 0u32;
        let mut rmn = false;

        slo_policy(&ss, &params, 1, &mut nmc, &mut rmn, true);

        assert_eq!(nmc, 0);
    }

    #[test]
    fn slo_policy_scales_up_on_latency_violation() {
        let mut ss = default_ss();
        ss.avg_latency = 6000.0; // 2x SLO
        ss.min_memory_occupancy = 0.5; // above upper threshold

        let params = MonitorParams {
            enable_elasticity: true,
            slo_worst_us: 3000,
            slo_occupancy_upper: 0.15,
            ..Default::default()
        };
        let mut nmc = 0u32;
        let mut rmn = false;

        slo_policy(&ss, &params, 2, &mut nmc, &mut rmn, true);

        assert!(nmc > 0, "should scale up on latency violation");
    }
}
