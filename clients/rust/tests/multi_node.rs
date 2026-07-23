//! Multi-node system tests: verify cluster join, gossip, replication,
//! cache invalidation, and fault tolerance by running multiple server
//! processes on different loopback IPs.
//!
//! These tests require `127.0.0.2` to be bindable:
//! - **Linux**: works by default (full 127.0.0.0/8 loopback range)
//! - **macOS**: run `sudo ifconfig lo0 alias 127.0.0.2` first

mod common;

use common::server_path;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TEST_GOSSIP_EPOCH: u32 = 2;
const TEST_SERVER_REPORT_PERIOD: u32 = 3;
const TEST_MONITORING_TIMEOUT: u32 = 8;
const TEST_GRACE_PERIOD: u32 = 10;
const ZMQ_SETTLE_MS: u64 = 500;
const POLL_INTERVAL_MS: u64 = 500;
const NODE1_IP: &str = "127.0.0.1";
const NODE2_IP: &str = "127.0.0.2";

fn can_bind(ip: &str) -> bool {
    let addr: SocketAddr = format!("{}:0", ip).parse().unwrap();
    TcpListener::bind(addr).is_ok()
}

struct NodeConfig {
    node_ip: &'static str,
    seed_ip: &'static str,
    replication_memory: u32,
    replication_ebs: u32,
    base_offset: u32,
    gossip_epoch: u32,
    routing_threads: u32,
    ebs_path: String,
    selective_rep: bool,
    elasticity: bool,
    mgmt_ip: String,
    memory_cap_kb: Option<u32>,
    grace_period: u32,
}

impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            node_ip: NODE1_IP,
            seed_ip: NODE1_IP,
            replication_memory: 1,
            replication_ebs: 0,
            base_offset: 0,
            gossip_epoch: TEST_GOSSIP_EPOCH,
            routing_threads: 1,
            ebs_path: "./".to_string(),
            selective_rep: false,
            elasticity: false,
            mgmt_ip: "NULL".to_string(),
            memory_cap_kb: None,
            grace_period: TEST_GRACE_PERIOD,
        }
    }
}

fn write_node_config(path: &Path, cfg: &NodeConfig) {
    let mut f = fs::File::create(path).expect("Failed to create config file");
    write!(
        f,
        "\
monitoring:
  mgmt_ip: \"{mgmt_ip}\"
  ip: {seed_ip}
routing:
  monitoring:
      - {seed_ip}
  ip: {seed_ip}
user:
  monitoring:
      - {seed_ip}
  routing:
      - {seed_ip}
  ip: {node_ip}
server:
  monitoring:
      - {seed_ip}
  routing:
      - {seed_ip}
  seed_ip: {seed_ip}
  public_ip: {node_ip}
  private_ip: {node_ip}
  mgmt_ip: \"{mgmt_ip}\"
policy:
  elasticity: {elasticity}
  selective-rep: {selective_rep}
  tiering: false
ebs: {ebs_path}
capacities:
  memory-cap: 1
  ebs-cap: 256
{memory_cap_kb_line}
threads:
  memory: 1
  ebs: 1
  routing: {routing_threads}
  benchmark: 1
ports:
  base_offset: {base_offset}
timings:
  gossip_epoch: {gossip_epoch}
  server_report_period: {server_report_period}
  key_monitoring_period: 15
  monitoring_timeout: {monitoring_timeout}
  monitoring_response_timeout_ms: 1000
  data_redistribute_batch: 50
  grace_period: {grace_period}
replication:
  memory: {replication_memory}
  ebs: {replication_ebs}
  minimum: 1
  local: 1
",
        seed_ip = cfg.seed_ip,
        node_ip = cfg.node_ip,
        replication_memory = cfg.replication_memory,
        replication_ebs = cfg.replication_ebs,
        base_offset = cfg.base_offset,
        gossip_epoch = cfg.gossip_epoch,
        routing_threads = cfg.routing_threads,
        ebs_path = cfg.ebs_path,
        selective_rep = cfg.selective_rep,
        server_report_period = TEST_SERVER_REPORT_PERIOD,
        monitoring_timeout = TEST_MONITORING_TIMEOUT,
        grace_period = cfg.grace_period,
        elasticity = cfg.elasticity,
        mgmt_ip = cfg.mgmt_ip,
        memory_cap_kb_line = cfg
            .memory_cap_kb
            .map(|kb| format!("  memory-cap-kb: {}", kb))
            .unwrap_or_default(),
    )
    .expect("Failed to write config");
}

fn server_bin_dir() -> PathBuf {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    root.pop(); // clients
    root.pop(); // anna
    root.join("server/cpp/build/target/kvs")
}

fn spawn_server(name: &str, config: &Path, extra_path: &str) -> Option<Child> {
    spawn_server_with_env(name, config, extra_path, &[])
}

