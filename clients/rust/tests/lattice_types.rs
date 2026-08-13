//! System test: drive KVSClient library API directly against a live server.
//! Tests run against both memory-tier and disk-tier KVS nodes to exercise
//! both Memory*Serializer and Disk*Serializer code paths.

mod common;

use annalib::kvs_client::KVSClient;
use common::{client_config, generate_config, generate_disk_config, ServerGuard};

const MEMORY_BASE_OFFSET: u16 = 200;
const DISK_BASE_OFFSET: u16 = 201;

/// Core lattice type tests: PUT, GET, overwrite, merge, DELETE, KEY_DNE.
/// Shared between memory and disk tier tests.
async fn test_all_lattice_types(client: &mut KVSClient, prefix: &str) {
    // LWW: put, get, overwrite
    let key_a = format!("{prefix}_a");
    client.put(&key_a, "hello").await.expect("PUT failed");
    let val = client.get(&key_a).await.expect("GET failed");
    assert_eq!(val, "hello");

    client
        .put(&key_a, "world")
        .await
        .expect("PUT overwrite failed");
    let val = client.get(&key_a).await.expect("GET overwrite failed");
    assert_eq!(val, "world");

    // Multiple keys
    let key_b = format!("{prefix}_b");
    client.put(&key_b, "42").await.expect("PUT b failed");
    assert_eq!(client.get(&key_a).await.unwrap(), "world");
    assert_eq!(client.get(&key_b).await.unwrap(), "42");

    // SET and ORDERED_SET
    #[cfg(feature = "set")]
    {
        let set_key = format!("{prefix}_set");
        client
            .put_set(&set_key, &["x", "y", "z"])
            .await
            .expect("PUT_SET failed");
        let set_val = client.get_set(&set_key).await.expect("GET_SET failed");
        assert_eq!(set_val.len(), 3);

        // SET union merge (second PUT to same key)
        client
            .put_set(&set_key, &["w", "x"])
            .await
            .expect("PUT_SET union failed");
        let set_val = client
            .get_set(&set_key)
            .await
            .expect("GET_SET union failed");
        assert!(set_val.len() >= 3);
        assert!(set_val.contains(&"w".to_string()));

        // ORDERED_SET with merge
        let oset_key = format!("{prefix}_oset");
        client
            .put_ordered_set(&oset_key, &["alpha", "beta"])
            .await
            .expect("PUT_ORDERED_SET 1 failed");
        client
            .put_ordered_set(&oset_key, &["beta", "gamma"])
            .await
            .expect("PUT_ORDERED_SET 2 failed");
        let oset_val = client
            .get_ordered_set(&oset_key)
            .await
            .expect("GET_ORDERED_SET failed");
        assert!(oset_val.len() >= 3, "OrderedSet union should merge");
    }

    // SINGLE_CAUSAL with merge
    #[cfg(feature = "causal")]
    {
        let sc_key = format!("{prefix}_sc");
        client
            .put_single_causal(&sc_key, "sc_v1")
            .await
            .expect("PUT_SINGLE_CAUSAL 1 failed");
        client
            .put_single_causal(&sc_key, "sc_v2")
            .await
            .expect("PUT_SINGLE_CAUSAL 2 failed");
        let (vc, values) = client
            .get_single_causal(&sc_key)
            .await
            .expect("GET_SINGLE_CAUSAL failed");
        assert!(!values.is_empty(), "SingleCausal should have values");
        assert!(!vc.is_empty(), "SingleCausal should have vector clock");
    }

    // MULTI_CAUSAL with merge
    #[cfg(feature = "causal")]
    {
        let mc_key = format!("{prefix}_mc");
        client
            .put_causal(&mc_key, "mc_v1")
            .await
            .expect("PUT_CAUSAL 1 failed");
        client
            .put_causal(&mc_key, "mc_v2")
            .await
            .expect("PUT_CAUSAL 2 failed");
        let (vc, _deps, value) = client.get_causal(&mc_key).await.expect("GET_CAUSAL failed");
        assert!(!value.is_empty(), "MultiCausal should have a value");
        assert!(!vc.is_empty(), "MultiCausal should have vector clock");
    }

    // PRIORITY with merge (lowest wins)
    let pri_key = format!("{prefix}_pri");
    client
        .put_priority(&pri_key, 5.0, "high")
        .await
        .expect("PUT_PRIORITY 1 failed");
    client
        .put_priority(&pri_key, 1.0, "low")
        .await
        .expect("PUT_PRIORITY 2 failed");
    let (priority, pri_value) = client
        .get_priority(&pri_key)
        .await
        .expect("GET_PRIORITY failed");
    assert!(
        (priority - 1.0).abs() < f64::EPSILON,
        "Priority merge: lowest should win, got {}",
        priority
    );
    assert_eq!(pri_value, "low");

    // DELETE
    let del_key = format!("{prefix}_del");
    client
        .put(&del_key, "to_delete")
        .await
        .expect("PUT for delete failed");
    assert_eq!(client.get(&del_key).await.unwrap(), "to_delete");
    client.delete(&del_key).await.expect("DELETE failed");
    let after_del = client.get(&del_key).await;
    assert!(after_del.is_err(), "GET after DELETE should fail");
    assert!(
        after_del.unwrap_err().to_string().contains("KEY_DNE"),
        "Error should indicate KEY_DNE"
    );

    // KEY_DNE
    let dne_key = format!("{prefix}_nonexistent_xyz");
    let dne = client.get(&dne_key).await;
    assert!(dne.is_err(), "GET nonexistent key should fail");
    assert!(dne.unwrap_err().to_string().contains("KEY_DNE"));
}

