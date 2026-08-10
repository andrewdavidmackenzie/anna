//! KVS event loop — receives and dispatches requests via ZMQ.
//!
//! Mirrors `server/cpp/src/kvs/server.cpp` `run()` function.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anna_server_common::config::Config;
use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
use anna_server_common::metadata::{Tier, TierMetadata};
use anna_server_common::proto::kvs::{KeyRequest, RequestType};
use anna_server_common::routing::DEFAULT_METADATA_REPLICATION_FACTOR;
use anna_server_common::signal;
use anna_server_common::threads::ServerThread;
use anna_server_common::types::Address;
use omq_tokio::{Context, Message as ZmqMessage, Options, SocketType};
use prost::Message;

use crate::context::KvsContext;
use crate::handlers;
use crate::storage;

/// ZMQ PUSH socket cache — lazily connects on first send.
struct SocketCache {
    ctx: Context,
    sockets: HashMap<Address, omq_tokio::Socket>,
}

impl SocketCache {
    fn new(ctx: Context) -> Self {
        Self {
            ctx,
            sockets: HashMap::new(),
        }
    }

    async fn send(&mut self, addr: &str, data: &[u8]) -> Result<(), String> {
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
}

/// Extract message bytes from an omq-tokio Message.
fn msg_bytes(msg: &ZmqMessage) -> Vec<u8> {
    msg.iter().flat_map(|f| f.to_vec()).collect()
}

/// Bind a PULL socket to the given address, returning a descriptive error.
async fn bind_pull(ctx: &Context, addr: &str, name: &str) -> Result<omq_tokio::Socket, String> {
    let sock = ctx.socket(SocketType::Pull, Options::default());
    let endpoint = addr
        .parse()
        .map_err(|e| format!("Invalid address for {}: {} ({})", name, addr, e))?;
    sock.bind(endpoint)
        .await
        .map_err(|e| format!("Failed to bind {} on {}: {}", name, addr, e))?;
    Ok(sock)
}

/// Run the KVS event loop for a single thread.
pub async fn run(
    thread_id: u32,
    config: &Config,
    public_ip: &str,
    private_ip: &str,
    _seed_ip: &str,
    self_tier: Tier,
    thread_count: u32,
    self_join_count: i32,
    routing_ips: Vec<Address>,
    monitoring_ips: Vec<Address>,
) -> Result<(), String> {
    let base_offset = config.ports.base_offset;
    let wt = ServerThread::new(public_ip, private_ip, thread_id, base_offset);

    let log_name = format!("kvs_{}", thread_id);
    log::info!("[{}] Starting KVS thread", log_name);

    // ── ZMQ context and sockets ──
    let ctx = Context::new();

    let join_puller = bind_pull(&ctx, &wt.node_join_connect_address(), "join").await?;
    let depart_puller = bind_pull(&ctx, &wt.node_depart_connect_address(), "depart").await?;
    let self_depart_puller =
        bind_pull(&ctx, &wt.self_depart_connect_address(), "self_depart").await?;
    let request_puller = bind_pull(&ctx, &wt.key_request_bind_address(), "request").await?;
    let gossip_puller = bind_pull(&ctx, &wt.gossip_connect_address(), "gossip").await?;
    let replication_response_puller = bind_pull(
        &ctx,
        &wt.replication_response_connect_address(),
        "rep_response",
    )
    .await?;
    let replication_change_puller =
        bind_pull(&ctx, &wt.replication_change_connect_address(), "rep_change").await?;
    let cache_ip_response_puller = bind_pull(
        &ctx,
        &wt.cache_ip_response_connect_address(),
        "cache_ip_response",
    )
    .await?;
    let management_node_response_puller = bind_pull(
        &ctx,
        &wt.management_node_response_connect_address(),
        "mgmt_response",
    )
    .await?;
    let cache_registration_puller =
        bind_pull(&ctx, &wt.cache_registration_connect_address(), "cache_reg").await?;

    let mut pushers = SocketCache::new(ctx.clone());

    // ── Initialize hash rings ──
    let mut global_hash_rings = HashMap::new();
    let mut local_hash_rings = HashMap::new();

    // Add self to global ring.
    let mut g_ring = ConsistentHashRing::new();
    g_ring.insert(
        public_ip,
        private_ip,
        0,
        base_offset,
        DEFAULT_VIRTUAL_THREAD_NUM,
        true,
    );
    global_hash_rings.insert(self_tier, g_ring);

    // Form local hash ring.
    let mut l_ring = ConsistentHashRing::new();
    for tid in 0..thread_count {
        l_ring.insert(
            public_ip,
            private_ip,
            tid,
            base_offset,
            DEFAULT_VIRTUAL_THREAD_NUM,
            false,
        );
    }
    local_hash_rings.insert(self_tier, l_ring);

    // ── Initialize serializers (all lattice types including compounds) ──
    let serializers = storage::create_memory_serializers();

    // ── Initialize tier metadata ──
    let mut tier_metadata = HashMap::new();
    tier_metadata.insert(
        Tier::Memory,
        TierMetadata {
            id: Tier::Memory,
            thread_number: config.threads.memory,
            default_replication: config.replication.memory,
            node_capacity: config.memory_capacity_bytes(),
        },
    );
    tier_metadata.insert(
        Tier::Disk,
        TierMetadata {
            id: Tier::Disk,
            thread_number: config.threads.disk,
            default_replication: config.replication.disk,
            node_capacity: config.disk_capacity_bytes(),
        },
    );

    // ── Build KVS context ──
    let mut ctx_state = KvsContext {
        thread_id,
        public_ip: public_ip.to_string(),
        private_ip: private_ip.to_string(),
        wt: wt.clone(),
        self_tier,
        thread_count,
        global_hash_rings,
        local_hash_rings,
        stored_key_map: HashMap::new(),
        serializers,
        key_replication_map: HashMap::new(),
        tier_metadata,
        default_local_replication: config.replication.local,
        metadata_replication_factor: DEFAULT_METADATA_REPLICATION_FACTOR,
        self_join_count,
        pending_requests: HashMap::new(),
        pending_gossip: HashMap::new(),
        key_access_tracker: HashMap::new(),
        local_changeset: Default::default(),
        access_count: 0,
        join_gossip_map: HashMap::new(),
        join_remove_set: Default::default(),
        extant_caches: Default::default(),
        cache_ip_to_keys: HashMap::new(),
        key_to_cache_ips: HashMap::new(),
        routing_ips,
        monitoring_ips,
        rid: 0,
        seed: 42 + thread_id,
    };

    // ── Timers ──
    let poll_timeout = Duration::from_millis(100);
    let gossip_period = Duration::from_secs(config.timings.gossip_epoch.into());
    let gc_period = Duration::from_micros(
        config
            .timings
            .garbage_collect_period_us
            .unwrap_or(config.timings.gossip_epoch as u64 * 1_000_000),
    );
    let report_period = Duration::from_secs(config.timings.server_report_period.into());
    let mut gossip_start = Instant::now();
    let mut gc_start = Instant::now();
    let mut report_start = Instant::now();

    // ── Store initial membership metadata (thread 0 only) ──
    // Clients using direct routing need this metadata immediately.
    if thread_id == 0 {
        use anna_server_common::proto::kvs::LatticeType;
        use anna_server_common::proto::shared::StringSet;

        // kvs_members
        let mut members_set = StringSet::default();
        for ring in ctx_state.global_hash_rings.values() {
            for st in ring.get_unique_servers() {
                members_set
                    .keys
                    .push(format!("{}/{}", st.public_ip(), st.private_ip()));
            }
        }
        if !members_set.keys.is_empty() {
            let members_payload = members_set.encode_to_vec();
            let ts = handlers::utils::generate_timestamp(0);
            let lww = anna_server_common::proto::kvs::LwwValue {
                timestamp: ts,
                value: members_payload,
            };
            let key = "ANNA_METADATA|kvs_members";
            if let Some(s) = ctx_state.serializers.get_mut(&(LatticeType::Lww as i32)) {
                handlers::utils::process_put(
                    key,
                    LatticeType::Lww,
                    &lww.encode_to_vec(),
                    s.as_mut(),
                    &mut ctx_state.stored_key_map,
                    0,
                );
            }
        }

        // cluster_topology
        let topology = anna_server_common::proto::metadata::ClusterTopology {
            memory_thread_count: config.threads.memory,
            disk_thread_count: config.threads.disk,
            ..Default::default()
        };
        let topo_payload = topology.encode_to_vec();
        let ts = handlers::utils::generate_timestamp(0);
        let lww = anna_server_common::proto::kvs::LwwValue {
            timestamp: ts,
            value: topo_payload,
        };
        let key = "ANNA_METADATA|cluster_topology";
        if let Some(s) = ctx_state.serializers.get_mut(&(LatticeType::Lww as i32)) {
            handlers::utils::process_put(
                key,
                LatticeType::Lww,
                &lww.encode_to_vec(),
                s.as_mut(),
                &mut ctx_state.stored_key_map,
                0,
            );
        }

        log::info!("[{}] Stored initial membership metadata", log_name);
    }

    // ── Startup notifications (thread 0 only) ──
    if thread_id == 0 {
        let tier_name = match self_tier {
            Tier::Memory => "MEMORY",
            Tier::Disk => "DISK",
            _ => "MEMORY",
        };
        let join_msg = format!(
            "join:{}:{}:{}:{}",
            tier_name, public_ip, private_ip, self_join_count
        );

        // Notify routing nodes.
        for addr in &ctx_state.routing_ips {
            let target = anna_server_common::threads::RoutingThread::new(addr, 0, base_offset)
                .notify_connect_address();
            if let Err(e) = pushers.send(&target, join_msg.as_bytes()).await {
                log::warn!("[{}] Failed to notify routing {}: {}", log_name, addr, e);
            }
        }

        // Notify monitoring nodes.
        for addr in &ctx_state.monitoring_ips {
            let target = anna_server_common::threads::MonitoringThread::new(addr, base_offset)
                .notify_connect_address();
            if let Err(e) = pushers.send(&target, join_msg.as_bytes()).await {
                log::warn!("[{}] Failed to notify monitoring {}: {}", log_name, addr, e);
            }
        }

        log::info!("[{}] Sent join notifications", log_name);
    }

    log::info!("[{}] Entering event loop", log_name);

    // ── Event loop ──
    while !signal::shutdown_requested() {
        if signal::self_depart_requested() {
            let depart_done_addr = if !ctx_state.monitoring_ips.is_empty() {
                anna_server_common::threads::MonitoringThread::new(
                    &ctx_state.monitoring_ips[0],
                    base_offset,
                )
                .depart_done_connect_address()
            } else {
                String::new()
            };

            let msgs = handlers::self_depart::handle(&mut ctx_state, &depart_done_addr);
            for (addr, data) in &msgs {
                let _ = pushers.send(addr, data).await;
            }
            // Allow ZMQ to deliver depart notifications.
            tokio::time::sleep(Duration::from_secs(2)).await;
            break;
        }

        // Socket 3: user requests (GET/PUT/SCAN) — gets poll_timeout as the
        // loop's sleep. This is the hot path; all other sockets drain with
        // Duration::ZERO so they never add latency to user requests.
        if let Ok(Ok(msg)) = tokio::time::timeout(poll_timeout, request_puller.recv()).await {
            let data = msg_bytes(&msg);
            if let Ok(req) = KeyRequest::decode(data.as_slice()) {
                let msgs = if req.r#type == RequestType::Scan as i32 {
                    handlers::scan::handle(&ctx_state, &data)
                } else {
                    handlers::user_request::handle(&mut ctx_state, &data)
                };
                log::debug!(
                    "[{}] Handler produced {} outgoing messages",
                    log_name,
                    msgs.len()
                );
                for (addr, d) in &msgs {
                    if let Err(e) = pushers.send(addr, d).await {}
                }
            } else {
            }
        }

        // Socket 0: node_join — non-blocking.
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, join_puller.recv()).await {
            let data = msg_bytes(&msg);
            let serialized = String::from_utf8_lossy(&data);
            let msgs = handlers::node_join::handle(&mut ctx_state, &serialized);
            for (addr, data) in &msgs {
                let _ = pushers.send(addr, data).await;
            }
        }