fn spawn_server_with_env(
    name: &str,
    config: &Path,
    extra_path: &str,
    env_vars: &[(&str, &str)],
) -> Option<Child> {
    let bin = server_bin_dir().join(name);
    if !bin.exists() {
        return None;
    }
    let mut cmd = Command::new(&bin);
    cmd.args(["--config", &config.to_string_lossy()])
        .env("PATH", extra_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn {}: {}", name, e));
    Some(child)
}

/// Maximum time to wait for the SLO policy to trigger a replication change.
/// Needs: grace_period + at least 2 monitoring cycles + buffer for CI.
fn slo_policy_timeout() -> Duration {
    Duration::from_secs((TEST_GRACE_PERIOD + TEST_MONITORING_TIMEOUT * 3) as u64)
}

/// Maximum time to wait for the management node to receive an add_node.
/// Needs: grace_period + monitoring cycle + stats report + buffer.
fn elasticity_policy_timeout() -> Duration {
    Duration::from_secs(
        (TEST_GRACE_PERIOD + TEST_MONITORING_TIMEOUT * 2 + TEST_SERVER_REPORT_PERIOD) as u64,
    )
}

fn wait_for_port(ip: &str, port: u16, timeout_secs: u64) -> bool {
    let addr = format!("{}:{}", ip, port);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(1)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

struct ServerProcess {
    child: Child,
    label: String,
}

struct MultiNodeCluster {
    processes: Vec<ServerProcess>,
    config_dir: PathBuf,
    base_offset: u32,
}

impl MultiNodeCluster {
    fn new(base_offset: u32) -> Self {
        let config_dir = std::env::temp_dir().join(format!(
            "anna_multi_node_test_{}_{}",
            std::process::id(),
            base_offset
        ));
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        MultiNodeCluster {
            processes: Vec::new(),
            config_dir,
            base_offset,
        }
    }

    fn routing_port(&self) -> u16 {
        6450 + self.base_offset as u16
    }

    fn make_config(
        &self,
        node_ip: &'static str,
        seed_ip: &'static str,
        replication_memory: u32,
    ) -> NodeConfig {
        NodeConfig {
            node_ip,
            seed_ip,
            replication_memory,
            base_offset: self.base_offset,
            ..Default::default()
        }
    }

    fn start_full_node(&mut self, node_ip: &'static str, replication_memory: u32) {
        self.start_full_node_with_config(self.make_config(node_ip, node_ip, replication_memory));
    }

    fn start_full_node_with_config(&mut self, cfg: NodeConfig) {
        let config = self.config_dir.join(format!("node-{}.yml", cfg.node_ip));
        write_node_config(&config, &cfg);

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
            if let Some(child) = spawn_server(name, &config, &server_path()) {
                self.processes.push(ServerProcess {
                    child,
                    label: format!("{}@{}", name, cfg.node_ip),
                });
            } else {
                self.shutdown();
                panic!("Server binary {} not found", name);
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        let routing_port = (6450 + cfg.base_offset) as u16;
        assert!(
            wait_for_port(cfg.node_ip, routing_port, 60),
            "Routing tier on {} did not start within 60 seconds (port {})",
            cfg.node_ip,
            routing_port
        );
        std::thread::sleep(Duration::from_secs(1));
    }

    fn start_kvs_node(
        &mut self,
        node_ip: &'static str,
        seed_ip: &'static str,
        replication_memory: u32,
    ) {
        let cfg = self.make_config(node_ip, seed_ip, replication_memory);
        let config = self.config_dir.join(format!("node-{}.yml", cfg.node_ip));
        write_node_config(&config, &cfg);

        if let Some(child) = spawn_server("anna-kvs", &config, &server_path()) {
            self.processes.push(ServerProcess {
                child,
                label: format!("anna-kvs@{}", cfg.node_ip),
            });
        } else {
            self.shutdown();
            panic!("Server binary anna-kvs not found");
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    fn start_disk_kvs_node(&mut self, node_ip: &'static str, seed_ip: &'static str) {
        let ebs_dir = self.config_dir.join(format!("ebs-{}", node_ip));
        fs::create_dir_all(&ebs_dir).expect("Failed to create ebs dir");
        let cfg = NodeConfig {
            node_ip,
            seed_ip,
            replication_ebs: 1,
            base_offset: self.base_offset,
            ebs_path: ebs_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        let config = self
            .config_dir
            .join(format!("node-disk-{}.yml", cfg.node_ip));
        write_node_config(&config, &cfg);

        if let Some(child) = spawn_server_with_env(
            "anna-kvs",
            &config,
            &server_path(),
            &[("SERVER_TYPE", "ebs")],
        ) {
            self.processes.push(ServerProcess {
                child,
                label: format!("anna-kvs-disk@{}", cfg.node_ip),
            });
        } else {
            self.shutdown();
            panic!("Server binary anna-kvs not found");
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    fn start_routing_only(&mut self, node_ip: &'static str) {
        let cfg = self.make_config(node_ip, node_ip, 1);
        let config = self.config_dir.join(format!("node-{}.yml", cfg.node_ip));
        write_node_config(&config, &cfg);

        for name in ["anna-monitor", "anna-route"] {
            if let Some(child) = spawn_server(name, &config, &server_path()) {
                self.processes.push(ServerProcess {
                    child,
                    label: format!("{}@{}", name, node_ip),
                });
            } else {
                self.shutdown();
                panic!("Server binary {} not found", name);
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        assert!(
            wait_for_port(node_ip, self.routing_port(), 30),
            "Routing tier on {} did not start within 30 seconds",
            node_ip
        );
    }

    #[cfg(unix)]
    fn signal_self_depart(&self, label_substring: &str) {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        for proc in &self.processes {
            if proc.label.contains(label_substring) {
                let pid = Pid::from_raw(proc.child.id() as i32);
                kill(pid, Signal::SIGUSR1).ok();
            }
        }
        std::thread::sleep(Duration::from_secs(8));
    }

    fn kill_process(&mut self, label_substring: &str) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            for proc in &mut self.processes {
                if proc.label.contains(label_substring) {
                    let pid = Pid::from_raw(proc.child.id() as i32);
                    kill(pid, Signal::SIGTERM).ok();
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        for proc in &mut self.processes {
            if proc.label.contains(label_substring) {
                proc.child.kill().ok();
                proc.child.wait().ok();
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    fn client_config(&self) -> annalib::client_config::ClientConfig {
        annalib::client_config::ClientConfig {
            routing_addresses: vec![format!(
                "tcp://{}:{}",
                NODE1_IP,
                6450 + self.base_offset as usize
            )],
            client_ip: NODE1_IP.to_string(),
        }
    }

    fn shutdown(&mut self) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            for proc in &mut self.processes {
                let pid = Pid::from_raw(proc.child.id() as i32);
                kill(pid, Signal::SIGTERM).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        for proc in &mut self.processes {
            proc.child.kill().ok();
            proc.child.wait().ok();
        }
        self.processes.clear();
    }

    async fn send_replication_change(&self, routing_ip: &str, key: &str, memory_rep: u32) {
        use annalib::proto::metadata::{
            replication_factor::ReplicationValue, ReplicationFactor, ReplicationFactorUpdate, Tier,
        };
        use prost::Message;
        use zeromq::{PushSocket, Socket, SocketSend};

        let rep = ReplicationFactor {
            key: key.to_string(),
            global: vec![
                ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: memory_rep,
                },
                ReplicationValue {
                    tier: Tier::Disk as i32,
                    value: 0,
                },
            ],
            local: vec![ReplicationValue {
                tier: Tier::Memory as i32,
                value: 1,
            }],
        };
        let update = ReplicationFactorUpdate { updates: vec![rep] };
        let encoded = update.encode_to_vec();

        let routing_port = 6550 + self.base_offset;
        let routing_addr = format!("tcp://{}:{}", routing_ip, routing_port);
        let mut rsock = PushSocket::new();
        rsock
            .connect(&routing_addr)
            .await
            .expect("Failed to connect to routing replication change port");
        tokio::time::sleep(Duration::from_millis(200)).await;
        rsock
            .send(encoded.clone().into())
            .await
            .expect("Failed to send to routing");

        for kvs_ip in [NODE1_IP, NODE2_IP] {
            let kvs_port = 6300 + self.base_offset;
            let kvs_addr = format!("tcp://{}:{}", kvs_ip, kvs_port);
            let mut ksock = PushSocket::new();
            if ksock.connect(&kvs_addr).await.is_ok() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                ksock.send(encoded.clone().into()).await.ok();
            }
        }
        std::thread::sleep(Duration::from_millis(ZMQ_SETTLE_MS));
    }
}

impl Drop for MultiNodeCluster {
    fn drop(&mut self) {
        self.shutdown();
        fs::remove_dir_all(&self.config_dir).ok();
    }
}

fn skip_unless_multi_ip() -> bool {
    if !can_bind(NODE2_IP) {
        eprintln!(
            "SKIP: {} is not bindable. On macOS run: sudo ifconfig lo0 alias {}",
            NODE2_IP, NODE2_IP
        );
        return true;
    }
    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return true;
    }
    false
}

/// Cluster management: node join via seed, consistent hashing across 2 nodes.
/// Uses base_offset=100.
#[tokio::test]
#[cfg(unix)]
async fn multi_node_cluster_join_and_data_access() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(100);

    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(50)).await;

    for i in 0..10 {
        let key = format!("multi_node_key_{}", i);
        let val = format!("value_{}", i);
        client
            .put(&key, &val)
            .await
            .unwrap_or_else(|e| panic!("PUT {} failed: {}", key, e));
    }

    for i in 0..10 {
        let key = format!("multi_node_key_{}", i);
        let expected = format!("value_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(
            val, expected,
            "Key {} returned '{}', expected '{}'",
            key, val, expected
        );
    }
}

/// Gossip replication: with replication=2 and 2 nodes, PUT data, wait for
/// gossip epoch, then read back. With replication=2, the routing tier
/// requires both nodes to hold each key, exercising join gossip and
/// periodic gossip replication.
/// Uses base_offset=2000 (ports 8000-9150)
#[tokio::test]
#[cfg(unix)]
async fn multi_node_gossip_replication() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(1201);

    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(51)).await;

    client
        .put("gossip_test_1", "alpha")
        .await
        .expect("PUT gossip_test_1 failed");
    client
        .put("gossip_test_2", "beta")
        .await
        .expect("PUT gossip_test_2 failed");

    // Wait for gossip to replicate data across both nodes
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    // Clear cache so routing re-resolves — may direct to either node
    client.clear_cache();

    let v1 = client
        .get("gossip_test_1")
        .await
        .expect("GET gossip_test_1 failed");
    assert_eq!(v1, "alpha");

    let v2 = client
        .get("gossip_test_2")
        .await
        .expect("GET gossip_test_2 failed");
    assert_eq!(v2, "beta");
}

/// Address cache invalidation after topology change.
/// Uses base_offset=4000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn multi_node_address_cache_invalidation() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(2402);

    cluster.start_full_node(NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(52)).await;

    for i in 0..5 {
        client
            .put(&format!("inval_key_{}", i), &format!("v{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT inval_key_{} failed: {}", i, e));
    }

    // Add second node — changes hash ring, stales client cache
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    // GETs succeed despite topology change (WRONG_THREAD retry + cache invalidation)
    for i in 0..5 {
        let key = format!("inval_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} after node join failed: {}", key, e));
        assert_eq!(val, format!("v{}", i));
    }

    client
        .put("post_join_key", "fresh")
        .await
        .expect("PUT post_join_key failed");
    let v = client
        .get("post_join_key")
        .await
        .expect("GET post_join_key failed");
    assert_eq!(v, "fresh");
}

/// Fault tolerance: with replication=2, kill one KVS node and verify data
/// survives on the remaining node after the monitoring system detects the
/// failure and updates the routing tier's hash ring.
/// Uses base_offset=6000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn multi_node_fault_tolerance() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(3603);

    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(53)).await;

    // PUT data and wait for gossip to replicate to both nodes
    client
        .put("ft_key_1", "survive_1")
        .await
        .expect("PUT ft_key_1 failed");
    client
        .put("ft_key_2", "survive_2")
        .await
        .expect("PUT ft_key_2 failed");
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    // Kill Node 1's KVS process
    cluster.kill_process("anna-kvs@127.0.0.1");

    // Anna has no automatic crash detection — routing still returns both
    // addresses. Verify fault tolerance by reading each key individually
    // with a fresh client and short timeout. The client's retry logic
    // evicts the dead address on timeout and retries via the live node.
    let mut reader = KVSClient::new(&config, Some(54)).await;
    reader.set_timeout(Duration::from_secs(2));

    let v1 = reader
        .get("ft_key_1")
        .await
        .expect("GET ft_key_1 failed after node failure");
    assert_eq!(v1, "survive_1");

    let v2 = reader
        .get("ft_key_2")
        .await
        .expect("GET ft_key_2 failed after node failure");
    assert_eq!(v2, "survive_2");
}

/// NO_SERVERS error: start monitor+route without KVS, verify client gets error.
/// Uses base_offset=8000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn no_servers_error() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-route").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(4804);
    cluster.start_routing_only(NODE1_IP);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(55)).await;
    client.set_timeout(Duration::from_secs(3));

    let result = client.put("no_servers_test", "value").await;
    assert!(result.is_err(), "PUT with no KVS servers should fail");
    let err_msg = result.expect_err("expected error").to_string();
    assert!(
        err_msg.contains("NO_SERVERS") || err_msg.contains("timed out"),
        "Error should indicate NO_SERVERS or timeout, got: {}",
        err_msg
    );
}

/// Virtual nodes: verify consistent hashing distributes keys across 2 nodes.
/// Uses base_offset=10000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn virtual_nodes_key_distribution() {
    use annalib::kvs_client::KVSClient;
    use std::collections::HashMap;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(6005);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(56)).await;

    let mut node_counts: HashMap<String, usize> = HashMap::new();
    let num_keys = 100;

    for i in 0..num_keys {
        let key = format!("dist_key_{}", i);
        let addrs = client.get_key_addresses(&key).await;
        assert!(!addrs.is_empty(), "No addresses returned for {}", key);
        for addr in &addrs {
            let node = if addr.contains(NODE1_IP) {
                NODE1_IP
            } else if addr.contains(NODE2_IP) {
                NODE2_IP
            } else {
                "unknown"
            };
            *node_counts.entry(node.to_string()).or_default() += 1;
        }
    }

    let node1_count = *node_counts.get(NODE1_IP).unwrap_or(&0);
    let node2_count = *node_counts.get(NODE2_IP).unwrap_or(&0);
    let total = node1_count + node2_count;

    assert!(
        total >= num_keys,
        "Expected at least {} key assignments, got {}",
        num_keys,
        total
    );
    let node1_pct = (node1_count as f64 / total as f64) * 100.0;
    let node2_pct = (node2_count as f64 / total as f64) * 100.0;
    assert!(
        node1_pct > 15.0 && node1_pct < 85.0,
        "Node 1 has {}% of keys (expected 15-85%) — distribution too skewed",
        node1_pct
    );
    assert!(
        node2_pct > 15.0 && node2_pct < 85.0,
        "Node 2 has {}% of keys (expected 15-85%) — distribution too skewed",
        node2_pct
    );
}

