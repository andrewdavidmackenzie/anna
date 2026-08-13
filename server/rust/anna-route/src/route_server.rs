//! Routing server event loop.
//!
//! Mirrors `server/cpp/src/route/routing.cpp` `run()` function.

use std::collections::HashMap;
use std::time::Duration;

use anna_server_common::config::Config;
use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
use anna_server_common::metadata::Tier;
use anna_server_common::routing::{DEFAULT_LOCAL_REPLICATION, DEFAULT_METADATA_REPLICATION_FACTOR};
use anna_server_common::signal;
use anna_server_common::threads::{MonitoringThread, RoutingThread};
use anna_server_common::types::Address;
use omq_tokio::{Context, Message as ZmqMessage, Options, SocketType};

use crate::context::RouteContext;
use crate::handlers;

/// ZMQ PUSH socket cache.
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

fn msg_bytes(msg: &ZmqMessage) -> Vec<u8> {
    msg.iter().flat_map(|f| f.to_vec()).collect()
}

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

async fn bind_rep(ctx: &Context, addr: &str, name: &str) -> Result<omq_tokio::Socket, String> {
    let sock = ctx.socket(SocketType::Rep, Options::default());
    let endpoint = addr
        .parse()
        .map_err(|e| format!("Invalid address for {}: {} ({})", name, addr, e))?;
    sock.bind(endpoint)
        .await
        .map_err(|e| format!("Failed to bind {} on {}: {}", name, addr, e))?;
    Ok(sock)
}

/// Run the routing event loop for a single thread.
pub async fn run(
    thread_id: u32,
    config: &Config,
    ip: &str,
    thread_count: u32,
    monitoring_ips: Vec<Address>,
) -> Result<(), String> {
    let base_offset = config.ports.base_offset;
    let rt = RoutingThread::new(ip, thread_id, base_offset);

    let log_name = format!("route_{}", thread_id);
    log::info!("[{}] Starting routing thread", log_name);

    // ── ZMQ sockets ──
    let ctx = Context::new();

    let seed_responder = bind_rep(&ctx, &rt.seed_connect_address(), "seed").await?;
    let notify_puller = bind_pull(&ctx, &rt.notify_connect_address(), "notify").await?;
    let replication_response_puller = bind_pull(
        &ctx,
        &rt.replication_response_connect_address(),
        "rep_response",
    )
    .await?;
    let replication_change_puller =
        bind_pull(&ctx, &rt.replication_change_connect_address(), "rep_change").await?;
    let key_address_puller =
        bind_pull(&ctx, &rt.key_address_connect_address(), "key_address").await?;

    let mut pushers = SocketCache::new(ctx.clone());

    // ── Initialize hash rings ──
    let mut local_hash_rings = HashMap::new();
    for tid in 0..thread_count {
        let mut l_ring = local_hash_rings
            .entry(Tier::Memory)
            .or_insert_with(ConsistentHashRing::new);
        l_ring.insert(ip, ip, tid, base_offset, DEFAULT_VIRTUAL_THREAD_NUM, false);
    }

    // ── Build context ──
    let mut ctx_state = RouteContext {
        thread_id,
        ip: ip.to_string(),
        rt: rt.clone(),
        thread_count,
        global_hash_rings: HashMap::new(),
        local_hash_rings,
        key_replication_map: HashMap::new(),
        pending_requests: HashMap::new(),
        default_local_replication: config.replication.local,
        metadata_replication_factor: DEFAULT_METADATA_REPLICATION_FACTOR,
        monitoring_ips: monitoring_ips.clone(),
        seed: 42 + thread_id,
    };

    // ── Thread 0: notify monitoring ──
    if thread_id == 0 {
        let join_msg = format!("join:ROUTING:{}:NULL", ip);
        for mon_ip in &monitoring_ips {
            let target = MonitoringThread::new(mon_ip, base_offset).notify_connect_address();
            let _ = pushers.send(&target, join_msg.as_bytes()).await;
        }
        log::info!("[{}] Sent join notification to monitoring", log_name);
    }

    let poll_timeout = Duration::from_millis(100);

    log::info!("[{}] Entering event loop", log_name);

    // ── Event loop ──
    while !signal::shutdown_requested() {
        // Socket 0: seed responder (REP — recv then send).
        if let Ok(Ok(msg)) = tokio::time::timeout(poll_timeout, seed_responder.recv()).await {
            let _request = msg_bytes(&msg);
            let response = handlers::seed::handle(&ctx_state);
            seed_responder.send(ZmqMessage::from(response)).await.ok();
        }

        // Socket 1: membership notifications.
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, notify_puller.recv()).await {
            let data = msg_bytes(&msg);
            let serialized = String::from_utf8_lossy(&data);
            let msgs = handlers::membership::handle(&mut ctx_state, &serialized);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 2: replication responses.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, replication_response_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            let msgs = handlers::replication_response::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 3: replication changes.
        if let Ok(Ok(msg)) =
            tokio::time::timeout(Duration::ZERO, replication_change_puller.recv()).await
        {
            let data = msg_bytes(&msg);
            let msgs = handlers::replication_change::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }

        // Socket 4: key address requests.
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::ZERO, key_address_puller.recv()).await {
            let data = msg_bytes(&msg);
            let msgs = handlers::address::handle(&mut ctx_state, &data);
            for (addr, d) in &msgs {
                let _ = pushers.send(addr, d).await;
            }
        }
    }

    log::info!("[{}] Event loop exited", log_name);
    Ok(())
}