        // Socket 1: node_depart — non-blocking.
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, depart_puller.recv()).await {
            let data = msg_bytes(&msg);
            let serialized = String::from_utf8_lossy(&data);
            let msgs = handlers::node_depart::handle(&mut ctx_state, &serialized);
            for (addr, data) in &msgs {
                let _ = pushers.send(addr, data).await;
            }
        }

        // Socket 2: self_depart relay from thread 0 to workers.
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, self_depart_puller.recv()).await {
            signal::request_self_depart();
            continue; // re-check self_depart_requested at top of loop.
        }

        // Socket 4: gossip.
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, gossip_puller.recv()).await {
            let data = msg_bytes(&msg);
            let msgs = handlers::gossip::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 5: replication response.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, replication_response_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            let msgs = handlers::replication_response::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 6: replication change.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, replication_change_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            let msgs = handlers::replication_change::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 7: cache IP response.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, cache_ip_response_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            handlers::cache_ip_response::handle(&mut ctx_state, &data);
        }

        // Socket 8: management node response.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, management_node_response_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            let msgs = handlers::management_node_response::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 9: cache registration.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, cache_registration_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            handlers::cache_registration::handle(&mut ctx_state, &data);
        }

        // ── Periodic: gossip ──
        if gossip_start.elapsed() >= gossip_period {
            // TODO: gossip local changeset to peers + cache update push
            gossip_start = Instant::now();
        }

        // ── Periodic: GC (expired key reaping) ──
        if gc_start.elapsed() >= gc_period {
            let reaped = handlers::utils::gc_reap_expired_keys(
                &mut ctx_state.stored_key_map,
                &mut ctx_state.serializers,
            );
            if reaped > 0 {
                log::debug!("[{}] GC reaped {} expired keys", log_name, reaped);
            }
            gc_start = Instant::now();
        }

        // ── Periodic: stats report ──
        if report_start.elapsed() >= report_period {
            // TODO: report storage stats, key access, cluster topology, kvs_members
            report_start = Instant::now();
        }

        // ── Join gossip drain ──
        if !ctx_state.join_gossip_map.is_empty() {
            let msgs = handlers::utils::build_gossip_messages(
                &ctx_state.join_gossip_map,
                &ctx_state.serializers,
                &ctx_state.stored_key_map,
            );
            for (addr, data) in &msgs {
                let _ = pushers.send(addr, data).await;
            }
            ctx_state.join_gossip_map.clear();

            // Remove keys this node is no longer responsible for.
            for key in ctx_state.join_remove_set.drain() {
                if let Some(kp) = ctx_state.stored_key_map.get(&key) {
                    let lt = kp.lattice_type() as i32;
                    if let Some(s) = ctx_state.serializers.get_mut(&lt) {
                        s.remove(&key);
                    }
                }
                ctx_state.stored_key_map.remove(&key);
            }
        }
    }

    log::info!("[{}] Event loop exited", log_name);
    Ok(())
}