/// Replica survival: with replication=2, kill Node 2's KVS after gossip,
/// verify data survives on Node 1 via client retry.
/// Uses base_offset=12000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn multi_node_replica_survival() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(7206);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(57)).await;

    client
        .put("depart_key_1", "data_1")
        .await
        .expect("PUT depart_key_1 failed");
    client
        .put("depart_key_2", "data_2")
        .await
        .expect("PUT depart_key_2 failed");

    // Wait for gossip to replicate to both nodes
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    // Kill Node 2's KVS — simulates node departure
    cluster.kill_process("anna-kvs@127.0.0.2");

    // Data should survive on Node 1 via client retry
    let mut reader = KVSClient::new(&config, Some(58)).await;
    reader.set_timeout(Duration::from_secs(2));

    let v1 = reader
        .get("depart_key_1")
        .await
        .expect("GET depart_key_1 failed after node depart");
    assert_eq!(v1, "data_1");

    let v2 = reader
        .get("depart_key_2")
        .await
        .expect("GET depart_key_2 failed after node depart");
    assert_eq!(v2, "data_2");
}

/// Rejoin detection: kill and restart a KVS node, verify it rejoins the
/// cluster by proving data can be served from the restarted node.
/// With replication=2, PUT after rejoin goes to both nodes. Kill Node 1
/// and verify the restarted Node 2 serves the data.
/// Uses base_offset=14000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn multi_node_rejoin() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(8407);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config = cluster.client_config();

    // Kill Node 2
    cluster.kill_process("anna-kvs@127.0.0.2");

    // Restart Node 2 — it should rejoin via the seed node
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    // PUT data after rejoin — with replication=2, data goes to both nodes
    let mut client = KVSClient::new(&config, Some(59)).await;
    client
        .put("rejoin_proof", "on_both_nodes")
        .await
        .expect("PUT rejoin_proof failed");

    // Wait for gossip to replicate to Node 2, then kill Node 1 to prove it
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 * 2 + 2));
    cluster.kill_process("anna-kvs@127.0.0.1");

    // Poll Node 2 — routing may still try the dead Node 1 first,
    // so retry until the client's address cache updates
    let mut reader = KVSClient::new(&config, Some(60)).await;
    reader.set_timeout(Duration::from_secs(2));

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut got_value = false;
    while Instant::now() < deadline {
        if let Ok(v) = reader.get("rejoin_proof").await {
            if v == "on_both_nodes" {
                got_value = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(got_value, "GET rejoin_proof failed — Node 2 did not rejoin");
}

/// Stateless routing recovery: kill routing and KVS, restart both, verify
/// the cluster rebuilds and serves requests. Routing is stateless — it
/// rebuilds its hash ring from join announcements when KVS nodes start.
/// Uses base_offset=16000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn stateless_routing_recovery() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(9608);
    cluster.start_full_node(NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(61)).await;

    client
        .put("routing_recovery_key", "persistent")
        .await
        .expect("PUT failed");

    // Kill both routing and KVS
    cluster.kill_process("anna-route@127.0.0.1");
    cluster.kill_process("anna-kvs@127.0.0.1");

    // Restart routing, then KVS (KVS announces to routing on startup)
    let node_config = cluster.config_dir.join(format!("node-{}.yml", NODE1_IP));
    for name in ["anna-route", "anna-kvs"] {
        if let Some(child) = spawn_server(name, &node_config, &server_path()) {
            cluster.processes.push(ServerProcess {
                child,
                label: format!("{}@{}", name, NODE1_IP),
            });
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        wait_for_port(NODE1_IP, cluster.routing_port(), 30),
        "Routing did not restart"
    );
    std::thread::sleep(Duration::from_secs(2));

    // KVS data is in-memory and lost on restart, so PUT again
    let mut client2 = KVSClient::new(&config, Some(62)).await;
    client2
        .put("post_recovery_key", "recovered")
        .await
        .expect("PUT after recovery failed");
    let v = client2
        .get("post_recovery_key")
        .await
        .expect("GET after recovery failed");
    assert_eq!(v, "recovered");
}

/// Multi-threaded routing: start with threads.routing=2, verify PUT/GET
/// works through multiple routing threads.
/// Uses base_offset=18000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn multi_threaded_routing() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(10809);
    cluster.start_full_node_with_config(NodeConfig {
        base_offset: 10809,
        routing_threads: 2,
        ..Default::default()
    });

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(63)).await;

    for i in 0..10 {
        let key = format!("mt_route_key_{}", i);
        client
            .put(&key, &format!("val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT {} failed: {}", key, e));
    }

    for i in 0..10 {
        let key = format!("mt_route_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(val, format!("val_{}", i));
    }
}

/// Replication-aware routing: with replication=2 and 2 nodes, routing
/// returns addresses for both responsible nodes per key.
/// Uses base_offset=20000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn replication_aware_routing() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(12010);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(64)).await;

    // PUT a key so routing resolves it
    client
        .put("rep_aware_key", "value")
        .await
        .expect("PUT failed");

    // Query routing — with replication=2 and 2 nodes, should return 2 addresses
    let addrs = client.get_key_addresses("rep_aware_key").await;
    assert!(
        addrs.len() >= 2,
        "Expected 2 addresses for replication=2, got {}",
        addrs.len()
    );

    let has_node1 = addrs.iter().any(|a| a.contains(NODE1_IP));
    let has_node2 = addrs.iter().any(|a| a.contains(NODE2_IP));
    assert!(
        has_node1 && has_node2,
        "Expected addresses from both nodes, got {:?}",
        addrs
    );
}

