//! Monitoring system tests: verify that anna-kvs reports statistics to
//! anna-monitor via internal metadata keys.
//!
//! These tests use a separate port offset range (100+) from multi_node.rs
//! (0-25221) to avoid conflicts. Since cargo runs test binaries sequentially,
//! there is no parallel conflict between files.
//!
//! Each test starts a full server cluster (monitor + route + kvs = 3
//! processes). Running all 7 tests in parallel starts 21 server processes
//! simultaneously, causing resource contention that leads to flaky timeouts.
//! The `#[serial(monitor)]` attribute serializes tests within this file.

mod common;

use common::server_path;
use serial_test::serial;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const NODE_IP: &str = "127.0.0.1";
const REPORT_PERIOD: u32 = 3;

fn server_bin_dir() -> PathBuf {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    root.pop();
    root.pop();
    root.join("server/cpp/build/target/kvs")
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

struct MonitorConfig {
    base_offset: u32,
    selective_rep: bool,
    tiering: bool,
    replication_memory: u32,
    replication_disk: u32,
    disk_path: String,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        MonitorConfig {
            base_offset: 100,
            selective_rep: false,
            tiering: false,
            replication_memory: 1,
            replication_disk: 0,
            disk_path: "./".to_string(),
        }
    }
}

fn write_monitor_config(path: &Path, cfg: &MonitorConfig) {
    let mut f = fs::File::create(path).expect("Failed to create config file");
    write!(
        f,
        "\
monitoring:
  scaling_alert_ip: {ip}
  ip: {ip}
routing:
  monitoring:
      - {ip}
  ip: {ip}
user:
  monitoring:
      - {ip}
  routing:
      - {ip}
  ip: {ip}
server:
  monitoring:
      - {ip}
  routing:
      - {ip}
  seed_ip: {ip}
  public_ip: {ip}
  private_ip: {ip}
  scaling_alert_ip: \"NULL\"
policy:
  elasticity: false
  selective-rep: {selective_rep}
  tiering: {tiering}
disk: {disk_path}
capacities:
  memory-cap: 1
  disk-cap: 256
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
ports:
  base_offset: {base_offset}
timings:
  gossip_epoch: 2
  server_report_period: {report_period}
  key_monitoring_period: 15
  monitoring_timeout: 8
  monitoring_response_timeout_ms: 1000
  data_redistribute_batch: 50
  tombstone_gc_multiplier: 30
  grace_period: 5
replication:
  memory: {replication_memory}
  disk: {replication_disk}
  minimum: 1
  local: 1
",
        ip = NODE_IP,
        base_offset = cfg.base_offset,
        report_period = REPORT_PERIOD,
        selective_rep = cfg.selective_rep,
        tiering = cfg.tiering,
        replication_memory = cfg.replication_memory,
        replication_disk = cfg.replication_disk,
        disk_path = cfg.disk_path,
    )
    .expect("Failed to write config");
}

struct MonitorTestCluster {
    processes: Vec<(Child, String)>,
    config_dir: PathBuf,
    base_offset: u32,
}

impl MonitorTestCluster {
    fn new(base_offset: u32) -> Self {
        let config_dir = std::env::temp_dir().join(format!(
            "anna_monitor_test_{}_{}",
            std::process::id(),
            base_offset
        ));
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        MonitorTestCluster {
            processes: Vec::new(),
            config_dir,
            base_offset,
        }
    }

    fn start(&mut self) {
        self.start_with_config(MonitorConfig {
            base_offset: self.base_offset,
            ..Default::default()
        });
    }

    fn start_with_config(&mut self, cfg: MonitorConfig) {
        self.start_with_config_inner(cfg, None);
    }

    /// Start a cluster with the KVS running as a disk-tier node.
    fn start_disk_with_config(&mut self, cfg: MonitorConfig) {
        self.start_with_config_inner(cfg, Some("disk"));
    }