/// Memory-tier-only tests (multi-key GET, lattice mismatch, metadata keys).
async fn test_memory_extras(client: &mut KVSClient) {
    // MULTI-KEY GET
    client.put("multi_a", "val_a").await.unwrap();
    client.put("multi_b", "val_b").await.unwrap();
    client.put("multi_c", "val_c").await.unwrap();
    let results = client
        .get_multi(&["multi_a", "multi_b", "multi_c"])
        .await
        .expect("GET_MULTI failed");
    assert_eq!(results.len(), 3);
    assert_eq!(results["multi_a"], "val_a");
    assert_eq!(results["multi_b"], "val_b");
    assert_eq!(results["multi_c"], "val_c");

    // MULTI-KEY GET with empty list
    let empty = client
        .get_multi::<String>(&[])
        .await
        .expect("GET_MULTI empty failed");
    assert!(empty.is_empty());

    // LATTICE mismatch: PUT as LWW then PUT_SET to the same key
    client.put("lattice_clash", "lww_value").await.unwrap();
    #[cfg(feature = "set")]
    {
        client.put_set("lattice_clash", &["set_val"]).await.ok();
        let original = client.get("lattice_clash").await.unwrap();
        assert_eq!(original, "lww_value", "Original LWW should be preserved");
    }

    // METADATA KEY
    let meta_key = "ANNA_METADATA|replication|meta_test_key";
    client.put(meta_key, "meta_value").await.unwrap();
    let meta_val = client.get(meta_key).await.unwrap();
    assert_eq!(meta_val, "meta_value");
}

#[tokio::test]
#[cfg(unix)]
async fn system_test_kvs_client() {
    let config_path = generate_config(MEMORY_BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, MEMORY_BASE_OFFSET);
    let config = client_config(MEMORY_BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(5)).await;

    test_all_lattice_types(&mut client, "mem").await;
    test_memory_extras(&mut client).await;
}

/// Same lattice type tests running against a disk-tier KVS.
/// Exercises all Disk*Serializer classes including merge paths
/// (read-merge-write cycle when a key already exists on disk).
#[tokio::test]
#[cfg(unix)]
async fn disk_tier_lattice_types() {
    let config_path = generate_disk_config(DISK_BASE_OFFSET);
    let _guard = ServerGuard::start_disk(&config_path, DISK_BASE_OFFSET);
    let config = client_config(DISK_BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(6)).await;

    test_all_lattice_types(&mut client, "disk").await;
}

const OR_SET_OFFSET: u16 = 206;