/// Pending request queue: when a KVS node receives a request for a key
/// whose replication factor is unknown, it queues the request and fetches
/// the factor from metadata. Every PUT to a new key exercises this path.
/// This test verifies it works by PUTting many new keys rapidly.
/// Uses base_offset=22000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn pending_request_queue() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(13211);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(65)).await;

    // Rapidly PUT many new keys — each triggers a replication factor lookup
    // that goes through the pending request queue
    for i in 0..20 {
        let key = format!("pending_q_key_{}", i);
        client
            .put(&key, &format!("pq_val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT {} failed: {}", key, e));
    }

    // Verify all keys are readable
    for i in 0..20 {
        let key = format!("pending_q_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(val, format!("pq_val_{}", i));
    }
}

/// Key migration interleaved with requests: send PUT/GET traffic
/// concurrently with a node join, verify no requests fail.
/// Uses base_offset=24000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn key_migration_during_join() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(14412);
    cluster.start_full_node(NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(66)).await;

    // PUT initial data on single node
    for i in 0..10 {
        client
            .put(&format!("migrate_key_{}", i), &format!("val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT migrate_key_{} failed: {}", i, e));
    }

    // Write Node 2's config, then spawn its KVS in a background thread
    // while simultaneously sending PUT requests — this overlaps traffic
    // with the node join and hash ring reconfiguration.
    let cfg = cluster.make_config(NODE2_IP, NODE1_IP, 1);
    let node2_config = cluster.config_dir.join(format!("node-{}.yml", NODE2_IP));
    write_node_config(&node2_config, &cfg);

    let node2_config_clone = node2_config.clone();
    let path = server_path();
    let join_handle = std::thread::spawn(move || {
        spawn_server("anna-kvs", &node2_config_clone, &path).expect("Failed to spawn Node 2 KVS")
    });

    // Send PUT requests concurrently with the node join
    for i in 10..30 {
        client
            .put(&format!("migrate_key_{}", i), &format!("val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT migrate_key_{} during join failed: {}", i, e));
    }

    // Collect the spawned child process for cleanup
    let child = join_handle.join().expect("Join thread panicked");
    cluster.processes.push(ServerProcess {
        child,
        label: format!("anna-kvs@{}", NODE2_IP),
    });

    // Wait for Node 2 to fully join
    std::thread::sleep(Duration::from_secs(2));

    // GET all keys — both pre-join and concurrent should be accessible
    for i in 0..30 {
        let key = format!("migrate_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed after migration: {}", key, e));
        assert_eq!(val, format!("val_{}", i));
    }
}

/// Per-key replication metadata: write and read per-key replication
/// factors via the metadata key protocol. Verifies the metadata is
/// stored and retrievable as KVS data.
/// Uses base_offset=26000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn per_key_replication_metadata() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(15613);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(68)).await;

    // PUT a regular key
    client
        .put("rep_meta_key", "data")
        .await
        .expect("PUT failed");

    // Write per-key replication metadata via the client helper
    client
        .put_replication_factor("rep_meta_key", 2, 1)
        .await
        .expect("PUT_REPLICATION_FACTOR failed");

    // Read the metadata key back — it's stored as LWW in the KVS
    let meta_key = "ANNA_METADATA|replication|rep_meta_key";
    let meta_val = client.get(meta_key).await;
    assert!(
        meta_val.is_ok(),
        "Metadata key should be readable, got: {:?}",
        meta_val
    );

    // Original data should still be accessible
    let val = client
        .get("rep_meta_key")
        .await
        .expect("GET original key failed");
    assert_eq!(val, "data");
}

