//! Main monitoring event loop.
//!
//! Mirrors `server/cpp/src/monitor/monitoring.cpp`.

use anna_server_common::config::Config;
use anna_server_common::hash_ring::ConsistentHashRing;
use anna_server_common::metadata::Tier;
use anna_server_common::signal;
use anna_server_common::threads::{MonitoringThread, RoutingThread, ServerThread};
use anna_server_common::types::Address;
use log::{error, info, warn};
use omq_tokio::{Context, Message as ZmqMessage, Options, Socket as OmqSocket, SocketType};
use prost::Message;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::handlers;
use crate::policies;
use crate::stats;
use crate::types::*;

/// Lazy-connecting PUSH socket cache.
pub(crate) struct SocketCache {
    ctx: Context,
    sockets: HashMap<Address, OmqSocket>,
}

impl SocketCache {
    pub(crate) fn new(ctx: Context) -> Self {
        Self {
            ctx,
            sockets: HashMap::new(),
        }
    }

    pub(crate) async fn send(&mut self, addr: &str, data: &[u8]) -> Result<(), String> {
        if !self.sockets.contains_key(addr) {
            let sock = self.ctx.socket(SocketType::Push, Options::default());
            let endpoint = addr
                .parse()
                .map_err(|e| format!("Invalid address {}: {}", addr, e))?;
            sock.connect(endpoint)
                .await
                .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;
            self.sockets.insert(addr.to_string(), sock);
        }
        let sock = self.sockets.get_mut(addr).expect("socket just inserted");
        sock.send(ZmqMessage::from(data.to_vec()))
            .await
            .map_err(|e| format!("Failed to send to {}: {}", addr, e))
    }

    pub(crate) async fn send_string(&mut self, addr: &str, msg: &str) -> Result<(), String> {
        self.send(addr, msg.as_bytes()).await
    }
}

/// Send a ScalingAlert (ADD) to the external scaling system.
async fn emit_scale_up_alert(
    pushers: &mut SocketCache,
    scaling_alert_ip: &str,
    base_offset: u32,
    tier_name: &str,
    count: u32,
    new_count: &mut u32,
) {
    use anna_server_common::proto::metadata::ScalingAlert;

    let alert = ScalingAlert {
        action: 1, // ADD
        tier: match tier_name {
            "MEMORY" => 1,
            "DISK" => 2,
            _ => 0,
        },
        count,
        departed_node_ip: String::new(),
    };
    let encoded = alert.encode_to_vec();
    let addr = anna_server_common::threads::scaling_alert_address(scaling_alert_ip, base_offset);

    if let Err(e) = pushers.send(&addr, &encoded).await {
        error!("Failed to send scale-up alert: {}", e);
    } else {
        info!(
            "Emitted scale-up alert: add {} {} node(s)",
            count, tier_name
        );
        *new_count = count;
    }
}

/// Send a self-depart command to a KVS node, initiating graceful removal.
async fn remove_node(
    pushers: &mut SocketCache,
    mt: &MonitoringThread,
    node: &ServerThread,
    departing_node_map: &mut HashMap<Address, u32>,
    removing: &mut bool,
    thread_count: u32,
) {
    let depart_addr = mt.depart_done_connect_address();

    if let Err(e) = pushers
        .send_string(&node.self_depart_connect_address(), &depart_addr)
        .await
    {
        error!("Failed to send depart to {}: {}", node.private_ip(), e);
        return;
    }

    departing_node_map.insert(node.private_ip().to_string(), thread_count);
    *removing = true;
    info!("Initiated removal of node {}", node.private_ip());
}

