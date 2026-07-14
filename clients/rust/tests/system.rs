//! System test: drive KVSClient library API directly against a live server.

mod common;

use common::{config_file, server_path, start_servers, ServerGuard};

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
    client
        .put("sys_test_a", "world")
        .expect("PUT overwrite failed");
    let val = client
        .get("sys_test_a")
        .expect("GET after overwrite failed");
    assert_eq!(val, "world", "GET after overwrite returned wrong value");

    // Multiple keys
    client.put("sys_test_b", "42").expect("PUT b failed");
    let val_a = client.get("sys_test_a").expect("GET a failed");
    let val_b = client.get("sys_test_b").expect("GET b failed");
    assert_eq!(val_a, "world");
    assert_eq!(val_b, "42");

    // PUT_SET and GET_SET
    #[cfg(feature = "set")]
    {
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
        let set_val = client
            .get_set("sys_test_set")
            .expect("GET_SET after union failed");
        assert!(
            set_val.len() >= 3,
            "Expected at least 3 elements, got {}",
            set_val.len()
        );
        assert!(set_val.contains(&"x".to_string()));
        assert!(set_val.contains(&"w".to_string()));
    }
}
