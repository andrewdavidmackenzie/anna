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

const GOSSIP_EPOCH_SECS: u64 = 12;
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

struct MultiNodeCluster {
    children: Vec<Child>,
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
            children: Vec::new(),
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
        );

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
            if let Some(child) = spawn_server(name, &config, &server_path()) {
                self.children.push(child);
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
        );

        if let Some(child) = spawn_server("anna-kvs", &config, &server_path()) {
            self.children.push(child);
        } else {
            self.shutdown();
            panic!("Server binary anna-kvs not found");
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    fn client_config_path(&self, node_ip: &str) -> PathBuf {
        self.config_dir.join(format!("node-{}.yml", node_ip))
    }

    fn shutdown(&mut self) {
        for child in &mut self.children {
            child.kill().ok();
            child.wait().ok();
        }
        self.children.clear();
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
    std::thread::sleep(Duration::from_secs(GOSSIP_EPOCH_SECS));

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