/// Run the monitoring event loop.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    signal::install_shutdown_handler();

    let base_offset = config.ports.base_offset;
    let monitoring_ip = config.monitoring.ip.clone();
    let scaling_alert_ip = config.monitoring.scaling_alert_ip.clone();

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

    let memory_node_capacity = config.memory_capacity_bytes() / 1024;
    let disk_node_capacity = config.disk_capacity_bytes() / 1024;
    let virtual_nodes = 3000u32;

    let mt = MonitoringThread::new(&monitoring_ip, base_offset);

    info!("Elasticity policy enabled: {}", params.enable_elasticity);
    info!("Tiering policy enabled: {}", params.enable_tiering);
    info!(
        "Selective replication policy enabled: {}",
        params.enable_selective_rep
    );

    // ── ZMQ Sockets ─────────────────────────────────────────────────

    let ctx = Context::new();

    let notify_puller = ctx.socket(SocketType::Pull, Options::default());
    notify_puller
        .bind(mt.notify_connect_address().parse()?)
        .await?;

    let depart_done_puller = ctx.socket(SocketType::Pull, Options::default());
    depart_done_puller
        .bind(mt.depart_done_connect_address().parse()?)
        .await?;

    let feedback_puller = ctx.socket(SocketType::Pull, Options::default());
    feedback_puller
        .bind(mt.feedback_report_connect_address().parse()?)
        .await?;

    let response_puller = ctx.socket(SocketType::Pull, Options::default());
    response_puller
        .bind(mt.response_connect_address().parse()?)
        .await?;

    let mut pushers = SocketCache::new(ctx.clone());

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
    let mut key_replication_map: HashMap<
        anna_server_common::types::Key,
        anna_server_common::metadata::KeyReplication,
    > = HashMap::new();
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
    let mut epoch = 0u32;
    let mut last_epoch_change: HashMap<Address, Instant> = HashMap::new();
    let mut rid = 0u32;

    info!(
        "Monitor listening on {} (base_offset={})",
        monitoring_ip, base_offset
    );

    // ── Event loop ──────────────────────────────────────────────────

    let poll_timeout = Duration::from_millis(100);

    while !signal::shutdown_requested() {
        // Non-blocking poll: try each socket with a short timeout.
        // Process at most one message per iteration to keep the loop responsive.

        if let Ok(Ok(msg)) = tokio::time::timeout(poll_timeout, notify_puller.recv()).await {
            let data: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
            let text = String::from_utf8_lossy(&data);

            // Record last_epoch_change for KVS join/depart events only.
            // Routing joins don't report stats and shouldn't be crash-checked.
            let parts: Vec<&str> = text.split(':').collect();
            if parts.len() >= 4 {
                let tier_name = parts[1];
                if tier_name == "MEMORY" || tier_name == "DISK" {
                    let ip_pair = format!("{}/{}", parts[2], parts[3]);
                    if parts[0] == "join" {
                        last_epoch_change.insert(ip_pair, Instant::now());
                    } else if parts[0] == "depart" {
                        last_epoch_change.remove(&ip_pair);
                    }
                }
            }

            handlers::membership_handler(
                &text,
                &mut global_hash_rings,
                &mut routing_ips,
                &mut memory_storage,
                &mut disk_storage,
                &mut memory_occupancy,
                &mut disk_occupancy,
                &mut key_access_frequency,
                &mut new_memory_count,
                &mut new_disk_count,
                &mut grace_start,
                memory_thread_count,
                disk_thread_count,
                virtual_nodes,
                base_offset,
            );
        }

        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, depart_done_puller.recv()).await {
            let data: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
            let text = String::from_utf8_lossy(&data);

            if let Some((_tier_id, _pub_ip, _priv_ip)) = handlers::depart_done_handler(
                &text,
                &mut departing_node_map,
                &mut removing_memory_node,
                &mut removing_disk_node,
                &mut grace_start,
            ) {
                // Send ScalingAlert (REMOVE) to scaling system.
                use anna_server_common::proto::metadata::ScalingAlert;
                let alert = ScalingAlert {
                    action: 2, // REMOVE
                    tier: _tier_id as i32,
                    count: 1,
                    departed_node_ip: format!("{}_{}", _pub_ip, _priv_ip),
                };
                let alert_addr = anna_server_common::threads::scaling_alert_address(
                    &scaling_alert_ip,
                    base_offset,
                );
                let encoded = alert.encode_to_vec();
                if let Err(e) = pushers.send(&alert_addr, &encoded).await {
                    error!("Failed to send depart ScalingAlert: {}", e);
                }
                grace_start = Instant::now();
            }
        }

        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, feedback_puller.recv()).await {
            let data: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
            handlers::feedback_handler(
                &data,
                &mut user_latency,
                &mut user_throughput,
                &mut latency_miss_ratio_map,
                params.slo_worst_us,
            );
        }

        // ── Periodic monitoring cycle ───────────────────────────────
        let elapsed = report_start.elapsed().as_secs() as u32;
        if elapsed >= params.monitoring_threshold_s {
            epoch += 1;
            info!("Starting monitoring cycle {} (elapsed={}s)", epoch, elapsed);

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

            // Collect internal stats from all KVS nodes.
            collect_internal_stats(
                &global_hash_rings,
                &mt,
                &mut pushers,
                &response_puller,
                &mut memory_storage,
                &mut disk_storage,
                &mut memory_occupancy,
                &mut disk_occupancy,
                &mut memory_accesses,
                &mut disk_accesses,
                &mut key_access_frequency,
                &mut key_size,
                memory_thread_count,
                disk_thread_count,
                base_offset,
                &mut rid,
                &monitoring_ip,
                Duration::from_millis(params.monitoring_response_timeout_ms as u64),
            )
            .await;
            // Crash detection — only run if we received at least some stats
            // responses this cycle. If collect_internal_stats got nothing
            // (e.g., KVS hasn't published its first report yet), skip
            // crash detection to avoid false positives.
            let stale_threshold = Duration::from_secs(params.monitoring_threshold_s as u64);
            let mut dead_nodes = Vec::new();

            let mut reporting_nodes = std::collections::HashSet::new();
            for ip_pair in memory_occupancy.keys() {
                reporting_nodes.insert(ip_pair.clone());
            }
            for ip_pair in disk_occupancy.keys() {
                reporting_nodes.insert(ip_pair.clone());
            }

            // Update last_epoch_change for reporting nodes.
            for ip_pair in &reporting_nodes {
                last_epoch_change
                    .entry(ip_pair.clone())
                    .and_modify(|t| *t = Instant::now())
                    .or_insert_with(Instant::now);
            }

            // Only detect dead nodes if we got SOME responses this cycle.
            // This prevents false positives during startup when the KVS
            // hasn't published its first stats report yet.
            if !reporting_nodes.is_empty() {
                for (ip_pair, last_seen) in &last_epoch_change {
                    if !reporting_nodes.contains(ip_pair) && last_seen.elapsed() > stale_threshold {
                        dead_nodes.push(ip_pair.clone());
                    }
                }
            }
            for ip_pair in &dead_nodes {
                warn!("Detected crashed node: {}", ip_pair);
                let parts: Vec<&str> = ip_pair.split('/').collect();
                if parts.len() == 2 {
                    let (pub_ip, priv_ip) = (parts[0], parts[1]);

                    // Remove from hash rings.
                    for ring in global_hash_rings.values_mut() {
                        ring.remove(pub_ip, priv_ip, 0);
                    }

                    // Notify all routing nodes.
                    for tier_name in ["MEMORY", "DISK"] {
                        let msg = format!("depart:{}:{}:{}", tier_name, pub_ip, priv_ip);
                        for rt_ip in &routing_ips {
                            let rt = RoutingThread::new(rt_ip, 0, base_offset);
                            if let Err(e) = pushers
                                .send_string(&rt.notify_connect_address(), &msg)
                                .await
                            {
                                error!("Failed to notify routing of crash: {}", e);
                            }
                        }
                    }
                }
                last_epoch_change.remove(ip_pair);
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

            // Run policies — they set new_*_count to request scaling.
            let prev_mem = new_memory_count;
            let prev_disk = new_disk_count;

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

            let movement_requests = policies::movement_policy(
                &ss,
                &params,
                &key_access_summary,
                &key_replication_map,
                &key_size,
                memory_node_count,
                &mut new_memory_count,
                grace_elapsed,
            );

            let slo_requests = policies::slo_policy(
                &ss,
                &params,
                &key_access_summary,
                &latency_miss_ratio_map,
                &key_replication_map,
                memory_node_count,
                &mut new_memory_count,
                &mut removing_memory_node,
                grace_elapsed,
            );

            // Execute replication changes requested by policies.
            if !movement_requests.is_empty() {
                crate::replication::change_replication_factor(
                    &movement_requests,
                    &global_hash_rings,
                    &routing_ips,
                    &mut key_replication_map,
                    &mut pushers,
                    &mt,
                    &response_puller,
                    &mut rid,
                    &monitoring_ip,
                    base_offset,
                    Duration::from_millis(params.monitoring_response_timeout_ms as u64),
                )
                .await;
            }

            if !slo_requests.is_empty() {
                crate::replication::change_replication_factor(
                    &slo_requests,
                    &global_hash_rings,
                    &routing_ips,
                    &mut key_replication_map,
                    &mut pushers,
                    &mt,
                    &response_puller,
                    &mut rid,
                    &monitoring_ip,
                    base_offset,
                    Duration::from_millis(params.monitoring_response_timeout_ms as u64),
                )
                .await;
            }

            // Send scaling alerts if policies requested new nodes.
            if new_memory_count > prev_mem {
                emit_scale_up_alert(
                    &mut pushers,
                    &scaling_alert_ip,
                    base_offset,
                    "MEMORY",
                    new_memory_count,
                    &mut new_memory_count,
                )
                .await;
                grace_start = Instant::now();
            }
            if new_disk_count > prev_disk {
                emit_scale_up_alert(
                    &mut pushers,
                    &scaling_alert_ip,
                    base_offset,
                    "DISK",
                    new_disk_count,
                    &mut new_disk_count,
                )
                .await;
                grace_start = Instant::now();
            }

            // Clear feedback maps.
            user_latency.clear();
            user_throughput.clear();
            latency_miss_ratio_map.clear();

            report_start = Instant::now();
        }
    }

    info!("Monitor shutting down");
    let _ = (epoch, departing_node_map, key_size);
    Ok(())
}