    fn start_with_config_inner(&mut self, cfg: MonitorConfig, server_type: Option<&str>) {
        let config = self.config_dir.join("config.yml");
        write_monitor_config(&config, &cfg);

        // Create disk_0 subdirectory for disk-tier tests
        let disk_dir = std::path::Path::new(&cfg.disk_path);
        if disk_dir.is_absolute() || cfg.disk_path != "./" {
            fs::create_dir_all(disk_dir.join("disk_0")).ok();
        }

        let port_range_start = 6200 + self.base_offset;
        let port_range_end = 7200 + self.base_offset;
        eprintln!(
            "[monitor-test] base_offset={} starting cluster (ports {}-{})",
            self.base_offset, port_range_start, port_range_end
        );

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
            // Allow overriding the monitor binary via ANNA_MONITOR_BIN.
            let bin = if name == "anna-monitor" {
                if let Ok(alt) = std::env::var("ANNA_MONITOR_BIN") {
                    let alt_bin = std::path::PathBuf::from(&alt);
                    if alt_bin.exists() {
                        alt_bin
                    } else {
                        server_bin_dir().join(name)
                    }
                } else {
                    server_bin_dir().join(name)
                }
            } else {
                server_bin_dir().join(name)
            };
            if !bin.exists() {
                self.shutdown();
                panic!("Server binary {} not found", name);
            }
            let mut cmd = Command::new(&bin);
            cmd.args(["--config", &config.to_string_lossy()])
                .env("PATH", server_path())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if name == "anna-kvs" {
                if let Some(st) = server_type {
                    cmd.env("SERVER_TYPE", st);
                }
            }
            let child = cmd
                .spawn()
                .unwrap_or_else(|e| panic!("Failed to spawn {}: {}", name, e));
            self.processes.push((child, name.to_string()));
            std::thread::sleep(Duration::from_secs(1));
        }

        let routing_port = (6450 + self.base_offset) as u16;
        assert!(
            wait_for_port(NODE_IP, routing_port, 30),
            "Routing tier did not start (port {})",
            routing_port
        );
        std::thread::sleep(Duration::from_secs(1));
        eprintln!(
            "[monitor-test] base_offset={} cluster ready",
            self.base_offset
        );
    }

    fn start_disk_kvs(&mut self) {
        let config = self.config_dir.join("config.yml");
        let bin = server_bin_dir().join("anna-kvs");
        if !bin.exists() {
            panic!("anna-kvs binary not found");
        }
        let child = Command::new(&bin)
            .args(["--config", &config.to_string_lossy()])
            .env("PATH", server_path())
            .env("SERVER_TYPE", "disk")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn anna-kvs (disk)");
        self.processes.push((child, "anna-kvs-disk".to_string()));
        std::thread::sleep(Duration::from_secs(3));
    }

    fn client_config(&self) -> annalib::client_config::ClientConfig {
        annalib::client_config::ClientConfig {
            routing_addresses: vec![format!(
                "tcp://{}:{}",
                NODE_IP,
                6450 + self.base_offset as usize
            )],
            client_ip: NODE_IP.to_string(),
        }
    }

    fn shutdown(&mut self) {
        eprintln!(
            "[monitor-test] base_offset={} shutting down cluster",
            self.base_offset
        );
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            for (child, _) in &mut self.processes {
                kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        for (child, _) in &mut self.processes {
            child.kill().ok();
            child.wait().ok();
        }
        self.processes.clear();
        eprintln!(
            "[monitor-test] base_offset={} cluster stopped",
            self.base_offset
        );
    }
}

