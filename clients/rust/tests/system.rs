//! System test: drive KVSClient library API directly against a live server.

use std::env;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn anna_binary() -> std::path::PathBuf {
    let mut path = env::current_exe().expect("Could not get test executable path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("anna")
}

fn server_path() -> String {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"));
    root = root.parent().unwrap();
    root = root.parent().unwrap();
    format!(
        "{}:{}",
        env::var("PATH").unwrap(),
        root.join("server/cpp/build/target/kvs").to_string_lossy(),
    )
}

fn config_file() -> String {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"));
    root = root.parent().unwrap();
    root = root.parent().unwrap();
    root.join("conf/anna-config.yml")
        .to_string_lossy()
        .to_string()
}

fn start_servers(path: &str, config: &str) {
    Command::new(anna_binary())
        .args(["--config", config, "start"])
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to start servers");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if TcpStream::connect_timeout(
            &"127.0.0.1:6450".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok()
        {
            break;
        }
        if Instant::now() > deadline {
            panic!("Routing tier did not start within 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_secs(1));
}

fn stop_servers(path: &str, config: &str) {
    Command::new(anna_binary())
        .args(["--config", config, "stop"])
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

struct ServerGuard {
    path: String,
    config: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        stop_servers(&self.path, &self.config);
    }
}

#[test]
#[cfg(unix)]
fn system_test_kvs_client() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    let path = server_path();
    let config_path = config_file();

    start_servers(&path, &config_path);
    let _guard = ServerGuard {
        path: path.clone(),
        config: config_path.clone(),
    };

    let config =
        Config::read(&std::path::PathBuf::from(&config_path)).expect("Failed to read config");
    let mut client = KVSClient::new(&config, Some(50));

    // PUT and GET a LWW value
    client.put("sys_test_a", "hello").expect("PUT failed");
    let val = client.get("sys_test_a").expect("GET failed");
    assert_eq!(val, "hello", "GET returned wrong value");

    // Overwrite
    client.put("sys_test_a", "world").expect("PUT overwrite failed");
    let val = client.get("sys_test_a").expect("GET after overwrite failed");
    assert_eq!(val, "world", "GET after overwrite returned wrong value");

    // Multiple keys
    client.put("sys_test_b", "42").expect("PUT b failed");
    let val_a = client.get("sys_test_a").expect("GET a failed");
    let val_b = client.get("sys_test_b").expect("GET b failed");
    assert_eq!(val_a, "world");
    assert_eq!(val_b, "42");

    // PUT_SET and GET_SET
    client
        .put_set("sys_test_set", &["x", "y", "z"])
        .expect("PUT_SET failed");
    let set_val = client.get_set("sys_test_set").expect("GET_SET failed");
    assert!(set_val.contains(&"x".to_string()));
    assert!(set_val.contains(&"y".to_string()));
    assert!(set_val.contains(&"z".to_string()));
    assert_eq!(set_val.len(), 3);

    // SET union
    client
        .put_set("sys_test_set", &["w", "x"])
        .expect("PUT_SET union failed");
    let set_val = client.get_set("sys_test_set").expect("GET_SET after union failed");
    assert!(set_val.len() >= 3, "Expected at least 3 elements, got {}", set_val.len());
    assert!(set_val.contains(&"x".to_string()));
    assert!(set_val.contains(&"w".to_string()));
}
