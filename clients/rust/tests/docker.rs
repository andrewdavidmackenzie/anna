//! Integration test: run the Anna cluster inside a Docker container
//! and verify PUT/GET works from a client on the host.
//!
//! This test only runs on Linux (--network host requires Linux).
//! It is ignored by default since it requires Docker to be installed
//! and the image build takes ~20 seconds.

mod common;

use annalib::kvs_client::KVSClient;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Check whether Docker is available and we're on Linux.
fn docker_available() -> bool {
    if cfg!(not(target_os = "linux")) {
        return false;
    }
    Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build the Docker image from the repo root.
fn docker_build() -> bool {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find repo root");

    Command::new("docker")
        .args(["build", "-t", "anna-test", "."])
        .current_dir(repo_root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

struct DockerGuard {
    container_name: String,
}

impl DockerGuard {
    fn start(name: &str) -> Self {
        // Remove any leftover container with the same name
        Command::new("docker")
            .args(["rm", "-f", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok();

        let status = Command::new("docker")
            .args([
                "run",
                "--rm",
                "-d",
                "--network",
                "host",
                "--name",
                name,
                "anna-test",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .expect("failed to start Docker container");

        assert!(status.success(), "docker run failed");

        DockerGuard {
            container_name: name.to_string(),
        }
    }
}

impl Drop for DockerGuard {
    fn drop(&mut self) {
        Command::new("docker")
            .args(["stop", &self.container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok();
    }
}

#[tokio::test]
#[cfg(target_os = "linux")]
#[ignore] // requires Docker; run with: cargo test --test docker -- --ignored
async fn docker_put_get() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    eprintln!("Building Docker image...");
    if !docker_build() {
        panic!("Docker build failed");
    }

    // Verify port 6450 is free before starting -- with --network host,
    // a pre-existing Anna process would satisfy the readiness check.
    assert!(
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:6450".parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_err(),
        "Port 6450 is already in use -- kill any existing Anna processes first"
    );

    eprintln!("Starting container...");
    let _guard = DockerGuard::start("anna-docker-test");

    // Wait for routing tier to accept connections
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if std::net::TcpStream::connect_timeout(
            &"127.0.0.1:6450".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok()
        {
            break;
        }
        if Instant::now() > deadline {
            panic!("Docker cluster did not start within 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // Let the KVS join the hash ring
    std::thread::sleep(Duration::from_secs(2));

    let config = common::client_config(0);
    let mut client = KVSClient::new(&config, Some(1)).await;

    // PUT two independent keys
    client
        .put("docker_key_1", "value_one")
        .await
        .expect("PUT docker_key_1 failed");
    client
        .put("docker_key_2", "value_two")
        .await
        .expect("PUT docker_key_2 failed");

    // GET: verify each key returns the correct value (not swapped, not empty)
    let val1 = client
        .get("docker_key_1")
        .await
        .expect("GET docker_key_1 failed");
    assert_eq!(val1, "value_one", "docker_key_1 returned wrong value");

    let val2 = client
        .get("docker_key_2")
        .await
        .expect("GET docker_key_2 failed");
    assert_eq!(val2, "value_two", "docker_key_2 returned wrong value");

    // Overwrite and verify the new value is returned
    client
        .put("docker_key_1", "updated_value")
        .await
        .expect("PUT overwrite failed");
    let val1_updated = client
        .get("docker_key_1")
        .await
        .expect("GET after overwrite failed");
    assert_eq!(
        val1_updated, "updated_value",
        "overwrite did not take effect"
    );

    // DELETE and verify the value is empty (Anna delete = PUT empty LWW)
    client.delete("docker_key_1").await.expect("DELETE failed");

    // Use a fresh client to bypass the local read cache and confirm the
    // delete reached the server. The result is either an empty string
    // (the delete's empty LWW was received) or KEY_DNE (the delete
    // hasn't propagated via gossip yet). Both confirm the original
    // value is gone.
    let mut client2 = KVSClient::new(&config, Some(2)).await;
    match client2.get("docker_key_1").await {
        Ok(val) => assert_eq!(val, "", "deleted key should return empty value"),
        Err(e) => assert!(
            e.to_string().contains("KEY_DNE"),
            "expected KEY_DNE after delete but got: {}",
            e
        ),
    }

    // Verify the other key is unaffected by the delete
    let val2_still = client2
        .get("docker_key_2")
        .await
        .expect("GET docker_key_2 after delete failed");
    assert_eq!(
        val2_still, "value_two",
        "docker_key_2 was affected by deleting docker_key_1"
    );

    eprintln!("Docker integration test passed: PUT, GET, overwrite, DELETE all verified");
}