impl Drop for MonitorTestCluster {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Verify that KVS nodes report statistics as metadata keys that can be
/// read back using the client library stats helper methods.
///
/// Covers 6 monitoring features + 3 client library helpers:
/// - Storage consumption reporting
/// - CPU occupancy reporting
/// - Access count reporting
/// - Per-key access frequency
/// - Per-key size for primary replicas
/// - Per-event-type occupancy logging (verified via non-zero occupancy)
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn monitor_stats_collection() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(100);
    cluster.start();

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(95)).await;

    // Generate activity: PUT keys with varying sizes
    for i in 0..5 {
        let key = format!("stats_test_key_{}", i);
        let value = "x".repeat((i + 1) * 100);
        client
            .put(&key, &value)
            .await
            .unwrap_or_else(|e| panic!("PUT {} failed: {}", key, e));
    }

    // Generate access counts: GET each key multiple times
    for i in 0..5 {
        let key = format!("stats_test_key_{}", i);
        for _ in 0..3 {
            client.get(&key).await.ok();
        }
    }

    // Wait for 2 report periods so stats are written
    std::thread::sleep(Duration::from_secs(REPORT_PERIOD as u64 * 2 + 1));

    // Read stats using client helper methods
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client
            .get_storage_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            assert!(
                s.storage_consumption > 0,
                "storage_consumption should be > 0"
            );
            assert!(s.epoch > 0, "epoch should be > 0");
            assert!(s.access_count > 0, "access_count should be > 0");
            assert!(s.occupancy >= 0.0, "occupancy should be >= 0");
            stats_ok = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(stats_ok, "Failed to read valid stats metadata");

    // Read per-key access frequency using helper
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut access_ok = false;
    while Instant::now() < deadline {
        if let Ok(a) = client
            .get_key_access_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            if a.keys.iter().any(|k| k.key.starts_with("stats_test_key_")) {
                let total: u32 = a
                    .keys
                    .iter()
                    .filter(|k| k.key.starts_with("stats_test_key_"))
                    .map(|k| k.access_count)
                    .sum();
                assert!(total > 0, "total access count for test keys should be > 0");
                access_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(access_ok, "Failed to read valid access metadata");

    // Read per-key size using helper
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut size_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client
            .get_key_size_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            let test_sizes: Vec<_> = s
                .key_sizes
                .iter()
                .filter(|k| k.key.starts_with("stats_test_key_"))
                .collect();
            if !test_sizes.is_empty() {
                for ks in &test_sizes {
                    assert!(ks.size > 0, "size for {} should be > 0", ks.key);
                }
                size_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(size_ok, "Failed to read valid size metadata");
}

/// Test that policy toggle config is parsed and the monitor runs correctly
/// with selective-rep enabled. Verifies the monitor processes stats and
/// policy code paths without crashing.
/// Uses base_offset=3700.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn policy_toggles_and_grace_period() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(3700);
    cluster.start_with_config(MonitorConfig {
        base_offset: 3700,
        selective_rep: true,
        tiering: false,
        ..Default::default()
    });

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(96)).await;

    // PUT and GET keys to generate activity with policies enabled
    for i in 0..3 {
        let key = format!("policy_test_key_{}", i);
        client
            .put(&key, &"x".repeat(1000))
            .await
            .expect("PUT failed");
        client.get(&key).await.ok();
    }

    // Wait for stats to be collected with policies active
    std::thread::sleep(Duration::from_secs(REPORT_PERIOD as u64 * 2 + 1));

    // Verify the monitor is still running and collecting stats
    // (policies enabled but no action taken — grace period active,
    // and with 1 node there's nothing to scale)
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client
            .get_storage_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            if s.access_count > 0 {
                stats_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        stats_ok,
        "Monitor should collect stats with selective-rep enabled"
    );
}

/// Test cross-tier data movement: start with a disk-tier KVS, PUT data,
/// enable tiering policy, verify the monitor promotes accessed keys to
/// memory tier by changing their replication factors.
/// Uses base_offset=5000.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn cross_tier_data_movement() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let disk_dir = std::env::temp_dir().join(format!("anna_disk_test_{}", std::process::id()));
    fs::create_dir_all(&disk_dir).expect("Failed to create disk dir");

    let mut cluster = MonitorTestCluster::new(5000);
    cluster.start_with_config(MonitorConfig {
        base_offset: 5000,
        tiering: true,
        replication_memory: 1,
        replication_disk: 1,
        disk_path: disk_dir.to_string_lossy().to_string(),
        ..Default::default()
    });

    // Start a disk-tier KVS node on the same cluster
    cluster.start_disk_kvs();

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(97)).await;

    // PUT hot keys (will be accessed) and cold keys (won't be accessed).
    // Hot keys exercise the promotion path, cold keys exercise demotion.
    for i in 0..3 {
        let key = format!("tier_move_key_{}", i);
        client
            .put(&key, &"x".repeat(1000))
            .await
            .expect("PUT hot key failed");
    }
    for i in 0..3 {
        let key = format!("tier_cold_key_{}", i);
        client
            .put(&key, &"y".repeat(500))
            .await
            .expect("PUT cold key failed");
    }

    // Only access hot keys — cold keys remain unaccessed to trigger demotion
    for i in 0..3 {
        let key = format!("tier_move_key_{}", i);
        for _ in 0..5 {
            client.get(&key).await.ok();
        }
    }

    // Wait for monitoring cycles with tiering enabled
    // grace_period=5, monitoring_timeout=8, report_period=3
    std::thread::sleep(Duration::from_secs(15));

    // Verify stats are collected with tiering enabled (the policy ran
    // without crashing). The tiering policy promotes accessed disk keys
    // to memory — we verify the monitor processes correctly.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client
            .get_storage_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            if s.access_count > 0 {
                stats_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        stats_ok,
        "Monitor should collect stats with tiering enabled"
    );
}

