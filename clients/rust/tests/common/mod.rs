#![allow(dead_code)]

use std::env;
use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn anna_binary() -> PathBuf {
    let mut path = env::current_exe().expect("Could not get test executable path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("anna")
}

pub fn server_path() -> String {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"));
    root = root.parent().unwrap();
    root = root.parent().unwrap();
    format!(
        "{}:{}",
        env::var("PATH").unwrap(),
        root.join("server/cpp/build/target/kvs").to_string_lossy(),
    )
}

pub fn generate_config(base_offset: u16) -> String {
    let config_dir = std::env::temp_dir().join(format!(
        "anna_system_test_{}_{}",
        std::process::id(),
        base_offset
    ));
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");
    let config_path = config_dir.join("config.yml");
    let ip = "127.0.0.1";
    let content = format!(
        r#"monitoring:
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
  mgmt_ip: {ip}
policy:
  elasticity: false
  selective-rep: false
  tiering: false
ebs: /tmp/anna_ebs_{base_offset}
capacities:
  memory-cap: 1
  ebs-cap: 0
threads:
  memory: 1
  ebs: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  ebs: 0
  minimum: 1
  local: 1
ports:
  base_offset: {base_offset}
timings:
  server_report_period: 15
  key_monitoring_period: 60
  monitoring_timeout: 30
  gossip_epoch: 10
  data_redistribute_batch: 50
  grace_period: 120
  monitoring_response_timeout_ms: 10000
"#,
        ip = ip,
        base_offset = base_offset
    );
    fs::write(&config_path, content).expect("Failed to write config");
    config_path.to_string_lossy().to_string()
}

pub fn client_config(base_offset: u16) -> annalib::client_config::ClientConfig {
    annalib::client_config::ClientConfig {
        routing_addresses: vec![format!("tcp://127.0.0.1:{}", 6450 + base_offset as usize)],
        client_ip: "127.0.0.1".to_string(),
    }
}

pub fn routing_port(base_offset: u16) -> u16 {
    6450 + base_offset
}

pub fn start_servers(path: &str, config: &str) {
    start_servers_with_offset(path, config, 0);
}

pub fn start_servers_with_offset(path: &str, config: &str, base_offset: u16) {
    Command::new(anna_binary())
        .args(["--server-config", config, "start"])
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to start servers");

    wait_for_routing(base_offset);
}

pub fn wait_for_routing(base_offset: u16) {
    let port = routing_port(base_offset);
    let addr = format!("127.0.0.1:{}", port);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(1)).is_ok() {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "Routing tier did not start within 30 seconds (port {})",
                port
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_secs(1));
}

pub fn stop_servers(path: &str, config: &str) {
    Command::new(anna_binary())
        .args(["--server-config", config, "stop"])
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

pub struct ServerGuard {
    pub path: String,
    pub config: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        stop_servers(&self.path, &self.config);
    }
}
