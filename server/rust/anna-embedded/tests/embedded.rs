//! Integration tests for the embedded KVS.

use anna_embedded::EmbeddedKvs;
use std::sync::Arc;
use std::thread;

#[test]
fn put_and_get() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    kvs.put("key1", b"value1").unwrap();

    let result = kvs.get("key1").unwrap();
    assert_eq!(result, Some(b"value1".to_vec()));
}

#[test]
fn get_missing_key() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    let result = kvs.get("nonexistent").unwrap();
    assert_eq!(result, None);
}

#[test]
fn delete_key() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    kvs.put("del_me", b"gone").unwrap();
    assert_eq!(kvs.get("del_me").unwrap(), Some(b"gone".to_vec()));

    kvs.delete("del_me").unwrap();
    assert_eq!(kvs.get("del_me").unwrap(), None);
}

#[test]
fn delete_nonexistent_key() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    // Deleting a key that doesn't exist should not error.
    kvs.delete("never_existed").unwrap();
}

#[test]
fn overwrite_value() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    kvs.put("key", b"first").unwrap();
    kvs.put("key", b"second").unwrap();

    let result = kvs.get("key").unwrap();
    assert_eq!(result, Some(b"second".to_vec()));
}

#[test]
fn scan_all_keys() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    kvs.put("b_key", b"b").unwrap();
    kvs.put("a_key", b"a").unwrap();
    kvs.put("c_key", b"c").unwrap();

    let entries = kvs.scan("").unwrap();
    assert_eq!(entries.len(), 3);
    // Should be sorted.
    assert_eq!(entries[0].key, "a_key");
    assert_eq!(entries[1].key, "b_key");
    assert_eq!(entries[2].key, "c_key");
}

#[test]
fn scan_with_prefix() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    kvs.put("user:alice", b"a").unwrap();
    kvs.put("user:bob", b"b").unwrap();
    kvs.put("config:timeout", b"30").unwrap();

    let users = kvs.scan("user:").unwrap();
    assert_eq!(users.len(), 2);
    assert!(users.iter().all(|e| e.key.starts_with("user:")));

    let configs = kvs.scan("config:").unwrap();
    assert_eq!(configs.len(), 1);
}

#[test]
fn scan_excludes_deleted_keys() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    kvs.put("keep", b"yes").unwrap();
    kvs.put("remove", b"no").unwrap();
    kvs.delete("remove").unwrap();

    let entries = kvs.scan("").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key, "keep");
}

#[test]
fn multiple_actors_partition_keys() {
    let kvs = EmbeddedKvs::new(4).unwrap();

    // Write many keys — they should be distributed across actors.
    for i in 0..100 {
        kvs.put(&format!("key_{}", i), format!("val_{}", i).as_bytes())
            .unwrap();
    }

    // All keys should be retrievable.
    for i in 0..100 {
        let val = kvs.get(&format!("key_{}", i)).unwrap();
        assert_eq!(val, Some(format!("val_{}", i).into_bytes()));
    }

    let entries = kvs.scan("").unwrap();
    assert_eq!(entries.len(), 100);
}

#[test]
fn zero_actors_is_error() {
    let result = EmbeddedKvs::new(0);
    assert!(result.is_err());
}

#[test]
fn single_actor_works() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    kvs.put("solo", b"value").unwrap();
    assert_eq!(kvs.get("solo").unwrap(), Some(b"value".to_vec()));
}

#[test]
fn many_actors_works() {
    let kvs = EmbeddedKvs::new(16).unwrap();
    for i in 0..50 {
        kvs.put(&format!("k{}", i), b"v").unwrap();
    }
    let entries = kvs.scan("").unwrap();
    assert_eq!(entries.len(), 50);
}

#[test]
fn concurrent_writes() {
    let kvs = Arc::new(EmbeddedKvs::new(4).unwrap());
    let mut handles = Vec::new();

    for thread_id in 0..8 {
        let kvs = Arc::clone(&kvs);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let key = format!("t{}_k{}", thread_id, i);
                kvs.put(&key, b"value").unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // All 800 keys should be present.
    let entries = kvs.scan("").unwrap();
    assert_eq!(entries.len(), 800);
}

#[test]
fn concurrent_reads_and_writes() {
    let kvs = Arc::new(EmbeddedKvs::new(4).unwrap());

    // Pre-populate.
    for i in 0..100 {
        kvs.put(&format!("pre_{}", i), b"initial").unwrap();
    }

    let mut handles = Vec::new();

    // Writers overwrite existing keys.
    for thread_id in 0..4 {
        let kvs = Arc::clone(&kvs);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let key = format!("pre_{}", i);
                let val = format!("updated_by_{}", thread_id);
                kvs.put(&key, val.as_bytes()).unwrap();
            }
        }));
    }

    // Readers read concurrently.
    for _ in 0..4 {
        let kvs = Arc::clone(&kvs);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let key = format!("pre_{}", i);
                // Should always get a value (never None since we pre-populated
                // and writers only overwrite, never delete).
                let val = kvs.get(&key).unwrap();
                assert!(val.is_some(), "key {} should exist", key);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn ttl_key_expires() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    // Set a TTL of 1 second.
    kvs.put_with_ttl("ephemeral", b"temporary", 1).unwrap();

    // Should be readable immediately.
    assert_eq!(kvs.get("ephemeral").unwrap(), Some(b"temporary".to_vec()));

    // Wait for expiry.
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Should be gone.
    assert_eq!(kvs.get("ephemeral").unwrap(), None);
}

#[test]
fn large_values() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    let large_value = vec![42u8; 1_000_000]; // 1 MB
    kvs.put("large", &large_value).unwrap();

    let result = kvs.get("large").unwrap().unwrap();
    assert_eq!(result.len(), 1_000_000);
    assert!(result.iter().all(|&b| b == 42));
}

#[test]
fn empty_value() {
    let kvs = EmbeddedKvs::new(1).unwrap();
    kvs.put("empty", b"").unwrap();

    // Empty value is stored as an LWW tombstone, which means it's treated
    // as deleted. This is consistent with the Anna KVS semantics.
    let result = kvs.get("empty").unwrap();
    assert_eq!(result, None);
}

#[test]
fn binary_values() {
    let kvs = EmbeddedKvs::new(2).unwrap();
    let binary = vec![0u8, 1, 2, 255, 254, 253, 0, 0];
    kvs.put("binary", &binary).unwrap();

    let result = kvs.get("binary").unwrap().unwrap();
    assert_eq!(result, binary);
}