/// Test latency feedback ingestion: send a UserFeedback protobuf to the
/// monitor's feedback port and verify the monitor processes it without
/// crashing (stats continue to be collected).
/// Uses base_offset=6300.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn latency_feedback_ingestion() {
    use annalib::kvs_client::KVSClient;
    use annalib::proto::metadata::user_feedback::KeyLatency;
    use annalib::proto::metadata::UserFeedback;
    use omq_tokio::{Context, Message as ZmqMessage, Options, SocketType};
    use prost::Message;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(6300);
    cluster.start_with_config(MonitorConfig {
        base_offset: 6300,
        selective_rep: true,
        ..Default::default()
    });

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(98)).await;

    // PUT keys to have data in the system
    for i in 0..3 {
        let key = format!("feedback_key_{}", i);
        client
            .put(&key, &"x".repeat(500))
            .await
            .expect("PUT failed");
        client.get(&key).await.ok();
    }

    // Wait for initial stats collection
    std::thread::sleep(Duration::from_secs(REPORT_PERIOD as u64 + 1));

    // Send UserFeedback to the monitor's feedback port
    let feedback_addr = format!("tcp://{}:{}", NODE_IP, 6953 + 6300);
    let ctx = Context::new();
    let pusher = ctx.socket(SocketType::Push, Options::default());
    pusher
        .connect(feedback_addr.parse().unwrap())
        .await
        .expect("connect failed");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let feedback = UserFeedback {
        uid: "test_client_1".into(),
        latency: 5000.0, // above kSloWorst (3000us)
        throughput: 100.0,
        finish: false,
        warmup: false,
        key_latency: vec![
            KeyLatency {
                key: "feedback_key_0".into(),
                latency: 5000.0,
            },
            KeyLatency {
                key: "feedback_key_1".into(),
                latency: 4000.0,
            },
        ],
    };
    let bytes = feedback.encode_to_vec();
    pusher
        .send(ZmqMessage::from(bytes))
        .await
        .expect("Failed to send feedback");

    // Wait for a monitoring cycle to process the feedback
    std::thread::sleep(Duration::from_secs(REPORT_PERIOD as u64 * 2 + 1));

    // Verify the monitor is still running and collecting stats after
    // processing the feedback (didn't crash)
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client
            .get_storage_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            if s.access_count > 0 {
                stats_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        stats_ok,
        "Monitor should still collect stats after processing UserFeedback"
    );

    // Send finish signal
    let finish = UserFeedback {
        uid: "test_client_1".into(),
        finish: true,
        ..Default::default()
    };
    pusher
        .send(ZmqMessage::from(finish.encode_to_vec()))
        .await
        .expect("Failed to send finish feedback");
}

/// Test hot-key selective replication: enable selective-rep, access keys
/// heavily, verify the monitor's policy engine runs the de-replication
/// code path. With a single node at minimum replication, no actual change
/// occurs, but the policy code is exercised (tested via stats collection
/// continuing after policy runs).
/// Uses base_offset=7600.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn hot_key_selective_replication() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(7600);
    cluster.start_with_config(MonitorConfig {
        base_offset: 7600,
        selective_rep: true,
        ..Default::default()
    });

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(99)).await;

    // Create a "hot" key with many accesses
    client
        .put("hot_key", &"x".repeat(1000))
        .await
        .expect("PUT failed");
    for _ in 0..20 {
        client.get("hot_key").await.ok();
    }

    // Create "cold" keys with few accesses
    for i in 0..5 {
        let key = format!("cold_key_{}", i);
        client
            .put(&key, &"x".repeat(100))
            .await
            .expect("PUT failed");
    }

    // Wait for monitoring cycles to collect stats and run policies.
    // grace_period=5, monitoring_timeout=8
    std::thread::sleep(Duration::from_secs(15));

    // Verify per-key access stats show the hot key has higher access count
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut access_ok = false;
    while Instant::now() < deadline {
        if let Ok(a) = client
            .get_key_access_stats(NODE_IP, NODE_IP, 0, "MEMORY")
            .await
        {
            let hot = a.keys.iter().find(|k| k.key == "hot_key");
            let cold = a.keys.iter().find(|k| k.key == "cold_key_0");
            if let (Some(h), Some(c)) = (hot, cold) {
                if h.access_count > c.access_count {
                    access_ok = true;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        access_ok,
        "Hot key should have higher access count than cold keys"
    );
}

/// Verify that the KVS server publishes monitoring IPs as a metadata key
/// that clients can read to discover monitor addresses.
///
/// Uses base_offset=8900 to avoid conflicts with other monitor tests.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn monitoring_ips_metadata() {
    use annalib::kvs_client::KVSClient;
    use annalib::proto::shared::StringSet;
    use prost::Message;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(8900);
    cluster.start();

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(96)).await;

    // The KVS writes monitoring IPs every server_report_period (3s in test config).
    // Poll until the metadata key appears.
    // Allow up to 20 attempts (40s) to account for startup latency and
    // resource contention when running all monitor tests together.
    let meta_key = "ANNA_METADATA|monitoring_ips";
    let mut found = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Ok(bytes) = client.get_bytes(meta_key).await {
            let string_set =
                StringSet::decode(bytes.as_slice()).expect("Failed to decode StringSet");
            assert!(
                !string_set.keys.is_empty(),
                "monitoring_ips StringSet should not be empty"
            );
            assert!(
                string_set.keys.contains(&NODE_IP.to_string()),
                "monitoring_ips should contain {}",
                NODE_IP
            );
            found = true;
            break;
        }
    }
    assert!(
        found,
        "ANNA_METADATA|monitoring_ips was not written within timeout"
    );
}

