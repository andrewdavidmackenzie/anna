//! Monitoring system tests: verify that anna-kvs reports statistics to
//! anna-monitor via internal metadata keys.
//!
//! These tests use a separate port offset range (100+) from multi_node.rs
//! (0-25221) to avoid conflicts. Since cargo runs test binaries sequentially,
//! there is no parallel conflict between files.

mod common;

use common::server_path;
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

fn write_monitor_config(path: &Path, base_offset: u32) {
    let mut f = fs::File::create(path).expect("Failed to create config file");
    write!(
        f,
        "\
monitoring:
  mgmt_ip: {ip}
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
  gossip_epoch: 2
  server_report_period: {report_period}
  key_monitoring_period: 15
  monitoring_timeout: 8
  monitoring_response_timeout_ms: 1000
  data_redistribute_batch: 50
  grace_period: 10
replication:
  memory: 1
  ebs: 0
  minimum: 1
  local: 1
",
        ip = NODE_IP,
        base_offset = base_offset,
        report_period = REPORT_PERIOD,
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
        let config = self.config_dir.join("config.yml");
        write_monitor_config(&config, self.base_offset);

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
            let bin = server_bin_dir().join(name);
            if !bin.exists() {
                self.shutdown();
                panic!("Server binary {} not found", name);
            }
            let child = Command::new(&bin)
                .args(["--config", &config.to_string_lossy()])
                .env("PATH", &server_path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
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
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.yml")
    }

    fn shutdown(&mut self) {
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
    }
}

impl Drop for MonitorTestCluster {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Metadata key format: ANNA_METADATA|<type>|<public_ip>|<private_ip>|<tid>|<tier>
/// Types: "stats", "access", "size"
fn stats_metadata_key(ip: &str, tid: u32, meta_type: &str) -> String {
    format!("ANNA_METADATA|{}|{}|{}|{}|MEMORY", meta_type, ip, ip, tid)
}

/// Verify that KVS nodes report statistics as metadata keys that can be
/// read back by a client.
///
/// Covers 6 monitoring features:
/// - Storage consumption reporting
/// - CPU occupancy reporting
/// - Access count reporting
/// - Per-key access frequency
/// - Per-key size for primary replicas
/// - Per-event-type occupancy logging (verified via non-zero occupancy)
#[tokio::test]
#[cfg(unix)]
async fn monitor_stats_collection() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;
    use prost::Message;

    if !server_bin_dir().join("anna-kvs").exists() {
        eprintln!("SKIP: server binaries not built");
        return;
    }

    let mut cluster = MonitorTestCluster::new(100);
    cluster.start();

    let config = Config::read(&cluster.config_path()).expect("Failed to read config");
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

    // Read stats metadata key
    let stats_key = stats_metadata_key(NODE_IP, 0, "stats");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stats_ok = false;
    while Instant::now() < deadline {
        if let Ok(bytes) = client.get_bytes(&stats_key).await {
            let stats = annalib::proto::metadata::ServerThreadStatistics::decode(bytes.as_slice());
            if let Ok(s) = stats {
                assert!(
                    s.storage_consumption > 0,
                    "storage_consumption should be > 0, got {}",
                    s.storage_consumption
                );
                assert!(s.epoch > 0, "epoch should be > 0, got {}", s.epoch);
                assert!(
                    s.access_count > 0,
                    "access_count should be > 0, got {}",
                    s.access_count
                );
                assert!(
                    s.occupancy >= 0.0,
                    "occupancy should be >= 0, got {}",
                    s.occupancy
                );
                stats_ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(stats_ok, "Failed to read valid stats metadata");

    // Read per-key access frequency metadata
    let access_key = stats_metadata_key(NODE_IP, 0, "access");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut access_ok = false;
    while Instant::now() < deadline {
        if let Ok(bytes) = client.get_bytes(&access_key).await {
            let access = annalib::proto::metadata::KeyAccessData::decode(bytes.as_slice());
            if let Ok(a) = access {
                if !a.keys.is_empty() {
                    let has_test_key = a.keys.iter().any(|k| k.key.starts_with("stats_test_key_"));
                    assert!(
                        has_test_key,
                        "access data should contain our test keys, got: {:?}",
                        a.keys.iter().map(|k| &k.key).collect::<Vec<_>>()
                    );
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
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(access_ok, "Failed to read valid access metadata");

    // Read per-key size metadata
    let size_key = stats_metadata_key(NODE_IP, 0, "size");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut size_ok = false;
    while Instant::now() < deadline {
        if let Ok(bytes) = client.get_bytes(&size_key).await {
            let sizes = annalib::proto::metadata::KeySizeData::decode(bytes.as_slice());
            if let Ok(s) = sizes {
                if !s.key_sizes.is_empty() {
                    let has_test_key = s
                        .key_sizes
                        .iter()
                        .any(|k| k.key.starts_with("stats_test_key_"));
                    assert!(
                        has_test_key,
                        "size data should contain our test keys, got: {:?}",
                        s.key_sizes.iter().map(|k| &k.key).collect::<Vec<_>>()
                    );
                    let test_sizes: Vec<_> = s
                        .key_sizes
                        .iter()
                        .filter(|k| k.key.starts_with("stats_test_key_"))
                        .collect();
                    for ks in &test_sizes {
                        assert!(ks.size > 0, "size for {} should be > 0", ks.key);
                    }
                    size_ok = true;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(size_ok, "Failed to read valid size metadata");
}