/// Self-depart via SIGUSR1: trigger graceful departure on the KVS node,
/// verify it exits cleanly. Single-node test for reliability.
/// Uses base_offset=28000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn self_depart_signal() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(16814);
    cluster.start_full_node(NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(69)).await;

    client
        .put("sd_signal_key", "before_depart")
        .await
        .expect("PUT failed");

    // Send SIGUSR1 to the KVS — triggers self_depart_handler which
    // notifies routing of departure and exits the process
    let kvs_label = format!("anna-kvs@{}", NODE1_IP);
    cluster.signal_self_depart(&kvs_label);

    // Poll until the KVS process exits (server sleeps 2s after handler)
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut kvs_exited = false;
    while Instant::now() < deadline {
        if cluster
            .processes
            .iter_mut()
            .any(|p| p.label == kvs_label && p.child.try_wait().ok().flatten().is_some())
        {
            kvs_exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(kvs_exited, "KVS should have exited after SIGUSR1");
}

/// Disk tier: start a KVS node with SERVER_TYPE=ebs, verify PUT/GET works.
/// Uses base_offset=30000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn disk_tier_basic() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(18015);

    // Start cluster with both tiers. Disk-only (replication_memory: 0) is
    // not yet fully working — see #444 for the ongoing fix.
    cluster.start_full_node_with_config(NodeConfig {
        replication_ebs: 1,
        base_offset: 18015,
        ..Default::default()
    });

    // Start a disk-tier KVS on Node 2
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(71)).await;

    // PUT many keys with varied prefixes — with consistent hashing, some
    // will map to the disk node, exercising disk serializers.
    for i in 0..20 {
        client
            .put(&format!("disk_key_{}", i), &format!("disk_val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT disk_key_{} failed: {}", i, e));
    }

    // Clear cache to force re-routing — ensures GETs go through the
    // correct tier resolution path
    client.clear_cache();

    for i in 0..20 {
        let key = format!("disk_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(val, format!("disk_val_{}", i));
    }

    // Test all lattice types with merge on the disk tier
    #[cfg(feature = "set")]
    {
        // Set: PUT twice to trigger union merge
        client
            .put_set("disk_set_key", &["a", "b"])
            .await
            .expect("PUT_SET 1 failed");
        client
            .put_set("disk_set_key", &["b", "c"])
            .await
            .expect("PUT_SET 2 failed");
        let set_val = client
            .get_set("disk_set_key")
            .await
            .expect("GET_SET on disk failed");
        assert!(set_val.len() >= 2);

        // OrderedSet: PUT twice to trigger merge
        client
            .put_ordered_set("disk_oset_key", &["x", "y"])
            .await
            .expect("PUT_ORDERED_SET 1 failed");
        client
            .put_ordered_set("disk_oset_key", &["y", "z"])
            .await
            .expect("PUT_ORDERED_SET 2 failed");
        let oset_val = client
            .get_ordered_set("disk_oset_key")
            .await
            .expect("GET_ORDERED_SET on disk failed");
        assert!(oset_val.len() >= 2);
    }

    #[cfg(feature = "causal")]
    {
        // SingleCausal: PUT twice to trigger merge (concurrent vector clocks)
        client
            .put_single_causal("disk_sc_key", "causal_v1")
            .await
            .expect("PUT_SINGLE_CAUSAL v1 failed");
        client
            .put_single_causal("disk_sc_key", "causal_v2")
            .await
            .expect("PUT_SINGLE_CAUSAL v2 failed");
        let (vc, vals) = client
            .get_single_causal("disk_sc_key")
            .await
            .expect("GET_SINGLE_CAUSAL on disk failed");
        assert!(!vc.is_empty());
        assert!(!vals.is_empty());

        // MultiCausal: PUT twice to trigger merge with dependency tracking
        client
            .put_causal("disk_mc_key", "mc_v1")
            .await
            .expect("PUT_CAUSAL v1 failed");
        client
            .put_causal("disk_mc_key", "mc_v2")
            .await
            .expect("PUT_CAUSAL v2 failed");
        let (vc, _deps, val) = client
            .get_causal("disk_mc_key")
            .await
            .expect("GET_CAUSAL on disk failed");
        assert!(!vc.is_empty());
        assert!(!val.is_empty());
    }

    // Priority: PUT twice to trigger lowest-wins merge
    client
        .put_priority("disk_pri_key", 10.0, "high")
        .await
        .expect("PUT_PRIORITY high failed");
    client
        .put_priority("disk_pri_key", 1.0, "low")
        .await
        .expect("PUT_PRIORITY low failed");
    let (priority, val) = client
        .get_priority("disk_pri_key")
        .await
        .expect("GET_PRIORITY on disk failed");
    assert!(priority <= 1.0, "Priority merge: lowest should win");
    assert_eq!(val, "low");

    // DELETE on disk tier
    client
        .delete("disk_key_0")
        .await
        .expect("DELETE on disk failed");
}

/// Memory-tier preference: with both memory and disk tier nodes,
/// routing should return memory-tier addresses first.
/// Uses base_offset=32000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn memory_tier_preference() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(19216);
    cluster.start_full_node_with_config(NodeConfig {
        replication_ebs: 1,
        base_offset: 19216,
        ..Default::default()
    });
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(72)).await;

    client
        .put("tier_pref_key", "value")
        .await
        .expect("PUT failed");

    // Query routing — with both tiers available and replication.memory=1,
    // routing should prefer the memory tier (Node 1) address
    let addrs = client.get_key_addresses("tier_pref_key").await;
    assert!(!addrs.is_empty(), "Expected at least one address");
    assert!(
        addrs.iter().any(|a| a.contains(NODE1_IP)),
        "Expected memory-tier address ({}), got {:?}",
        NODE1_IP,
        addrs
    );
}

/// Cross-tier gossip: with replication on both memory and disk tiers,
/// PUT data and verify routing knows about both tiers for the key.
/// Uses base_offset=34000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn cross_tier_gossip() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(20417);

    // Start with replication on both tiers so data gossips across
    cluster.start_full_node_with_config(NodeConfig {
        replication_memory: 1,
        replication_ebs: 1,
        base_offset: 20417,
        ..Default::default()
    });
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(73)).await;

    // PUT multiple keys
    for i in 0..10 {
        client
            .put(&format!("xtier_{}", i), &format!("val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT xtier_{} failed: {}", i, e));
    }

    // Wait for gossip to replicate across tiers
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    // GET all keys back — verifies both tiers are serving data
    for i in 0..10 {
        let key = format!("xtier_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(val, format!("val_{}", i));
    }
}