/// Collect internal stats from all KVS nodes by sending GET requests
/// for metadata keys and parsing the responses.
///
/// Sends requests to all threads on all nodes, collects responses,
/// then processes them into the stat maps.
///
/// Note: This runs sequentially (one request-response at a time) and
/// blocks the event loop during collection. This matches the C++ monitor
/// behavior. For large clusters, this could be parallelized with
/// tokio::spawn, but the sequential approach is simpler and sufficient
/// for typical deployments.
async fn collect_internal_stats(
    global_hash_rings: &HashMap<Tier, ConsistentHashRing>,
    mt: &MonitoringThread,
    pushers: &mut SocketCache,
    response_puller: &OmqSocket,
    memory_storage: &mut StorageStats,
    disk_storage: &mut StorageStats,
    memory_occupancy: &mut OccupancyStats,
    disk_occupancy: &mut OccupancyStats,
    memory_accesses: &mut AccessStats,
    disk_accesses: &mut AccessStats,
    key_access_frequency: &mut KeyAccessFrequency,
    key_size: &mut KeySizeMap,
    memory_thread_count: u32,
    disk_thread_count: u32,
    base_offset: u32,
    rid: &mut u32,
    monitor_ip: &str,
    timeout: Duration,
) {
    use anna_server_common::proto::kvs::{
        KeyRequest, KeyResponse, KeyTuple, LatticeType, RequestType,
    };

    // Phase 1: Send all requests and collect raw response bytes.
    let mut responses: Vec<Vec<u8>> = Vec::new();

    let tier_configs: &[(Tier, u32)] = &[
        (Tier::Memory, memory_thread_count),
        (Tier::Disk, disk_thread_count),
    ];

    for &(tier, thread_count) in tier_configs {
        let ring = match global_hash_rings.get(&tier) {
            Some(r) => r,
            None => continue,
        };

        let tier_name = match tier {
            Tier::Memory => "MEMORY",
            Tier::Disk => "DISK",
            _ => continue,
        };

        for st in ring.get_unique_servers() {
            for tid in 0..thread_count {
                let server_thread =
                    ServerThread::new(st.public_ip(), st.private_ip(), tid, base_offset);

                let ip_pair = format!("{}/{}", st.public_ip(), st.private_ip());
                let meta_keys = [
                    format!(
                        "ANNA_METADATA|server_stats|{}|{}|{}",
                        ip_pair, tid, tier_name
                    ),
                    format!("ANNA_METADATA|key_access|{}|{}|{}", ip_pair, tid, tier_name),
                    format!("ANNA_METADATA|key_size|{}|{}|{}", ip_pair, tid, tier_name),
                ];

                *rid += 1;
                let request_id = format!("{}:{}", monitor_ip, rid);

                let mut request = KeyRequest {
                    r#type: RequestType::Get as i32,
                    response_address: mt.response_connect_address(),
                    request_id,
                    ..Default::default()
                };

                for key in &meta_keys {
                    request.tuples.push(KeyTuple {
                        key: key.clone(),
                        lattice_type: LatticeType::Lww as i32,
                        ..Default::default()
                    });
                }

                let target_addr = server_thread.key_request_connect_address();
                let encoded = request.encode_to_vec();

                if let Err(e) = pushers.send(&target_addr, &encoded).await {
                    warn!("Failed to send stats request to {}: {}", target_addr, e);
                    continue;
                }

                match tokio::time::timeout(timeout, response_puller.recv()).await {
                    Ok(Ok(msg)) => {
                        let bytes: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
                        responses.push(bytes);
                    }
                    Ok(Err(e)) => {
                        warn!("ZMQ recv error for stats from {}: {}", ip_pair, e);
                    }
                    Err(_) => {
                        warn!("Stats collection timed out for {}:{}", ip_pair, tid);
                    }
                }
            }
        }
    }

    // Phase 2: Process all collected responses.
    for bytes in &responses {
        if let Ok(response) = KeyResponse::decode(bytes.as_slice()) {
            process_stats_response(
                &response,
                &mut *memory_storage,
                &mut *disk_storage,
                &mut *memory_occupancy,
                &mut *disk_occupancy,
                &mut *memory_accesses,
                &mut *disk_accesses,
                &mut *key_access_frequency,
                &mut *key_size,
            );
        }
    }
}