/// Test OR-Set: add, remove, get, add-wins-over-concurrent-remove.
#[tokio::test]
#[cfg(unix)]
async fn or_set_add_remove_get() {
    let config_path = generate_config(OR_SET_OFFSET);
    let _guard = ServerGuard::start(&config_path, OR_SET_OFFSET);
    let config = client_config(OR_SET_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    // Add elements
    client.or_set_add("oset", "apple").await.expect("add apple");
    client
        .or_set_add("oset", "banana")
        .await
        .expect("add banana");
    client
        .or_set_add("oset", "cherry")
        .await
        .expect("add cherry");

    // Get should return all 3
    let vals = client.get_or_set("oset").await.expect("get_or_set");
    assert_eq!(vals, vec!["apple", "banana", "cherry"]);

    // Remove banana
    client
        .or_set_remove("oset", "banana")
        .await
        .expect("remove banana");

    // Get should return apple and cherry
    let vals = client
        .get_or_set("oset")
        .await
        .expect("get_or_set after remove");
    assert_eq!(vals, vec!["apple", "cherry"]);

    // Re-add banana (add wins over previous remove)
    client
        .or_set_add("oset", "banana")
        .await
        .expect("re-add banana");
    let vals = client
        .get_or_set("oset")
        .await
        .expect("get_or_set after re-add");
    assert_eq!(vals, vec!["apple", "banana", "cherry"]);
}

const BATCH_PUT_OFFSET: u16 = 207;

/// Test put_multi: batch PUT multiple keys, verify all readable.
#[tokio::test]
#[cfg(unix)]
async fn batch_put_and_get() {
    let config_path = generate_config(BATCH_PUT_OFFSET);
    let _guard = ServerGuard::start(&config_path, BATCH_PUT_OFFSET);
    let config = client_config(BATCH_PUT_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    let pairs: Vec<(&str, &str)> = (0..10)
        .map(|i| {
            // Leak strings to get &str with 'static lifetime for the vec
            let k: &str = Box::leak(format!("batch_key_{}", i).into_boxed_str());
            let v: &str = Box::leak(format!("batch_val_{}", i).into_boxed_str());
            (k, v)
        })
        .collect();

    client.put_multi(&pairs).await.expect("put_multi failed");

    // Verify all keys are readable.
    for i in 0..10 {
        let val = client
            .get(&format!("batch_key_{}", i))
            .await
            .unwrap_or_else(|e| panic!("GET batch_key_{} failed: {}", i, e));
        assert_eq!(val, format!("batch_val_{}", i));
    }
}

const COUNTER_BASIC_OFFSET: u16 = 208;
const COUNTER_DNE_OFFSET: u16 = 209;

/// Test PN-Counter: increment, decrement, get_counter.
#[tokio::test]
#[cfg(unix)]
async fn counter_basic() {
    let config_path = generate_config(COUNTER_BASIC_OFFSET);
    let _guard = ServerGuard::start(&config_path, COUNTER_BASIC_OFFSET);
    let config = client_config(COUNTER_BASIC_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    // Increment 3 times
    client
        .increment("counter_key")
        .await
        .expect("increment 1 failed");
    client
        .increment("counter_key")
        .await
        .expect("increment 2 failed");
    client
        .increment("counter_key")
        .await
        .expect("increment 3 failed");

    // Get counter value
    let val = client
        .get_counter("counter_key")
        .await
        .expect("get_counter failed");
    assert_eq!(val, 3, "counter should be 3 after 3 increments");

    // Increment by 10
    client
        .increment_by("counter_key", 10)
        .await
        .expect("increment_by failed");
    let val = client
        .get_counter("counter_key")
        .await
        .expect("get_counter failed");
    assert_eq!(val, 13, "counter should be 13");

    // Decrement by 5
    client
        .decrement_by("counter_key", 5)
        .await
        .expect("decrement_by failed");
    let val = client
        .get_counter("counter_key")
        .await
        .expect("get_counter failed");
    assert_eq!(val, 8, "counter should be 8 after decrementing 5 from 13");

    // Decrement by 1
    client
        .decrement("counter_key")
        .await
        .expect("decrement failed");
    let val = client
        .get_counter("counter_key")
        .await
        .expect("get_counter failed");
    assert_eq!(val, 7, "counter should be 7");
}

/// Test that a counter that doesn't exist returns KEY_DNE.
#[tokio::test]
#[cfg(unix)]
async fn counter_nonexistent_key() {
    let config_path = generate_config(COUNTER_DNE_OFFSET);
    let _guard = ServerGuard::start(&config_path, COUNTER_DNE_OFFSET);
    let config = client_config(COUNTER_DNE_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    let result = client.get_counter("nonexistent_counter").await;
    assert!(
        result.is_err(),
        "get_counter on nonexistent key should fail"
    );
}

const TTL_EXPIRE_OFFSET: u16 = 210;
const TTL_PERSIST_OFFSET: u16 = 220;
const TTL_STRESS_OFFSET: u16 = 230;

/// Generate a config with aggressive TTL GC settings for testing.
/// Uses gossip_epoch=1s and tombstone_gc_multiplier=1 so TTL-expired
/// keys are reaped within ~2 seconds.
fn generate_ttl_config(base_offset: u16) -> String {
    let config_dir = std::env::temp_dir().join(format!(
        "anna_ttl_test_{}_{}",
        std::process::id(),
        base_offset
    ));
    std::fs::create_dir_all(&config_dir).expect("create dir");
    let config_path = config_dir.join("config.yml");
    let disk_dir = config_dir.join("disk");
    std::fs::create_dir_all(&disk_dir).expect("create disk dir");
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
disk: {disk_path}
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
ports:
  base_offset: {base_offset}
timings:
  gossip_epoch: 1
  tombstone_gc_multiplier: 1
  server_report_period: 15
  key_monitoring_period: 60
  monitoring_timeout: 30
  data_redistribute_batch: 50
  grace_period: 120
  monitoring_response_timeout_ms: 10000
"#,
        ip = ip,
        base_offset = base_offset,
        disk_path = disk_dir.to_string_lossy(),
    );
    std::fs::write(&config_path, content).expect("write config");
    config_path.to_string_lossy().to_string()
}

/// Test TTL: PUT with a short TTL, verify GET works before expiry,
/// then verify GET returns KEY_DNE after expiry.
#[tokio::test]
#[cfg(unix)]
async fn ttl_key_expires() {
    let config_path = generate_ttl_config(TTL_EXPIRE_OFFSET);
    let _guard = ServerGuard::start(&config_path, TTL_EXPIRE_OFFSET);
    let config = client_config(TTL_EXPIRE_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    // PUT with 2-second TTL
    client
        .put_with_ttl("ttl_key", "ttl_value", 2)
        .await
        .expect("PUT_TTL failed");

    // GET immediately — should succeed
    let val = client
        .get("ttl_key")
        .await
        .expect("GET before expiry failed");
    assert_eq!(
        val, "ttl_value",
        "value should be readable before TTL expires"
    );

    // Wait for TTL to expire + GC cycle (gossip_epoch=1s, gc_multiplier=1)
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    // GET after expiry — should fail with KEY_DNE.
    // Use a fresh client to avoid the read cache.
    let mut client2 = KVSClient::new(&config, Some(8)).await;
    let result = client2.get("ttl_key").await;
    assert!(
        result.is_err(),
        "GET after TTL expiry should return error but got: {:?}",
        result
    );
}

/// Test that a key without TTL does not expire.
#[tokio::test]
#[cfg(unix)]
async fn no_ttl_key_persists() {
    let config_path = generate_ttl_config(TTL_PERSIST_OFFSET);
    let _guard = ServerGuard::start(&config_path, TTL_PERSIST_OFFSET);
    let config = client_config(TTL_PERSIST_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    // PUT without TTL
    client
        .put("persist_key", "persist_value")
        .await
        .expect("PUT failed");

    // Wait longer than the TTL test
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    // GET — should still succeed
    let val = client
        .get("persist_key")
        .await
        .expect("GET should succeed for non-TTL key");
    assert_eq!(val, "persist_value");
}

/// Stress test: PUT many keys with short TTLs, verify they all expire.
#[tokio::test]
#[cfg(unix)]
async fn ttl_stress_many_keys() {
    let config_path = generate_ttl_config(TTL_STRESS_OFFSET);
    let _guard = ServerGuard::start(&config_path, TTL_STRESS_OFFSET);
    let config = client_config(TTL_STRESS_OFFSET);
    let mut client = KVSClient::new(&config, Some(7)).await;

    let key_count = 50;

    // PUT many keys with 10-second TTL (must be long enough for all PUTs
    // and GETs to complete before expiry, even on loaded CI runners).
    for i in 0..key_count {
        client
            .put_with_ttl(&format!("stress_{}", i), &format!("val_{}", i), 10)
            .await
            .unwrap_or_else(|e| panic!("PUT_TTL stress_{} failed: {}", i, e));
    }

    // Verify all readable immediately
    for i in 0..key_count {
        let val = client
            .get(&format!("stress_{}", i))
            .await
            .unwrap_or_else(|e| panic!("GET stress_{} failed: {}", i, e));
        assert_eq!(val, format!("val_{}", i));
    }

    // Wait for expiry
    tokio::time::sleep(std::time::Duration::from_secs(12)).await;

    // Fresh client — verify all expired
    let mut client2 = KVSClient::new(&config, Some(8)).await;
    let mut expired_count = 0;
    for i in 0..key_count {
        if client2.get(&format!("stress_{}", i)).await.is_err() {
            expired_count += 1;
        }
    }
    assert_eq!(
        expired_count, key_count,
        "all keys should have expired, but only {}/{} did",
        expired_count, key_count
    );
}

/// SCAN: list keys matching a prefix across all KVS threads.
#[tokio::test]
#[cfg(unix)]
async fn scan_keys() {
    let config_path = generate_config(221);
    let _guard = ServerGuard::start(&config_path, 221);
    let config = client_config(221);
    let mut client = KVSClient::new(&config, Some(15)).await;

    // Insert some keys with different prefixes.
    client.put("scan_a1", "v1").await.expect("PUT failed");
    client.put("scan_a2", "v2").await.expect("PUT failed");
    client.put("scan_b1", "v3").await.expect("PUT failed");
    client.put("other_x", "v4").await.expect("PUT failed");

    // Scan all keys.
    let all = client.scan("").await.expect("SCAN all failed");
    assert!(
        all.len() >= 4,
        "expected at least 4 keys, got {}",
        all.len()
    );

    // Scan with prefix filter.
    let scan_a = client.scan("scan_a").await.expect("SCAN prefix failed");
    assert_eq!(scan_a.len(), 2, "expected 2 keys with prefix 'scan_a'");
    for entry in &scan_a {
        assert!(
            entry.key.starts_with("scan_a"),
            "key '{}' should start with 'scan_a'",
            entry.key
        );
    }

    // Scan with non-existent prefix.
    let none = client
        .scan("nonexistent_prefix_")
        .await
        .expect("SCAN empty failed");
    assert!(none.is_empty(), "expected 0 keys for non-existent prefix");
}

/// Client-side routing: verify get_kvs_members returns node IPs.
#[tokio::test]
#[cfg(unix)]
async fn kvs_members_metadata() {
    let config_path = generate_config(1222);
    let _guard = ServerGuard::start(&config_path, 1222);
    let config = client_config(1222);
    let mut client = KVSClient::new(&config, Some(16)).await;

    // PUT a key so the server has been active.
    client.put("member_test", "val").await.expect("PUT failed");

    // Wait for the KVS to publish its member list (happens during
    // stats reporting, period=15s in test config). Poll with retries.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut members = vec![];
    while std::time::Instant::now() < deadline {
        members = client.get_kvs_members().await;
        if !members.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    assert!(
        !members.is_empty(),
        "KVS should publish member list within 20s"
    );
    // Single-node cluster: expect exactly one member.
    assert_eq!(members.len(), 1, "expected 1 KVS member");
    assert!(
        members[0].contains("127.0.0.1"),
        "member should contain localhost IP"
    );
}

/// Client-side routing: PUT/GET using direct hash ring routing.
#[tokio::test]
#[cfg(unix)]
#[cfg(feature = "direct-routing")]
async fn direct_routing_put_get() {
    let config_path = generate_config(1223);
    let _guard = ServerGuard::start(&config_path, 1223);
    let config = client_config(1223);
    let mut client = KVSClient::new(&config, Some(17)).await;

    // PUT a key via routing (normal path) so server publishes membership.
    client
        .put("direct_test_setup", "setup")
        .await
        .expect("setup PUT failed");

    // Wait for membership to be published.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if !client.get_kvs_members().await.is_empty() {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("KVS members not published within 20s");
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // Enable direct routing.
    client
        .enable_direct_routing()
        .await
        .expect("enable_direct_routing failed");

    // PUT and GET via direct routing (no routing tier involved).
    client
        .put("direct_key1", "value1")
        .await
        .expect("direct PUT failed");
    let val = client.get("direct_key1").await.expect("direct GET failed");
    assert_eq!(val, "value1");

    // Multiple keys to exercise hash distribution.
    for i in 0..10 {
        let key = format!("direct_batch_{}", i);
        let value = format!("val_{}", i);
        client.put(&key, &value).await.expect("batch PUT failed");
    }
    for i in 0..10 {
        let key = format!("direct_batch_{}", i);
        let value = format!("val_{}", i);
        let got = client.get(&key).await.expect("batch GET failed");
        assert_eq!(got, value, "mismatch for key {}", key);
    }
}
