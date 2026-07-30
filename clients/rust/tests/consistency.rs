//! Consistency semantics tests: verify that each lattice type merges
//! concurrent writes correctly according to its consistency guarantees.
//!
//! Tests run against both memory-tier and disk-tier KVS nodes to exercise
//! both Memory*Serializer and Disk*Serializer merge paths.

mod common;

use annalib::kvs_client::KVSClient;
use common::{client_config, generate_config, generate_disk_config, ServerGuard};

const MEMORY_BASE_OFFSET: u16 = 250;
const DISK_BASE_OFFSET: u16 = 251;

/// Core consistency tests: verify merge semantics for all lattice types.
/// Shared between memory and disk tier tests.
async fn test_consistency(client: &mut KVSClient, prefix: &str) {
    // === LWW: last writer wins ===
    let lww_key = format!("{prefix}_lww");
    client
        .put(&lww_key, "first")
        .await
        .expect("PUT first failed");
    assert_eq!(client.get(&lww_key).await.unwrap(), "first");

    std::thread::sleep(std::time::Duration::from_millis(10));
    client
        .put(&lww_key, "second")
        .await
        .expect("PUT second failed");
    assert_eq!(
        client.get(&lww_key).await.unwrap(),
        "second",
        "LWW: later timestamp should win"
    );

    // === Set: union merge ===
    #[cfg(feature = "set")]
    {
        let set_key = format!("{prefix}_set");
        client.put_set(&set_key, &["a", "b"]).await.unwrap();
        client.put_set(&set_key, &["b", "c"]).await.unwrap();
        let values = client.get_set(&set_key).await.unwrap();
        assert!(values.contains(&"a".to_string()));
        assert!(values.contains(&"b".to_string()));
        assert!(values.contains(&"c".to_string()));
    }

    // === OrderedSet: union merge ===
    #[cfg(feature = "set")]
    {
        let oset_key = format!("{prefix}_oset");
        client
            .put_ordered_set(&oset_key, &["x", "y"])
            .await
            .unwrap();
        client
            .put_ordered_set(&oset_key, &["y", "z"])
            .await
            .unwrap();
        let values = client.get_ordered_set(&oset_key).await.unwrap();
        assert!(values.len() >= 2, "Ordered set should merge elements");
    }

    // === Priority: lowest wins ===
    let pri_key = format!("{prefix}_pri");
    client.put_priority(&pri_key, 10.0, "high").await.unwrap();
    client.put_priority(&pri_key, 1.0, "low").await.unwrap();
    let (priority, value) = client.get_priority(&pri_key).await.unwrap();
    assert!(
        priority <= 1.0,
        "Lowest priority should win, got {}",
        priority
    );
    assert_eq!(value, "low");

    // === SingleCausal: vector clock merge ===
    #[cfg(feature = "causal")]
    {
        let sc_key = format!("{prefix}_sc");
        client.put_single_causal(&sc_key, "v1").await.unwrap();
        let (vc, values) = client.get_single_causal(&sc_key).await.unwrap();
        assert!(!vc.is_empty(), "Vector clock should be present");
        assert!(!values.is_empty(), "Should have a value");

        client.put_single_causal(&sc_key, "v2").await.unwrap();
        let (vc2, values2) = client.get_single_causal(&sc_key).await.unwrap();
        assert!(!vc2.is_empty(), "Updated vector clock should exist");
        assert!(!values2.is_empty(), "Should have updated value");
    }

    // === MultiCausal: dependency tracking ===
    #[cfg(feature = "causal")]
    {
        let mc_a = format!("{prefix}_mc_a");
        let mc_b = format!("{prefix}_mc_b");
        client.put_causal(&mc_a, "value_a").await.unwrap();
        client.put_causal(&mc_b, "value_b").await.unwrap();

        let (vc_a, _deps_a, val_a) = client.get_causal(&mc_a).await.unwrap();
        assert!(!vc_a.is_empty());
        assert!(!val_a.is_empty());

        let (vc_b, deps_b, val_b) = client.get_causal(&mc_b).await.unwrap();
        assert!(!vc_b.is_empty());
        assert!(!val_b.is_empty());
        assert!(!deps_b.is_empty(), "Key B should have dependencies");

        client.put_causal(&mc_a, "value_a_v2").await.unwrap();
        let (vc_a2, _deps_a2, val_a2) = client.get_causal(&mc_a).await.unwrap();
        assert!(!vc_a2.is_empty());
        assert!(!val_a2.is_empty());
    }
    // === LWW_SET: last-writer-wins set ===
    {
        use annalib::value::Value;
        let lww_set_key = format!("{prefix}_lww_set");
        let val1 = Value::LwwSet(vec!["a".into(), "b".into(), "c".into()]);
        client
            .put_value(&lww_set_key, &val1)
            .await
            .expect("PUT LWW_SET first failed");

        let got = client
            .get_value(&lww_set_key)
            .await
            .expect("GET LWW_SET first failed");
        match &got {
            Value::LwwSet(vals) => {
                let mut sorted = vals.clone();
                sorted.sort();
                assert_eq!(sorted, vec!["a", "b", "c"]);
            }
            other => panic!("Expected LwwSet, got {:?}", other.type_name()),
        }

        // Second PUT should replace the entire set (LWW semantics).
        std::thread::sleep(std::time::Duration::from_millis(10));
        let val2 = Value::LwwSet(vec!["x".into(), "y".into()]);
        client
            .put_value(&lww_set_key, &val2)
            .await
            .expect("PUT LWW_SET second failed");

        let got2 = client
            .get_value(&lww_set_key)
            .await
            .expect("GET LWW_SET second failed");
        match &got2 {
            Value::LwwSet(vals) => {
                let mut sorted = vals.clone();
                sorted.sort();
                assert_eq!(sorted, vec!["x", "y"], "LWW_SET: latest set should replace");
            }
            other => panic!("Expected LwwSet, got {:?}", other.type_name()),
        }
    }
}

#[tokio::test]
#[cfg(unix)]
async fn consistency_all_lattice_types() {
    let config_path = generate_config(MEMORY_BASE_OFFSET);
    let _guard = ServerGuard::start(&config_path, MEMORY_BASE_OFFSET);
    let config = client_config(MEMORY_BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(30)).await;

    test_consistency(&mut client, "mem").await;
}

/// Same consistency tests running against a disk-tier KVS.
/// Exercises the read-merge-write cycle in all Disk*Serializer classes.
#[tokio::test]
#[cfg(unix)]
async fn consistency_disk_tier() {
    let config_path = generate_disk_config(DISK_BASE_OFFSET);
    let _guard = ServerGuard::start_disk(&config_path, DISK_BASE_OFFSET);
    let config = client_config(DISK_BASE_OFFSET);
    let mut client = KVSClient::new(&config, Some(31)).await;

    test_consistency(&mut client, "disk").await;
}
