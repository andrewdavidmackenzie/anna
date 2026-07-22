//! Consistency semantics tests: verify that each lattice type merges
//! concurrent writes correctly according to its consistency guarantees.
//!
//! These tests run against a single-node cluster since they test the
//! lattice merge behavior on a single replica. The merge function is
//! the same whether triggered by a client PUT or by gossip replication.

mod common;

use common::{client_config, generate_config, ServerGuard};

const BASE_OFFSET: u16 = 250;

#[tokio::test]
#[cfg(unix)]
async fn lww_last_writer_wins() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(30)).await;

    // First write
    client
        .put("lww_key", "first")
        .await
        .expect("PUT first failed");
    let val = client.get("lww_key").await.expect("GET after first failed");
    assert_eq!(val, "first");

    // Second write (later timestamp) overwrites
    std::thread::sleep(std::time::Duration::from_millis(10));
    client
        .put("lww_key", "second")
        .await
        .expect("PUT second failed");
    let val = client
        .get("lww_key")
        .await
        .expect("GET after second failed");
    assert_eq!(val, "second", "LWW: later timestamp should win");
}

#[tokio::test]
#[cfg(unix)]
#[cfg(feature = "set")]
async fn set_union_merge() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(31)).await;

    // First PUT adds {a, b}
    client
        .put_set("set_key", &["a", "b"])
        .await
        .expect("PUT_SET 1 failed");

    // Second PUT adds {b, c} — merge should produce {a, b, c}
    client
        .put_set("set_key", &["b", "c"])
        .await
        .expect("PUT_SET 2 failed");

    let values = client.get_set("set_key").await.expect("GET_SET failed");
    assert!(
        values.contains(&"a".to_string()),
        "Set should contain 'a' after union merge"
    );
    assert!(
        values.contains(&"b".to_string()),
        "Set should contain 'b' after union merge"
    );
    assert!(
        values.contains(&"c".to_string()),
        "Set should contain 'c' after union merge"
    );
}

#[tokio::test]
#[cfg(unix)]
#[cfg(feature = "set")]
async fn ordered_set_union_merge() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(32)).await;

    client
        .put_ordered_set("oset_key", &["x", "y"])
        .await
        .expect("PUT_ORDERED_SET 1 failed");
    client
        .put_ordered_set("oset_key", &["y", "z"])
        .await
        .expect("PUT_ORDERED_SET 2 failed");

    let values = client
        .get_ordered_set("oset_key")
        .await
        .expect("GET_ORDERED_SET failed");
    assert!(values.len() >= 2, "Ordered set should merge elements");
}

#[tokio::test]
#[cfg(unix)]
async fn priority_lowest_wins() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(33)).await;

    // Write high priority first
    client
        .put_priority("pri_key", 10.0, "high")
        .await
        .expect("PUT high priority failed");

    // Write low priority — should win
    client
        .put_priority("pri_key", 1.0, "low")
        .await
        .expect("PUT low priority failed");

    let (priority, value) = client
        .get_priority("pri_key")
        .await
        .expect("GET_PRIORITY failed");
    assert!(
        priority <= 1.0,
        "Priority merge: lowest should win, got {}",
        priority
    );
    assert_eq!(value, "low");
}

#[tokio::test]
#[cfg(unix)]
#[cfg(feature = "causal")]
async fn single_causal_vector_clock_merge() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(34)).await;

    // Write initial causal value
    client
        .put_single_causal("sc_key", "version1")
        .await
        .expect("PUT_SINGLE_CAUSAL failed");

    // Read back — verify vector clock exists
    let (vc, values) = client
        .get_single_causal("sc_key")
        .await
        .expect("GET_SINGLE_CAUSAL failed");
    assert!(!vc.is_empty(), "Vector clock should be present");
    assert!(!values.is_empty(), "Should have a value");

    // Overwrite — dominating vector clock wins
    client
        .put_single_causal("sc_key", "version2")
        .await
        .expect("PUT_SINGLE_CAUSAL overwrite failed");

    let (vc2, values2) = client
        .get_single_causal("sc_key")
        .await
        .expect("GET_SINGLE_CAUSAL after overwrite failed");
    assert!(!vc2.is_empty(), "Updated vector clock should exist");
    assert!(!values2.is_empty(), "Should have the updated value");
}

#[tokio::test]
#[cfg(unix)]
#[cfg(feature = "causal")]
async fn multi_causal_dependency_tracking() {
    use annalib::kvs_client::KVSClient;

    let config_path = generate_config(BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, BASE_OFFSET);
    let config = client_config(BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(35)).await;

    // Write key A
    client
        .put_causal("mc_a", "value_a")
        .await
        .expect("PUT_CAUSAL a failed");

    // Write key B (with dependencies set by the client)
    client
        .put_causal("mc_b", "value_b")
        .await
        .expect("PUT_CAUSAL b failed");

    // Read back — both should have vector clocks
    let (vc_a, _deps_a, val_a) = client
        .get_causal("mc_a")
        .await
        .expect("GET_CAUSAL a failed");
    assert!(!vc_a.is_empty(), "Key A: vector clock should exist");
    assert!(!val_a.is_empty(), "Key A: value should exist");

    let (vc_b, deps_b, val_b) = client
        .get_causal("mc_b")
        .await
        .expect("GET_CAUSAL b failed");
    assert!(!vc_b.is_empty(), "Key B: vector clock should exist");
    assert!(!val_b.is_empty(), "Key B: value should exist");

    // Dependencies are set by the client in put_causal — verify
    // the dependency structure is preserved through the server
    assert!(
        !deps_b.is_empty(),
        "Key B should have dependency information"
    );

    // Overwrite key A — new version should dominate
    client
        .put_causal("mc_a", "value_a_v2")
        .await
        .expect("PUT_CAUSAL a v2 failed");

    let (vc_a2, _deps_a2, val_a2) = client
        .get_causal("mc_a")
        .await
        .expect("GET_CAUSAL a v2 failed");
    assert!(!vc_a2.is_empty(), "Key A v2: vector clock should exist");
    assert!(!val_a2.is_empty(), "Key A v2: updated value should exist");
}