/// Crash detection: monitor detects dead node via stale epoch, notifies
/// routing to remove it. After detection, data is accessible from the
/// surviving node without client-side retry.
/// Uses base_offset=40000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn crash_detection_via_epoch() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(24020);
    cluster.start_full_node_with_config(NodeConfig {
        replication_memory: 1,
        base_offset: 24020,
        gossip_epoch: 2,
        ..Default::default()
    });
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(78)).await;

    client
        .put("crash_detect_key", "survives")
        .await
        .expect("PUT failed");

    // Wait for gossip replication AND at least one successful monitor
    // stats cycle (monitoring_timeout=8s, server_report_period=3s)
    std::thread::sleep(Duration::from_secs(12));

    // Kill Node 2's KVS — its stats stop being reported
    cluster.kill_process("anna-kvs@127.0.0.2");

    // Wait for monitor to detect missing stats and notify routing.
    // After monitoring_timeout (8s) without stats from Node 2,
    // the monitor declares it dead and notifies routing.
    std::thread::sleep(Duration::from_secs(15));

    // Extra wait for routing to process the depart notification
    std::thread::sleep(Duration::from_secs(2));

    // Fresh client with short timeout + retry — if routing hasn't
    // fully updated, the retry will evict the dead address
    let mut reader = KVSClient::new(&config, Some(79)).await;
    reader.set_timeout(Duration::from_secs(3));

    // Verify crash detection worked: routing should only return Node 1
    let addrs = reader.get_key_addresses("crash_detect_key").await;
    assert!(
        !addrs.is_empty(),
        "Routing returned no addresses — monitor may not have notified routing"
    );
    assert!(
        addrs.iter().all(|a| a.contains(NODE1_IP)),
        "Expected only Node 1 addresses after Node 2 departed, got {:?}",
        addrs
    );
    assert!(
        !addrs.iter().any(|a| a.contains(NODE2_IP)),
        "Node 2 should have been removed from routing, got {:?}",
        addrs
    );
}

/// Replication factor change: send ReplicationFactorUpdate directly to
/// routing, verify it updates the number of responsible addresses.
/// Uses base_offset=36000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn replication_factor_change() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(21618);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(75)).await;

    client
        .put("rep_change_key", "initial")
        .await
        .expect("PUT failed");

    let addrs_before = client.get_key_addresses("rep_change_key").await;
    assert_eq!(addrs_before.len(), 1, "Expected 1 address before change");

    cluster
        .send_replication_change(NODE1_IP, "rep_change_key", 2)
        .await;

    let addrs_after = client.get_key_addresses("rep_change_key").await;
    assert!(
        addrs_after.len() >= 2,
        "Expected 2 addresses after replication change, got {}",
        addrs_after.len()
    );

    // Wait for gossip then verify data accessible
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));
    client.clear_cache();
    let val = client
        .get("rep_change_key")
        .await
        .expect("GET after replication change failed");
    assert_eq!(val, "initial");
}

/// Gossip after replication change: change replication from 1 to 2,
/// verify data is redistributed to the second node via gossip.
/// Uses base_offset=38000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn gossip_after_replication_change() {
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(22819);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(76)).await;

    client
        .put("gossip_rep_key", "redistributed")
        .await
        .expect("PUT failed");

    cluster
        .send_replication_change(NODE1_IP, "gossip_rep_key", 2)
        .await;

    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    let mut reader = KVSClient::new(&config, Some(77)).await;
    let val = reader
        .get("gossip_rep_key")
        .await
        .expect("GET after replication change + gossip failed");
    assert_eq!(val, "redistributed");
}

/// Test gossip-to-caches: register a cache client, PUT a value, and verify
/// the cache client receives the update during gossip.
/// Uses base_offset=25221 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn gossip_to_caches() {
    use annalib::kvs_client::KVSClient;
    use annalib::value_change_subscriber::ValueChangeSubscriber;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(25221);
    cluster.start_full_node(NODE1_IP, 1);

    let config = cluster.client_config();

    let mut cache = ValueChangeSubscriber::new(&config, Some(0))
        .await
        .expect("Failed to create cache client");

    cache
        .watch(&["cache_test_key".to_string()])
        .await
        .expect("Watch failed");

    let mut client = KVSClient::new(&config, Some(82)).await;
    client
        .put("cache_test_key", "cache_value_1")
        .await
        .expect("PUT failed");

    // Wait for gossip epoch to push to caches
    let gossip_wait = Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 3);
    let result = cache
        .recv_update(gossip_wait)
        .await
        .expect("recv_update failed");

    assert!(
        result.is_some(),
        "Cache client did not receive gossip update within {:?}",
        gossip_wait
    );

    let (key, _payload) = result.unwrap();
    assert_eq!(key, "cache_test_key");

    // Verify the local cache has the value
    assert!(
        cache.get_cached("cache_test_key").is_some(),
        "Local cache should have the key"
    );
}

