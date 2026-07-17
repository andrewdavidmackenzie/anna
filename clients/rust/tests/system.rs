//! System test: drive KVSClient library API directly against a live server.

mod common;

use common::{config_file, server_path, start_servers, ServerGuard};

#[tokio::test]
#[cfg(unix)]
async fn system_test_kvs_client() {
    use annalib::config::Config;
    use annalib::kvs_client::KVSClient;

    let path = server_path();
    let config_path = config_file();

    start_servers(&path, &config_path);
    let _guard = ServerGuard {
        path,
        config: config_path,
    };

    let config =
        Config::read(&std::path::PathBuf::from(&_guard.config)).expect("Failed to read config");
    let mut client = KVSClient::new(&config, Some(50)).await;

    // PUT and GET a LWW value
    client.put("sys_test_a", "hello").await.expect("PUT failed");
    let val = client.get("sys_test_a").await.expect("GET failed");
    assert_eq!(val, "hello", "GET returned wrong value");

    // Overwrite
    client
        .put("sys_test_a", "world")
        .await
        .expect("PUT overwrite failed");
    let val = client
        .get("sys_test_a")
        .await
        .expect("GET after overwrite failed");
    assert_eq!(val, "world", "GET after overwrite returned wrong value");

    // Multiple keys
    client.put("sys_test_b", "42").await.expect("PUT b failed");
    let val_a = client.get("sys_test_a").await.expect("GET a failed");
    let val_b = client.get("sys_test_b").await.expect("GET b failed");
    assert_eq!(val_a, "world");
    assert_eq!(val_b, "42");

    // PUT_SET and GET_SET
    #[cfg(feature = "set")]
    {
        client
            .put_set("sys_test_set", &["x", "y", "z"])
            .await
            .expect("PUT_SET failed");
        let set_val = client
            .get_set("sys_test_set")
            .await
            .expect("GET_SET failed");
        assert!(set_val.contains(&"x".to_string()));
        assert!(set_val.contains(&"y".to_string()));
        assert!(set_val.contains(&"z".to_string()));
        assert_eq!(set_val.len(), 3);

        // SET union
        client
            .put_set("sys_test_set", &["w", "x"])
            .await
            .expect("PUT_SET union failed");
        let set_val = client
            .get_set("sys_test_set")
            .await
            .expect("GET_SET after union failed");
        assert!(
            set_val.len() >= 3,
            "Expected at least 3 elements, got {}",
            set_val.len()
        );
        assert!(set_val.contains(&"x".to_string()));
        assert!(set_val.contains(&"w".to_string()));

        // ORDERED_SET
        client
            .put_ordered_set("sys_test_oset", &["alpha", "beta", "gamma"])
            .await
            .expect("PUT_ORDERED_SET failed");
        let oset_val = client
            .get_ordered_set("sys_test_oset")
            .await
            .expect("GET_ORDERED_SET failed");
        assert!(oset_val.contains(&"alpha".to_string()));
        assert!(oset_val.contains(&"beta".to_string()));
        assert!(oset_val.contains(&"gamma".to_string()));
        assert_eq!(oset_val.len(), 3, "ORDERED_SET should have 3 elements");
    }

    // SINGLE_CAUSAL
    #[cfg(feature = "causal")]
    {
        client
            .put_single_causal("sys_test_sc", "sc_hello")
            .await
            .expect("PUT_SINGLE_CAUSAL failed");
        let (vc, values) = client
            .get_single_causal("sys_test_sc")
            .await
            .expect("GET_SINGLE_CAUSAL failed");
        assert!(
            values.contains(&"sc_hello".to_string()),
            "SINGLE_CAUSAL values should contain 'sc_hello'"
        );
        assert!(
            vc.contains_key("test"),
            "SINGLE_CAUSAL vector clock should have 'test' key"
        );
    }

    // MULTI_CAUSAL
    #[cfg(feature = "causal")]
    {
        client
            .put_causal("sys_test_mc", "mc_hello")
            .await
            .expect("PUT_CAUSAL failed");
        let (vc, deps, value) = client
            .get_causal("sys_test_mc")
            .await
            .expect("GET_CAUSAL failed");
        assert_eq!(value, "mc_hello", "MULTI_CAUSAL value should be 'mc_hello'");
        assert!(
            vc.contains_key("test"),
            "MULTI_CAUSAL vector clock should have 'test' key"
        );
        assert!(
            deps.iter().any(|(k, _)| k == "dep1"),
            "MULTI_CAUSAL dependencies should have 'dep1'"
        );
    }

    // PRIORITY
    client
        .put_priority("sys_test_pri", 1.5, "important")
        .await
        .expect("PUT_PRIORITY failed");
    let (priority, pri_value) = client
        .get_priority("sys_test_pri")
        .await
        .expect("GET_PRIORITY failed");
    assert!(
        (priority - 1.5).abs() < f64::EPSILON,
        "PRIORITY should be 1.5, got {}",
        priority
    );
    assert_eq!(
        pri_value, "important",
        "PRIORITY value should be 'important'"
    );

    // DELETE
    client
        .put("sys_test_del", "to_delete")
        .await
        .expect("PUT failed");
    let del_val = client.get("sys_test_del").await.expect("GET failed");
    assert_eq!(
        del_val, "to_delete",
        "Value before delete should be 'to_delete'"
    );
    client.delete("sys_test_del").await.expect("DELETE failed");

    // MULTI-KEY GET
    client
        .put("multi_a", "val_a")
        .await
        .expect("PUT multi_a failed");
    client
        .put("multi_b", "val_b")
        .await
        .expect("PUT multi_b failed");
    client
        .put("multi_c", "val_c")
        .await
        .expect("PUT multi_c failed");
    let results = client
        .get_multi(&["multi_a", "multi_b", "multi_c"])
        .await
        .expect("GET_MULTI failed");
    assert_eq!(results.len(), 3, "GET_MULTI should return 3 results");
    assert_eq!(results["multi_a"], "val_a");
    assert_eq!(results["multi_b"], "val_b");
    assert_eq!(results["multi_c"], "val_c");

    // MULTI-KEY GET with empty list
    let empty_results = client
        .get_multi::<String>(&[])
        .await
        .expect("GET_MULTI empty failed");
    assert!(empty_results.is_empty());
}