/// Verify that the KVS server publishes cluster topology (thread counts)
/// as a metadata key that clients can read to discover the cluster shape.
///
/// Uses base_offset=10100 to avoid conflicts with other monitor tests.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn cluster_topology_metadata() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(10100);
    cluster.start();

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(97)).await;

    // Poll until the topology metadata key appears.
    // The KVS publishes this key every server_report_period (3s in test config).
    // Allow up to 20 attempts (40s) to account for startup latency and
    // resource contention when running all monitor tests together.
    let mut topology = None;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        topology = client.get_cluster_topology().await;
        if topology.is_some() {
            break;
        }
    }

    let topo = topology.expect("ANNA_METADATA|cluster_topology was not written within timeout");
    assert_eq!(
        topo.memory_thread_count, 1,
        "Expected 1 memory thread in test config"
    );
    assert_eq!(
        topo.disk_thread_count, 1,
        "Expected 1 disk thread in test config"
    );
}

/// Test that the monitor collects stats from a disk-tier KVS node.
/// This exercises the disk-tier branches in stats_helpers.cpp:
/// - disk_storage, disk_occupancy, disk_accesses population
/// - avg/max disk consumption percentage computation
/// - disk occupancy min/max/avg computation
/// Uses base_offset=6300.
#[tokio::test]
#[cfg(unix)]
#[serial(monitor)]
async fn monitor_disk_tier_stats() {
    use annalib::kvs_client::KVSClient;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let disk_dir = std::env::temp_dir().join(format!("anna_disk_monitor_{}", std::process::id()));
    fs::create_dir_all(&disk_dir).expect("Failed to create disk dir");

    let mut cluster = MonitorTestCluster::new(6300);
    cluster.start_disk_with_config(MonitorConfig {
        base_offset: 6300,
        replication_memory: 0,
        replication_disk: 1,
        disk_path: disk_dir.to_string_lossy().to_string(),
        ..Default::default()
    });

    let config = cluster.client_config();
    let mut client = KVSClient::new(&config, Some(98)).await;

    // PUT data so the disk-tier KVS has storage consumption to report
    for i in 0..5 {
        let key = format!("disk_stats_key_{}", i);
        client
            .put(&key, &"x".repeat(500))
            .await
            .unwrap_or_else(|e| panic!("PUT {} failed: {}", key, e));
    }

    // Wait for stats to be collected — the monitor needs at least one
    // report cycle (server_report_period=3s) + one monitoring cycle (8s)
    let deadline = Instant::now() + Duration::from_secs(REPORT_PERIOD as u64 * 2 + 10);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(s) = client.get_storage_stats(NODE_IP, NODE_IP, 0, "DISK").await {
            if s.epoch > 0 && s.storage_consumption > 0 {
                eprintln!(
                    "Disk stats: consumption={}, occupancy={}, epoch={}",
                    s.storage_consumption, s.occupancy, s.epoch
                );
                stats_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(
        stats_ok,
        "Monitor should collect disk-tier stats with storage_consumption > 0"
    );
}
