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
const NODE1_IP: &str = "127.0.0.1";
const NODE2_IP: &str = "127.0.0.2";

fn can_bind(ip: &str) -> bool {
    let addr: SocketAddr = format!("{}:0", ip).parse().unwrap();
    TcpListener::bind(addr).is_ok()
}

fn write_node_config(
    path: &Path,
    node_ip: &str,
    seed_ip: &str,
    replication_memory: u32,
    base_offset: u32,
    gossip_epoch: u32,
) {
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
ebs: ./
capacities:
  memory-cap: 1
  ebs-cap: 0
threads:
  memory: 1
  ebs: 1
  routing: 1
  benchmark: 1
ports:
  base_offset: {base_offset}
timings:
  gossip_epoch: {gossip_epoch}
  server_report_period: 5
  key_monitoring_period: 15
  monitoring_timeout: 10
  data_redistribute_batch: 50
  grace_period: 30
replication:
  memory: {replication_memory}
  ebs: 0
  minimum: 1
  local: 1
"
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
    let bin = server_bin_dir().join(name);
    if !bin.exists() {
        return None;
    }
    let child = Command::new(&bin)
        .args(["--config", &config.to_string_lossy()])
        .env("PATH", extra_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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

    fn start_full_node(&mut self, node_ip: &str, replication_memory: u32) {
        let config = self.config_dir.join(format!("node-{}.yml", node_ip));
        write_node_config(
            &config,
            node_ip,
            node_ip,
            replication_memory,
            self.base_offset,
            TEST_GOSSIP_EPOCH,
        );

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
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
        std::thread::sleep(Duration::from_secs(1));
    }

    fn start_kvs_node(&mut self, node_ip: &str, seed_ip: &str, replication_memory: u32) {
        let config = self.config_dir.join(format!("node-{}.yml", node_ip));
        write_node_config(
            &config,
            node_ip,
            seed_ip,
            replication_memory,
            self.base_offset,
            TEST_GOSSIP_EPOCH,
        );

        if let Some(child) = spawn_server("anna-kvs", &config, &server_path()) {
            self.processes.push(ServerProcess {
                child,
                label: format!("anna-kvs@{}", node_ip),
            });
        } else {
            self.shutdown();
            panic!("Server binary anna-kvs not found");
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    fn start_routing_only(&mut self, node_ip: &str) {
        let config = self.config_dir.join(format!("node-{}.yml", node_ip));
        write_node_config(
            &config,
            node_ip,
            node_ip,
            1,
            self.base_offset,
            TEST_GOSSIP_EPOCH,
        );

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

    fn kill_process(&mut self, label_substring: &str) {
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
        for proc in &mut self.processes {
            proc.child.kill().ok();
            proc.child.wait().ok();
        }
        self.processes.clear();
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
