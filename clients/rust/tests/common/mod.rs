#![allow(dead_code)]

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs};

fn server_bin_dir() -> PathBuf {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    root.pop();
    root.pop();
    root.join("server/cpp/build/target/kvs")
}

pub fn server_path() -> String {
    format!(
        "{}:{}",
        env::var("PATH").unwrap(),
        server_bin_dir().to_string_lossy(),
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
  mgmt_ip: "NULL"
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

pub struct ServerGuard {
    processes: Vec<Child>,
}

impl ServerGuard {
    pub fn start(config_path: &str, base_offset: u16) -> Self {
        let bin_dir = server_bin_dir();
        let extra_path = server_path();
        let mut processes: Vec<Child> = Vec::new();

        for name in ["anna-monitor", "anna-route", "anna-kvs"] {
            let bin = bin_dir.join(name);
            if !bin.exists() {
                for mut p in processes {
                    p.kill().ok();
                }
                panic!("Server binary {} not found at {:?}", name, bin);
            }
            let child = Command::new(&bin)
                .args(["--config", config_path])
                .env("PATH", &extra_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap_or_else(|e| panic!("Failed to spawn {}: {}", name, e));
            processes.push(child);
            std::thread::sleep(Duration::from_secs(1));
        }

        wait_for_routing(base_offset);

        ServerGuard { processes }
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            for child in &mut self.processes {
                kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        for child in &mut self.processes {
            child.kill().ok();
            child.wait().ok();
        }
    }
}
