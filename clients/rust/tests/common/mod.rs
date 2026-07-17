#![allow(dead_code)]

use std::env;
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

pub fn config_file() -> String {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"));
    root = root.parent().unwrap();
    root = root.parent().unwrap();
    root.join("conf/anna-config.yml")
        .to_string_lossy()
        .to_string()
}

pub fn routing_port(base_offset: u16) -> u16 {
    6450 + base_offset
}

pub fn start_servers(path: &str, config: &str) {
    start_servers_with_offset(path, config, 0);
}

pub fn start_servers_with_offset(path: &str, config: &str, base_offset: u16) {
    Command::new(anna_binary())
        .args(["--config", config, "start"])
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
        .args(["--config", config, "stop"])
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