/// Process a stats response from a KVS node.
fn process_stats_response(
    response: &anna_server_common::proto::kvs::KeyResponse,
    memory_storage: &mut StorageStats,
    disk_storage: &mut StorageStats,
    memory_occupancy: &mut OccupancyStats,
    disk_occupancy: &mut OccupancyStats,
    memory_accesses: &mut AccessStats,
    disk_accesses: &mut AccessStats,
    key_access_frequency: &mut KeyAccessFrequency,
    key_size: &mut KeySizeMap,
) {
    use anna_server_common::proto::kvs::LwwValue;
    use anna_server_common::proto::metadata::{KeyAccessData, KeySizeData, ServerThreadStatistics};

    for tuple in &response.tuples {
        if tuple.error != 0 {
            continue; // KEY_DNE or other error
        }

        let key = &tuple.key;
        let parts: Vec<&str> = key.split('|').collect();
        // Expected: ANNA_METADATA|type|ip_pair|tid|tier
        if parts.len() < 5 {
            continue;
        }

        let meta_type = parts[1];
        let ip_pair = parts[2].to_string();
        let tid: u32 = parts[3].parse().unwrap_or(0);
        let tier_name = parts[4];

        // Unwrap LWW wrapper.
        let inner_payload = match LwwValue::decode(tuple.payload.as_slice()) {
            Ok(lww) => lww.value,
            Err(_) => continue,
        };

        let is_memory = tier_name == "MEMORY";

        match meta_type {
            "server_stats" => {
                if let Ok(stats) = ServerThreadStatistics::decode(inner_payload.as_slice()) {
                    if is_memory {
                        memory_storage
                            .entry(ip_pair.clone())
                            .or_default()
                            .insert(tid, stats.storage_consumption);
                        memory_occupancy
                            .entry(ip_pair.clone())
                            .or_default()
                            .insert(tid, (stats.occupancy, stats.epoch));
                        memory_accesses
                            .entry(ip_pair)
                            .or_default()
                            .insert(tid, stats.access_count);
                    } else {
                        disk_storage
                            .entry(ip_pair.clone())
                            .or_default()
                            .insert(tid, stats.storage_consumption);
                        disk_occupancy
                            .entry(ip_pair.clone())
                            .or_default()
                            .insert(tid, (stats.occupancy, stats.epoch));
                        disk_accesses
                            .entry(ip_pair)
                            .or_default()
                            .insert(tid, stats.access_count);
                    }
                }
            }
            "key_access" => {
                if let Ok(access_data) = KeyAccessData::decode(inner_payload.as_slice()) {
                    let thread_key = format!("{}:{}", ip_pair, tid);
                    for ka in &access_data.keys {
                        key_access_frequency
                            .entry(ka.key.clone())
                            .or_default()
                            .insert(thread_key.clone(), ka.access_count);
                    }
                }
            }
            "key_size" => {
                if let Ok(size_data) = KeySizeData::decode(inner_payload.as_slice()) {
                    for ks in &size_data.key_sizes {
                        key_size.insert(ks.key.clone(), ks.size);
                    }
                }
            }
            _ => {}
        }
    }
}
