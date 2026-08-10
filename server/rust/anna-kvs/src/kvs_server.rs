//! KVS event loop — receives and dispatches requests via ZMQ.
//!
//! Mirrors `server/cpp/src/kvs/server.cpp` `run()` function.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anna_server_common::config::Config;
use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
use anna_server_common::metadata::{is_metadata, Tier, TierMetadata};
use anna_server_common::proto::kvs::{KeyRequest, LatticeType, RequestType};
use anna_server_common::routing::{DEFAULT_LOCAL_REPLICATION, DEFAULT_METADATA_REPLICATION_FACTOR};
use anna_server_common::signal;
use anna_server_common::threads::ServerThread;
use anna_server_common::types::Address;
use omq_tokio::{Context, Message as ZmqMessage, Options, SocketType};
use prost::Message;

use crate::context::KvsContext;
use crate::handlers;
use crate::storage::memory;
use crate::storage::SerializerMap;

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

/// Run the KVS event loop for a single thread.
pub async fn run(
    thread_id: u32,
    config: &Config,
    public_ip: &str,
    private_ip: &str,
    seed_ip: &str,
    self_tier: Tier,
    thread_count: u32,
    self_join_count: i32,
    routing_ips: Vec<Address>,
    monitoring_ips: Vec<Address>,
) {
    let base_offset = config.ports.base_offset;
    let wt = ServerThread::new(public_ip, private_ip, thread_id, base_offset);

    let log_name = format!("kvs_{}", thread_id);
    log::info!("[{}] Starting KVS thread", log_name);

    // ── ZMQ context and sockets ──
    let ctx = Context::new();

    let join_puller = ctx.socket(SocketType::Pull, Options::default());
    join_puller
        .bind(wt.node_join_connect_address().parse().unwrap())
        .await
        .expect("bind join_puller");

    let depart_puller = ctx.socket(SocketType::Pull, Options::default());
    depart_puller
        .bind(wt.node_depart_connect_address().parse().unwrap())
        .await
        .expect("bind depart_puller");

    let self_depart_puller = ctx.socket(SocketType::Pull, Options::default());
    self_depart_puller
        .bind(wt.self_depart_connect_address().parse().unwrap())
        .await
        .expect("bind self_depart_puller");

    let request_puller = ctx.socket(SocketType::Pull, Options::default());
    request_puller
        .bind(wt.key_request_bind_address().parse().unwrap())
        .await
        .expect("bind request_puller");

    let gossip_puller = ctx.socket(SocketType::Pull, Options::default());
    gossip_puller
        .bind(wt.gossip_connect_address().parse().unwrap())
        .await
        .expect("bind gossip_puller");

    let replication_response_puller = ctx.socket(SocketType::Pull, Options::default());
    replication_response_puller
        .bind(wt.replication_response_connect_address().parse().unwrap())
        .await
        .expect("bind replication_response_puller");

    let replication_change_puller = ctx.socket(SocketType::Pull, Options::default());
    replication_change_puller
        .bind(wt.replication_change_connect_address().parse().unwrap())
        .await
        .expect("bind replication_change_puller");

    let cache_ip_response_puller = ctx.socket(SocketType::Pull, Options::default());
    cache_ip_response_puller
        .bind(wt.cache_ip_response_connect_address().parse().unwrap())
        .await
        .expect("bind cache_ip_response_puller");

    let management_node_response_puller = ctx.socket(SocketType::Pull, Options::default());
    management_node_response_puller
        .bind(
            wt.management_node_response_connect_address()
                .parse()
                .unwrap(),
        )
        .await
        .expect("bind management_node_response_puller");

    let cache_registration_puller = ctx.socket(SocketType::Pull, Options::default());
    cache_registration_puller
        .bind(wt.cache_registration_connect_address().parse().unwrap())
        .await
        .expect("bind cache_registration_puller");

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

    // ── Initialize serializers ──
    let mut serializers: SerializerMap = HashMap::new();
    serializers.insert(
        LatticeType::Lww as i32,
        Box::new(memory::LwwSerializer::new()),
    );
    serializers.insert(
        LatticeType::Set as i32,
        Box::new(memory::SetSerializer::new()),
    );
    // OrderedSet uses the same wire format as Set.
    serializers.insert(
        LatticeType::OrderedSet as i32,
        Box::new(memory::SetSerializer::new()),
    );
    serializers.insert(
        LatticeType::SingleCausal as i32,
        Box::new(memory::SingleCausalSerializer::new()),
    );
    serializers.insert(
        LatticeType::MultiCausal as i32,
        Box::new(memory::MultiCausalSerializer::new()),
    );
    serializers.insert(
        LatticeType::Priority as i32,
        Box::new(memory::PrioritySerializer::new()),
    );
    serializers.insert(
        LatticeType::Counter as i32,
        Box::new(memory::CounterSerializer::new()),
    );
    serializers.insert(
        LatticeType::OrSet as i32,
        Box::new(memory::OrSetSerializer::new()),
    );

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
    let report_period = Duration::from_secs(config.timings.server_report_period.into());
    let mut gossip_start = Instant::now();
    let mut gc_start = Instant::now();
    let mut report_start = Instant::now();

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

        // Socket 0: node_join — first socket gets poll_timeout.
        if let Ok(Ok(msg)) = tokio::time::timeout(poll_timeout, join_puller.recv()).await {
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

        // Socket 2: self_depart — trigger from other threads.
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, self_depart_puller.recv()).await {
            // self_depart_requested flag is already set by the signal handler.
            // This socket is for relay from thread 0 to worker threads.
            continue; // re-check self_depart_requested at top of loop.
        }

        // Socket 3: user requests (GET/PUT/SCAN).
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, request_puller.recv()).await {
            let data = msg_bytes(&msg);
            // Peek at request type to dispatch to scan or user_request.
            if let Ok(req) = KeyRequest::decode(data.as_slice()) {
                let msgs = if req.r#type == RequestType::Scan as i32 {
                    handlers::scan::handle(&ctx_state, &data)
                } else {
                    handlers::user_request::handle(&mut ctx_state, &data)
                };
                for (addr, d) in &msgs {
                    let _ = pushers.send(addr, d).await;
                }
            }
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

        // ── Periodic: GC ──
        if gc_start.elapsed() >= gossip_period {
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
}
