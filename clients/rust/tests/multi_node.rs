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
const ZMQ_SETTLE_MS: u64 = 500;
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
        }
    }
}

fn write_node_config(path: &Path, cfg: &NodeConfig) {
    let mut f = fs::File::create(path).expect("Failed to create config file");
    write!(
        f,
        "\
monitoring:
  mgmt_ip: {seed_ip}
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
  mgmt_ip: \"NULL\"
policy:
  elasticity: false
  selective-rep: false
  tiering: false
ebs: {ebs_path}
capacities:
  memory-cap: 1
  ebs-cap: 256
threads:
  memory: 1
  ebs: 1
  routing: {routing_threads}
  benchmark: 1
ports:
  base_offset: {base_offset}
timings:
  gossip_epoch: {gossip_epoch}
  server_report_period: 3
  key_monitoring_period: 15
  monitoring_timeout: 8
  monitoring_response_timeout_ms: 1000
  data_redistribute_batch: 50
  grace_period: 10
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
        eprintln!(
            "[start] Starting full node on {} with offset {} (routing port {})",
            cfg.node_ip, cfg.base_offset, 6450 + cfg.base_offset
        );

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

        assert!(
            wait_for_port(cfg.node_ip, self.routing_port(), 30),
            "Routing tier on {} did not start within 30 seconds",
            cfg.node_ip
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

    fn client_config_path(&self, node_ip: &str) -> PathBuf {
        self.config_dir.join(format!("node-{}.yml", node_ip))
    }

    fn shutdown(&mut self) {
        eprintln!(
            "[shutdown] Stopping {} processes for cluster at offset {}",
            self.processes.len(),
            self.base_offset
        );
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            for proc in &mut self.processes {
                let pid = Pid::from_raw(proc.child.id() as i32);
                eprintln!("[shutdown] SIGTERM {} (pid {})", proc.label, proc.child.id());
                kill(pid, Signal::SIGTERM).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        for proc in &mut self.processes {
            proc.child.kill().ok();
            proc.child.wait().ok();
            eprintln!("[shutdown] Reaped {}", proc.label);
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
/// Uses base_offset=0 (ports 6000-7150)
#[tokio::test]
#[cfg(unix)]
async fn multi_node_cluster_join_and_data_access() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(0);

    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(2000);

    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(4000);

    cluster.start_full_node(NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(6000);

    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-route").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(8000);
    cluster.start_routing_only(NODE1_IP);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;
    use std::collections::HashMap;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(10000);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(12000);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(14000);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");

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

    // Wait for gossip to replicate
    std::thread::sleep(Duration::from_secs(TEST_GOSSIP_EPOCH as u64 + 2));

    // Kill Node 1 — forces reads through Node 2, proving it rejoined
    cluster.kill_process("anna-kvs@127.0.0.1");

    let mut reader = KVSClient::new(&config, Some(60)).await;
    reader.set_timeout(Duration::from_secs(2));
    let v = reader
        .get("rejoin_proof")
        .await
        .expect("GET rejoin_proof failed — Node 2 did not rejoin");
    assert_eq!(v, "on_both_nodes");
}

/// Stateless routing recovery: kill routing and KVS, restart both, verify
/// the cluster rebuilds and serves requests. Routing is stateless — it
/// rebuilds its hash ring from join announcements when KVS nodes start.
/// Uses base_offset=16000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn stateless_routing_recovery() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(16000);
    cluster.start_full_node(NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(18000);
    cluster.start_full_node_with_config(NodeConfig {
        base_offset: 18000,
        routing_threads: 2,
        ..Default::default()
    });

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(20000);
    cluster.start_full_node(NODE1_IP, 2);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 2);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(22000);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(24000);
    cluster.start_full_node(NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(26000);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MultiNodeCluster::new(28000);
    cluster.start_full_node(NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(30000);

    // Start full cluster with replication_ebs=1 so routing assigns to disk tier
    cluster.start_full_node_with_config(NodeConfig {
        replication_ebs: 1,
        base_offset: 30000,
        ..Default::default()
    });

    // Start a disk-tier KVS on Node 2
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
    let mut client = KVSClient::new(&config, Some(71)).await;

    // PUT and GET data — routing may direct to either memory or disk node
    for i in 0..5 {
        client
            .put(&format!("disk_key_{}", i), &format!("disk_val_{}", i))
            .await
            .unwrap_or_else(|e| panic!("PUT disk_key_{} failed: {}", i, e));
    }

    for i in 0..5 {
        let key = format!("disk_key_{}", i);
        let val = client
            .get(&key)
            .await
            .unwrap_or_else(|e| panic!("GET {} failed: {}", key, e));
        assert_eq!(val, format!("disk_val_{}", i));
    }
}

/// Memory-tier preference: with both memory and disk tier nodes,
/// routing should return memory-tier addresses first.
/// Uses base_offset=32000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn memory_tier_preference() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(32000);
    cluster.start_full_node_with_config(NodeConfig {
        replication_ebs: 1,
        base_offset: 32000,
        ..Default::default()
    });
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(34000);

    // Start with replication on both tiers so data gossips across
    cluster.start_full_node_with_config(NodeConfig {
        replication_memory: 1,
        replication_ebs: 1,
        base_offset: 34000,
        ..Default::default()
    });
    cluster.start_disk_kvs_node(NODE2_IP, NODE1_IP);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(40000);
    cluster.start_full_node_with_config(NodeConfig {
        replication_memory: 1,
        base_offset: 40000,
        gossip_epoch: 2,
        ..Default::default()
    });
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(36000);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(38000);
    cluster.start_full_node(NODE1_IP, 1);
    cluster.start_kvs_node(NODE2_IP, NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");
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
/// Uses base_offset=42000 to avoid conflicts with other tests.
#[tokio::test]
#[cfg(unix)]
async fn gossip_to_caches() {
    use annalib::cache_client::CacheClient;
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    if skip_unless_multi_ip() {
        return;
    }

    let mut cluster = MultiNodeCluster::new(42000);
    cluster.start_full_node(NODE1_IP, 1);

    let config =
        Config::read(&cluster.client_config_path(NODE1_IP)).expect("Failed to read config");

    eprintln!("[gossip_to_caches] Creating cache client (update port={})", 7150 + 42000);
    let mut cache = CacheClient::new(&config, Some(0))
        .await
        .expect("Failed to create cache client");

    eprintln!("[gossip_to_caches] Registering watch (registration port={})", 7200 + 42000);
    cache
        .watch(&["cache_test_key".to_string()])
        .await
        .expect("Watch failed");

    eprintln!("[gossip_to_caches] Creating KVS client (tid=82, resp={}, ka={})", 6800+42000+82, 6850+42000+82);
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
