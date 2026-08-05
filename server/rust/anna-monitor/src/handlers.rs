//! Message handlers for the monitoring event loop.
//!
//! Mirrors `server/cpp/src/monitor/membership_handler.cpp`,
//! `depart_done_handler.cpp`, and `feedback_handler.cpp`.

use anna_server_common::hash_ring::ConsistentHashRing;
use anna_server_common::metadata::Tier;
use anna_server_common::threads::MonitoringThread;
use anna_server_common::types::Address;
use log::{error, info, warn};
use std::collections::HashMap;
use std::time::Instant;

use crate::types::*;

/// Process a membership notification (join or depart).
///
/// Message format: "type:TIER:public_ip:private_ip"
/// where type is "join" or "depart", TIER is "MEMORY", "DISK", or "ROUTING".
pub fn membership_handler(
    msg: &str,
    global_hash_rings: &mut HashMap<Tier, ConsistentHashRing>,
    routing_ips: &mut Vec<Address>,
    memory_storage: &mut StorageStats,
    disk_storage: &mut StorageStats,
    memory_occupancy: &mut OccupancyStats,
    disk_occupancy: &mut OccupancyStats,
    key_access_frequency: &mut KeyAccessFrequency,
    new_memory_count: &mut u32,
    new_disk_count: &mut u32,
    grace_start: &mut Instant,
    memory_thread_count: u32,
    disk_thread_count: u32,
    virtual_nodes: u32,
    base_offset: u32,
) {
    let parts: Vec<&str> = msg.split(':').collect();
    if parts.len() < 4 {
        error!("Invalid membership message: {}", msg);
        return;
    }

    let action = parts[0];
    let tier_name = parts[1];
    let public_ip = parts[2];
    let private_ip = parts[3];

    let tier = match tier_name {
        "MEMORY" => Some(Tier::Memory),
        "DISK" => Some(Tier::Disk),
        "ROUTING" => None, // routing isn't a storage tier
        _ => {
            error!("Unknown tier in membership message: {}", tier_name);
            return;
        }
    };

    match action {
        "join" => {
            info!(
                "Received join from server {}/{} in tier {}",
                public_ip, private_ip, tier_name
            );
            if let Some(t) = tier {
                let ring = global_hash_rings.entry(t).or_default();
                ring.insert(public_ip, private_ip, 0, base_offset, virtual_nodes, true);
                match t {
                    Tier::Memory => {
                        if *new_memory_count > 0 {
                            *new_memory_count -= 1;
                        }
                    }
                    Tier::Disk => {
                        if *new_disk_count > 0 {
                            *new_disk_count -= 1;
                        }
                    }
                    _ => {}
                }
                *grace_start = Instant::now();
            } else {
                // Routing join
                if !routing_ips.contains(&public_ip.to_string()) {
                    routing_ips.push(public_ip.to_string());
                }
            }
        }
        "depart" => {
            info!(
                "Received depart from server {}/{} in tier {}",
                public_ip, private_ip, tier_name
            );
            if let Some(t) = tier {
                let ring = global_hash_rings.entry(t).or_default();
                ring.remove(public_ip, private_ip, 0);
                let ip_pair = format!("{}/{}", public_ip, private_ip);
                match t {
                    Tier::Memory => {
                        memory_storage.remove(&ip_pair);
                        memory_occupancy.remove(&ip_pair);
                        // Remove per-thread access entries.
                        for freq in key_access_frequency.values_mut() {
                            for tid in 0..memory_thread_count {
                                freq.remove(&format!("{}:{}", ip_pair, tid));
                            }
                        }
                    }
                    Tier::Disk => {
                        disk_storage.remove(&ip_pair);
                        disk_occupancy.remove(&ip_pair);
                        for freq in key_access_frequency.values_mut() {
                            for tid in 0..disk_thread_count {
                                freq.remove(&format!("{}:{}", ip_pair, tid));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {
            warn!("Unknown membership action: {}", action);
        }
    }
}

/// Process a depart-done ack from a departing node.
///
/// Message format: "public_ip_private_ip_tier_id"
/// Returns `Some((tier_id, public_ip, private_ip))` if departure is complete
/// (all thread acks received), so the caller can send a ScalingAlert via ZMQ.
pub fn depart_done_handler(
    msg: &str,
    departing_node_map: &mut HashMap<Address, u32>,
    removing_memory_node: &mut bool,
    removing_disk_node: &mut bool,
    grace_start: &mut Instant,
) -> Option<(u32, String, String)> {
    let parts: Vec<&str> = msg.split('_').collect();
    if parts.len() < 3 {
        error!("Invalid depart_done message: {}", msg);
        return None;
    }

    let public_ip = parts[0].to_string();
    let private_ip = parts[1].to_string();

    if let Some(count) = departing_node_map.get_mut(&private_ip) {
        *count -= 1;
        if *count == 0 {
            let tier_id: u32 = parts[2].parse().unwrap_or(0);
            if tier_id == Tier::Memory as u32 {
                *removing_memory_node = false;
            } else {
                *removing_disk_node = false;
            }

            info!("Depart done for node {} (tier {})", private_ip, tier_id);
            *grace_start = Instant::now();
            departing_node_map.remove(&private_ip);
            return Some((tier_id, public_ip, private_ip));
        }
    } else {
        error!("Received depart_done for unknown node: {}", private_ip);
    }

    None
}

/// Process user feedback (latency/throughput reports).
///
/// Parses a UserFeedback protobuf.
pub fn feedback_handler(
    data: &[u8],
    user_latency: &mut HashMap<String, f64>,
    user_throughput: &mut HashMap<String, f64>,
    latency_miss_ratio_map: &mut HashMap<String, (f64, u32)>,
    slo_worst: u32,
) {
    use anna_server_common::proto::metadata::UserFeedback;
    use prost::Message;

    let fb = match UserFeedback::decode(data) {
        Ok(fb) => fb,
        Err(e) => {
            error!("Failed to decode UserFeedback: {}", e);
            return;
        }
    };

    let uid = fb.uid.clone();

    if fb.finish {
        user_latency.remove(&uid);
        return;
    }

    user_latency.insert(uid.clone(), fb.latency);
    user_throughput.insert(uid, fb.throughput);

    // Update per-key latency miss ratio (running average).
    for kl in &fb.key_latency {
        let ratio = kl.latency / slo_worst as f64;
        let entry = latency_miss_ratio_map
            .entry(kl.key.clone())
            .or_insert((0.0, 0));
        entry.1 += 1;
        entry.0 += (ratio - entry.0) / entry.1 as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rings() -> HashMap<Tier, ConsistentHashRing> {
        let mut rings = HashMap::new();
        rings.insert(Tier::Memory, ConsistentHashRing::new());
        rings.insert(Tier::Disk, ConsistentHashRing::new());
        rings
    }

    #[test]
    fn membership_join_memory() {
        let mut rings = make_rings();
        let mut routing_ips = vec![];
        let mut ms = StorageStats::new();
        let mut ds = StorageStats::new();
        let mut mo = OccupancyStats::new();
        let mut do_ = OccupancyStats::new();
        let mut kaf = KeyAccessFrequency::new();
        let mut nmc = 1u32;
        let mut ndc = 0u32;
        let mut grace = Instant::now();

        membership_handler(
            "join:MEMORY:10.0.0.1:10.0.0.1",
            &mut rings,
            &mut routing_ips,
            &mut ms,
            &mut ds,
            &mut mo,
            &mut do_,
            &mut kaf,
            &mut nmc,
            &mut ndc,
            &mut grace,
            1,
            1,
            100,
            0,
        );

        assert!(!rings[&Tier::Memory].is_empty());
        assert_eq!(nmc, 0);
    }

    #[test]
    fn membership_join_routing() {
        let mut rings = make_rings();
        let mut routing_ips = vec![];
        let mut ms = StorageStats::new();
        let mut ds = StorageStats::new();
        let mut mo = OccupancyStats::new();
        let mut do_ = OccupancyStats::new();
        let mut kaf = KeyAccessFrequency::new();
        let mut nmc = 0u32;
        let mut ndc = 0u32;
        let mut grace = Instant::now();

        membership_handler(
            "join:ROUTING:10.0.0.2:10.0.0.2",
            &mut rings,
            &mut routing_ips,
            &mut ms,
            &mut ds,
            &mut mo,
            &mut do_,
            &mut kaf,
            &mut nmc,
            &mut ndc,
            &mut grace,
            1,
            1,
            100,
            0,
        );

        assert_eq!(routing_ips, vec!["10.0.0.2"]);
    }

    #[test]
    fn membership_depart_memory() {
        let mut rings = make_rings();
        rings
            .get_mut(&Tier::Memory)
            .unwrap()
            .insert("10.0.0.1", "10.0.0.1", 0, 0, 100, true);
        let mut routing_ips = vec![];
        let mut ms = StorageStats::new();
        ms.insert("10.0.0.1/10.0.0.1".into(), HashMap::new());
        let mut ds = StorageStats::new();
        let mut mo = OccupancyStats::new();
        mo.insert("10.0.0.1/10.0.0.1".into(), HashMap::new());
        let mut do_ = OccupancyStats::new();
        let mut kaf = KeyAccessFrequency::new();
        let mut nmc = 0u32;
        let mut ndc = 0u32;
        let mut grace = Instant::now();

        membership_handler(
            "depart:MEMORY:10.0.0.1:10.0.0.1",
            &mut rings,
            &mut routing_ips,
            &mut ms,
            &mut ds,
            &mut mo,
            &mut do_,
            &mut kaf,
            &mut nmc,
            &mut ndc,
            &mut grace,
            1,
            1,
            100,
            0,
        );

        assert!(rings[&Tier::Memory].is_empty());
        assert!(!ms.contains_key("10.0.0.1/10.0.0.1"));
        assert!(!mo.contains_key("10.0.0.1/10.0.0.1"));
    }

    #[test]
    fn feedback_stores_latency() {
        use anna_server_common::proto::metadata::UserFeedback;
        use prost::Message;

        let mut user_latency = HashMap::new();
        let mut user_throughput = HashMap::new();
        let mut latency_map = HashMap::new();

        let fb = UserFeedback {
            uid: "user1".into(),
            latency: 1500.0,
            throughput: 100.0,
            finish: false,
            warmup: false,
            key_latency: vec![],
        };
        let data = fb.encode_to_vec();

        feedback_handler(
            &data,
            &mut user_latency,
            &mut user_throughput,
            &mut latency_map,
            3000,
        );

        assert_eq!(*user_latency.get("user1").unwrap(), 1500.0);
        assert_eq!(*user_throughput.get("user1").unwrap(), 100.0);
    }

    #[test]
    fn feedback_finish_removes_user() {
        use anna_server_common::proto::metadata::UserFeedback;
        use prost::Message;

        let mut user_latency = HashMap::new();
        user_latency.insert("user1".into(), 1500.0);
        let mut user_throughput = HashMap::new();
        let mut latency_map = HashMap::new();

        let fb = UserFeedback {
            uid: "user1".into(),
            latency: 0.0,
            throughput: 0.0,
            finish: true,
            warmup: false,
            key_latency: vec![],
        };
        let data = fb.encode_to_vec();

        feedback_handler(
            &data,
            &mut user_latency,
            &mut user_throughput,
            &mut latency_map,
            3000,
        );

        assert!(!user_latency.contains_key("user1"));
    }

    #[test]
    fn depart_done_decrements_and_removes() {
        let mut departing = HashMap::new();
        departing.insert("10.0.0.1".to_string(), 1u32);
        let mut removing_mem = true;
        let mut removing_disk = false;
        let mut grace = Instant::now();

        let result = depart_done_handler(
            "10.0.0.1_10.0.0.1_1",
            &mut departing,
            &mut removing_mem,
            &mut removing_disk,
            &mut grace,
        );

        assert!(!removing_mem);
        assert!(departing.is_empty());
        assert!(result.is_some());
        let (tier_id, _pub_ip, _priv_ip) = result.unwrap();
        assert_eq!(tier_id, 1); // Memory
    }
}
