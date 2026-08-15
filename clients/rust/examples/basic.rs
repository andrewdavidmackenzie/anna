//! Basic example of using the anna Rust client library.
//!
//! This example starts the anna server processes (monitor, route, kvs),
//! connects a client, performs basic key-value operations (put, get, delete),
//! and then shuts the server down.
//!
//! # Prerequisites
//!
//! The anna server binaries (`anna-monitor`, `anna-route`, `anna-kvs`) must
//! be in your PATH. Build them first with `make server-cpp` or `make server-rust`.
//!
//! # Running
//!
//! ```sh
//! cargo run --example basic
//! ```

use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

use annalib::client_config::ClientConfig;
use annalib::kvs_client::KVSClient;

/// Generate a minimal anna config file in a temporary directory.
fn generate_config() -> String {
    let config_dir = std::env::temp_dir().join(format!("anna_example_{}", std::process::id()));
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");
    let config_path = config_dir.join("config.yml");
    let ip = "127.0.0.1";

    let content = format!(
        r#"monitoring:
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
  scaling_alert_ip: "NULL"
policy:
  elasticity: false
  selective-rep: false
  tiering: false
disk: {disk}
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
ports:
  base_offset: 0
"#,
        ip = ip,
        disk = config_dir.join("disk").to_string_lossy(),
    );
    fs::write(&config_path, content).expect("Failed to write config");
    config_path.to_string_lossy().to_string()
}

/// Wait for the routing tier to accept TCP connections (up to 30 seconds).
fn wait_for_routing() {
    let addr = "127.0.0.1:6450";
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(1)).is_ok() {
            // Give the cluster a moment to stabilize
            std::thread::sleep(Duration::from_secs(1));
            return;
        }
        if Instant::now() > deadline {
            panic!("Routing tier did not start within 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[tokio::main]
async fn main() {
    let config_path = generate_config();

    // Start the anna server processes
    println!("Starting anna server...");
    let count = annalib::start(Path::new(&config_path), &[])
        .expect("Failed to start anna server (are the binaries in PATH?)");
    println!("  Started {} processes", count);

    // Ensure the server is stopped on exit (including panics)
    struct StopGuard;
    impl Drop for StopGuard {
        fn drop(&mut self) {
            println!("\nStopping anna server...");
            match annalib::stop(&[]) {
                Ok(n) => println!("  Stopped {} processes", n),
                Err(e) => eprintln!("  Failed to stop: {}", e),
            }
        }
    }
    let _guard = StopGuard;

    wait_for_routing();

    // Connect a client
    let config = ClientConfig::default();
    let mut client = KVSClient::new(&config, Some(50)).await;

    // PUT a value
    println!("\nPUT greeting = hello");
    client.put("greeting", "hello").await.expect("PUT failed");

    // GET it back
    let val = client.get("greeting").await.expect("GET failed");
    println!("GET greeting = {}", val);

    // Overwrite the value
    println!("\nPUT greeting = hello world");
    client
        .put("greeting", "hello world")
        .await
        .expect("PUT overwrite failed");

    let val = client.get("greeting").await.expect("GET overwrite failed");
    println!("GET greeting = {}", val);

    // PUT a second key
    println!("\nPUT count = 42");
    client.put("count", "42").await.expect("PUT count failed");

    // DELETE the first key
    println!("\nDELETE greeting");
    client.delete("greeting").await.expect("DELETE failed");

    // Verify deletion: GET should return a KEY_DNE error
    match client.get("greeting").await {
        Err(e) if e.to_string().contains("KEY_DNE") => println!("GET greeting = (deleted)"),
        Ok(val) => println!("GET greeting = {} (unexpected)", val),
        Err(e) => println!("GET greeting error: {}", e),
    }

    // GET the remaining key
    let val = client.get("count").await.expect("GET count failed");
    println!("GET count = {}", val);

    println!("\nDone!");
    // StopGuard::drop() will stop the server
}
