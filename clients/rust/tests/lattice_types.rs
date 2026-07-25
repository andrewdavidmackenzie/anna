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
    let mut client = KVSClient::new(&config, Some(50)).await;

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
    let mut client = KVSClient::new(&config, Some(51)).await;

    test_all_lattice_types(&mut client, "disk").await;
}