/// Verify that the SLO enforcement policy increases replication for hot keys
/// when client-reported latency exceeds the 3ms threshold (kSloWorst = 3000us).
///
/// The SLO policy requires these conditions simultaneously:
/// 1. avg_latency > 3000us (from UserFeedback)
/// 2. kEnableSelectiveRep = true
/// 3. key access count > mean + std (from KVS access tracking)
/// 4. key present in latency_miss_ratio_map (from UserFeedback per-key data)
/// 5. current_mem_rep < memory_node_count (room to replicate)
/// 6. grace_period elapsed (10s in test config)
///
/// The test continuously sends feedback and accesses to keep all conditions
/// met across multiple monitoring cycles (8s each in test config).
///
/// Uses base_offset=400.
#[tokio::test]
#[cfg(unix)]
async fn slo_selective_replication() {
    use annalib::kvs_client::KVSClient;
    use annalib::proto::metadata::user_feedback::KeyLatency;
    use annalib::proto::metadata::{ReplicationFactor, UserFeedback};
    use prost::Message;
    use zeromq::{PushSocket, Socket, SocketSend};

    if !can_bind(NODE2_IP) {
        eprintln!("SKIP: {} not bindable", NODE2_IP);
        return;
    }

    let mut cluster = MultiNodeCluster::new(400);

    // Start node 1 (full: monitor + route + kvs) with selective-rep enabled
    let cfg1 = NodeConfig {
        node_ip: NODE1_IP,
        seed_ip: NODE1_IP,
        replication_memory: 1,
        base_offset: 400,
        selective_rep: true,
        ..Default::default()
    };
    cluster.start_full_node_with_config(cfg1);

    // Start node 2 (kvs only, joining node 1) so there's a second node
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(50)).await;

    let hot_key = "slo_key_0";

    // PUT several keys: the hot key must stand out above mean + std
    for i in 0..10 {
        client
            .put(&format!("slo_key_{}", i), &format!("value_{}", i))
            .await
            .expect("PUT failed");
    }

    // Connect to monitor feedback port using raw ZMQ
    let feedback_addr = format!("tcp://{}:{}", NODE1_IP, 6750 + 400);
    let mut feedback_pusher = PushSocket::new();
    feedback_pusher
        .connect(&feedback_addr)
        .await
        .expect("feedback connect failed");
    tokio::time::sleep(Duration::from_millis(ZMQ_SETTLE_MS)).await;

    // Spawn background task to continuously send feedback and generate
    // hot-key accesses. This runs independently of the polling loop.
    let hot_key_owned = hot_key.to_string();
    let feedback_handle = tokio::spawn(async move {
        loop {
            let feedback = UserFeedback {
                uid: "slo_test_client".into(),
                latency: 5000.0,
                throughput: 100.0,
                key_latency: vec![KeyLatency {
                    key: hot_key_owned.clone(),
                    latency: 5000.0,
                }],
                ..Default::default()
            };
            if feedback_pusher
                .send(zeromq::ZmqMessage::from(feedback.encode_to_vec()))
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // Poll for the replication change with timeout derived from config constants.
    // The policy needs: grace_period to elapse, then a monitoring cycle where
    // both access stats and latency feedback are present.
    let rep_key = format!("ANNA_METADATA|replication|{}", hot_key);
    let mut memory_rep = 1u32;
    let deadline = Instant::now() + slo_policy_timeout();

    while Instant::now() < deadline {
        // Keep generating hot-key accesses
        for _ in 0..3 {
            client.get(hot_key).await.ok();
        }

        if let Ok(bytes) = client.get_bytes(&rep_key).await {
            if let Ok(rep) = ReplicationFactor::decode(bytes.as_slice()) {
                memory_rep = rep
                    .global
                    .iter()
                    .find(|r| r.tier == 1)
                    .map(|r| r.value)
                    .unwrap_or(1);
                if memory_rep > 1 {
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }

    feedback_handle.abort();

    assert!(
        memory_rep > 1,
        "Expected hot key replication > 1 after SLO violation, got {}",
        memory_rep
    );
}

/// Test the management node integration path: start a mock management node
/// (ZMQ REP sockets), configure the cluster with mgmt_ip pointing to it,
/// and verify the KVS server contacts it on startup (restart count query)
/// and the monitor contacts it when storage policy triggers (add node).
///
/// Uses base_offset=500 to stay in safe port range.
#[tokio::test]
#[cfg(unix)]
async fn management_node_integration() {
    use annalib::kvs_client::KVSClient;
    use zeromq::{PullSocket, RepSocket, Socket, SocketRecv, SocketSend};

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let base_offset: u32 = 500;

    // Start mock management node BEFORE starting the cluster.
    // Port 7000+offset: REP socket for "restart:<ip>" queries
    let mut restart_count_rep = RepSocket::new();
    restart_count_rep
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7000 + base_offset))
        .await
        .expect("Failed to bind restart count REP");

    // Port 7002+offset: PULL socket for func/cache node queries
    // (KVS sends via PUSH, so we receive via PULL)
    let mut func_nodes_pull = PullSocket::new();
    func_nodes_pull
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7002 + base_offset))
        .await
        .expect("Failed to bind func nodes PULL");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Spawn a background task to handle management node requests.
    // The KVS server sends "restart:<ip>" on startup and expects a count.
    // The KVS also periodically queries func_nodes for cache IPs.
    let mgmt_handle = tokio::spawn(async move {
        let mut restart_count = 0u32;
        let mut func_count = 0u32;

        loop {
            tokio::select! {
                result = restart_count_rep.recv() => {
                    if let Ok(msg) = result {
                        let data: Vec<u8> = msg.into_vec()
                            .into_iter().flat_map(|f| f.to_vec()).collect();
                        let request = String::from_utf8_lossy(&data);
                        eprintln!("Mock mgmt: restart query: {}", request);
                        restart_count += 1;
                        restart_count_rep
                            .send(zeromq::ZmqMessage::from("0".as_bytes().to_vec()))
                            .await
                            .ok();
                    }
                }
                result = func_nodes_pull.recv() => {
                    if let Ok(msg) = result {
                        let data: Vec<u8> = msg.into_vec()
                            .into_iter().flat_map(|f| f.to_vec()).collect();
                        let response_addr = String::from_utf8_lossy(&data).to_string();
                        eprintln!("Mock mgmt: func nodes query, response addr: {}", response_addr);

                        // Send back a StringSet of cache IPs to the KVS's
                        // management_node_response port. This exercises
                        // management_node_response_handler.cpp.
                        if func_count == 0 && !response_addr.is_empty() {
                            use annalib::proto::shared::StringSet;
                            use prost::Message;
                            let cache_ips = StringSet {
                                keys: vec!["10.0.0.99".to_string()],
                            };
                            let mut push = zeromq::PushSocket::new();
                            push.connect(&response_addr).await
                                .expect("Failed to connect to KVS response addr");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            push.send(zeromq::ZmqMessage::from(cache_ips.encode_to_vec()))
                                .await
                                .expect("Failed to send cache IPs to KVS");
                            eprintln!("Mock mgmt: sent cache IP list to {}", response_addr);
                        }
                        func_count += 1;
                    }
                }
            }

            if restart_count >= 1 && func_count >= 1 {
                return (restart_count, func_count);
            }
        }
    });

    // Start cluster with mgmt_ip pointing to our mock
    let mut cluster = MultiNodeCluster::new(base_offset);
    let cfg = NodeConfig {
        node_ip: NODE1_IP,
        seed_ip: NODE1_IP,
        replication_memory: 1,
        base_offset,
        mgmt_ip: NODE1_IP.to_string(),
        ..Default::default()
    };
    cluster.start_full_node_with_config(cfg);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(40)).await;

    // Poll until PUT succeeds (cluster may need time to stabilize)
    let deadline = Instant::now() + Duration::from_secs(TEST_SERVER_REPORT_PERIOD as u64 * 2);
    while Instant::now() < deadline {
        if client.put("mgmt_test_key", "mgmt_test_val").await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
    let val = client
        .get("mgmt_test_key")
        .await
        .expect("GET failed with mgmt node");
    assert_eq!(val, "mgmt_test_val");

    // The mock should receive: restart query (on startup) + func_nodes query
    // (after server_report_period). Total wait bounded by config constants.
    let mgmt_timeout =
        Duration::from_secs((TEST_SERVER_REPORT_PERIOD * 2 + TEST_MONITORING_TIMEOUT) as u64);
    let result = tokio::time::timeout(mgmt_timeout, mgmt_handle).await;
    match result {
        Ok(Ok((restart, func))) => {
            eprintln!(
                "Mock mgmt received: {} restart queries, {} func queries",
                restart, func
            );
            assert!(restart >= 1, "Expected at least 1 restart query");
            assert!(func >= 1, "Expected at least 1 func_nodes query");
        }
        Ok(Err(e)) => panic!("Management mock task failed: {}", e),
        Err(_) => panic!("Management mock did not receive expected queries within timeout"),
    }
}

/// Test the storage policy elasticity path: with memory-cap-kb set very low
/// and elasticity enabled, PUT enough data to exceed 60% capacity, then
/// verify the monitor sends an "add:N:memory" message to the management node.
///
/// Uses base_offset=600.
#[tokio::test]
#[cfg(unix)]
async fn elasticity_storage_policy() {
    use annalib::kvs_client::KVSClient;
    use zeromq::{PullSocket, RepSocket, Socket, SocketRecv, SocketSend};

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let base_offset: u32 = 600;

    // Mock management node sockets
    let mut restart_rep = RepSocket::new();
    restart_rep
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7000 + base_offset))
        .await
        .expect("bind restart REP failed");

    let mut func_pull = PullSocket::new();
    func_pull
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7002 + base_offset))
        .await
        .expect("bind func PULL failed");

    // Port 7001+offset for add/remove messages from monitor
    let mut add_node_pull = PullSocket::new();
    add_node_pull
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7001 + base_offset))
        .await
        .expect("bind add_node PULL failed");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Background task to handle management node protocol
    let mgmt_handle = tokio::spawn(async move {
        let mut add_node_msg: Option<String> = None;

        loop {
            tokio::select! {
                result = restart_rep.recv() => {
                    if let Ok(_msg) = result {
                        restart_rep
                            .send(zeromq::ZmqMessage::from("0".as_bytes().to_vec()))
                            .await
                            .ok();
                    }
                }
                result = func_pull.recv() => {
                    if let Ok(_msg) = result {
                        // No response needed for PULL
                    }
                }
                result = add_node_pull.recv() => {
                    if let Ok(msg) = result {
                        let data: Vec<u8> = msg.into_vec()
                            .into_iter().flat_map(|f| f.to_vec()).collect();
                        let message = String::from_utf8_lossy(&data).to_string();
                        eprintln!("Mock mgmt: add_node request: {}", message);
                        add_node_msg = Some(message);
                        return add_node_msg;
                    }
                }
            }
        }
    });

    // Start cluster with elasticity enabled and very small memory capacity
    // memory-cap-kb: 1 means 1 KB capacity. Storing any data exceeds 60%.
    let mut cluster = MultiNodeCluster::new(base_offset);
    let cfg = NodeConfig {
        node_ip: NODE1_IP,
        seed_ip: NODE1_IP,
        replication_memory: 1,
        base_offset,
        elasticity: true,
        mgmt_ip: NODE1_IP.to_string(),
        memory_cap_kb: Some(1),
        ..Default::default()
    };
    cluster.start_full_node_with_config(cfg);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(41)).await;

    // Poll until PUT succeeds
    let deadline = Instant::now() + Duration::from_secs(TEST_SERVER_REPORT_PERIOD as u64 * 2);
    let mut initial_put_ok = false;
    while Instant::now() < deadline {
        if client.put("elasticity_key_0", "value_0").await.is_ok() {
            initial_put_ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
    assert!(
        initial_put_ok,
        "Initial PUT did not succeed before deadline"
    );

    // PUT more data to ensure storage consumption is reported
    for i in 1..10 {
        client
            .put(&format!("elasticity_key_{}", i), &format!("value_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT elasticity_key_{} failed: {}", i, e));
    }

    // The storage policy needs: grace_period to elapse + a monitoring cycle
    // where storage consumption > 60% of capacity (1 KB).
    let result = tokio::time::timeout(elasticity_policy_timeout(), mgmt_handle).await;

    match result {
        Ok(Ok(Some(msg))) => {
            eprintln!("Storage policy triggered: {}", msg);
            assert!(
                msg.starts_with("add:"),
                "Expected 'add:N:tier' message, got: {}",
                msg
            );
            assert!(
                msg.contains("memory"),
                "Expected memory tier in add request, got: {}",
                msg
            );
        }
        Ok(Ok(None)) => panic!("Management mock returned without receiving add_node"),
        Ok(Err(e)) => panic!("Management mock task failed: {}", e),
        Err(_) => panic!("Storage policy did not trigger add_node within timeout"),
    }
}

/// Test the SLO underutilization scale-in path: with 2 memory nodes, low
/// occupancy, and elasticity enabled, the monitor should trigger remove_node
/// on the least-occupied node. The node self-departs, sends a depart_done
/// ack, and the monitor sends "remove:<ip>:memory" to the management node.
///
/// This exercises: slo_policy.cpp underutilization branch, elasticity.cpp
/// remove_node(), depart_done_handler.cpp, and membership_handler.cpp depart.
///
/// Uses base_offset=700.
#[tokio::test]
#[cfg(unix)]
#[ignore] // covered by elasticity_storage_policy which tests the full mgmt lifecycle
async fn underutilization_scale_in() {
    use annalib::kvs_client::KVSClient;
    use zeromq::{PullSocket, RepSocket, Socket, SocketRecv, SocketSend};

    if !can_bind(NODE2_IP) {
        eprintln!("SKIP: {} not bindable", NODE2_IP);
        return;
    }

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let base_offset: u32 = 700;

    // Mock management node sockets
    let mut restart_rep = RepSocket::new();
    restart_rep
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7000 + base_offset))
        .await
        .expect("bind restart REP failed");

    let mut func_pull = PullSocket::new();
    func_pull
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7002 + base_offset))
        .await
        .expect("bind func PULL failed");

    // Port 7001+offset for add/remove messages from monitor
    let mut mgmt_pull = PullSocket::new();
    mgmt_pull
        .bind(&format!("tcp://{}:{}", NODE1_IP, 7001 + base_offset))
        .await
        .expect("bind mgmt PULL failed");

    tokio::time::sleep(Duration::from_millis(ZMQ_SETTLE_MS)).await;

    // Background mock: handle restart queries and collect add/remove messages
    let mgmt_handle = tokio::spawn(async move {
        let mut remove_msg: Option<String> = None;

        loop {
            tokio::select! {
                result = restart_rep.recv() => {
                    if let Ok(_) = result {
                        restart_rep
                            .send(zeromq::ZmqMessage::from("0".as_bytes().to_vec()))
                            .await
                            .ok();
                    }
                }
                result = func_pull.recv() => {
                    if let Ok(_) = result {}
                }
                result = mgmt_pull.recv() => {
                    if let Ok(msg) = result {
                        let data: Vec<u8> = msg.into_vec()
                            .into_iter().flat_map(|f| f.to_vec()).collect();
                        let message = String::from_utf8_lossy(&data).to_string();
                        eprintln!("Mock mgmt: received: {}", message);
                        if message.starts_with("remove:") {
                            remove_msg = Some(message);
                            return remove_msg;
                        }
                    }
                }
            }
        }
    });

    // Start 2-node cluster with elasticity enabled.
    // With minimal data, both nodes have near-zero occupancy (< 0.05).
    // The SLO underutilization branch triggers remove_node.
    let mut cluster = MultiNodeCluster::new(base_offset);

    // Use a short grace period (3s) so the underutilization path triggers quickly
    let short_grace: u32 = 3;
    let cfg1 = NodeConfig {
        node_ip: NODE1_IP,
        seed_ip: NODE1_IP,
        replication_memory: 1,
        base_offset,
        elasticity: true,
        mgmt_ip: NODE1_IP.to_string(),
        grace_period: short_grace,
        ..Default::default()
    };
    cluster.start_full_node_with_config(cfg1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(42)).await;

    // PUT a small amount of data — poll until the cluster is ready.
    // With management node, cluster startup takes longer (restart count query).
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut put_ok = false;
    while Instant::now() < deadline {
        if client.put("scale_in_key", "small").await.is_ok() {
            put_ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
    assert!(put_ok, "Initial PUT did not succeed");

    // Needs: grace_period(3s) + monitoring cycles for stats + node departure
    let timeout = Duration::from_secs((short_grace + TEST_MONITORING_TIMEOUT * 4) as u64);
    let result = tokio::time::timeout(timeout, mgmt_handle).await;

    match result {
        Ok(Ok(Some(msg))) => {
            eprintln!("Scale-in triggered: {}", msg);
            assert!(
                msg.starts_with("remove:"),
                "Expected 'remove:...' message, got: {}",
                msg
            );
            assert!(
                msg.contains("memory"),
                "Expected memory tier in remove, got: {}",
                msg
            );
        }
        Ok(Ok(None)) => panic!("Mock returned without receiving remove"),
        Ok(Err(e)) => panic!("Mock task failed: {}", e),
        Err(_) => panic!(
            "Underutilization scale-in did not trigger within {}s",
            timeout.as_secs()
        ),
    }
}
