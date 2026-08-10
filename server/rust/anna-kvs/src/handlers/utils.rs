//! Shared utility functions for KVS handlers.
//!
//! Mirrors `server/cpp/src/kvs/utils.cpp`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anna_server_common::metadata::{is_metadata, KeyProperty};
use anna_server_common::proto::kvs::{AnnaError, KeyRequest, KeyTuple, LatticeType, RequestType};
use anna_server_common::types::{Address, Key};
use prost::Message;

use crate::context::{AddressKeysetMap, OutgoingMessage};
use crate::storage::{Serializer, SerializerMap};

/// Process a GET request for a key.
/// Returns `(payload_bytes, error_code)`.
pub(crate) fn process_get(key: &str, serializer: &dyn Serializer) -> (Vec<u8>, i32) {
    let (data, err) = serializer.get(key);
    if err != 0 {
        (vec![], AnnaError::KeyDne as i32)
    } else {
        (data, AnnaError::NoError as i32)
    }
}

/// Process a PUT request for a key.
/// Returns the serialized size on success.
pub(crate) fn process_put(
    key: &str,
    lattice_type: LatticeType,
    payload: &[u8],
    serializer: &mut dyn Serializer,
    stored_key_map: &mut HashMap<Key, KeyProperty>,
    expiry_epoch_ms: u64,
) -> usize {
    let result = serializer.put(key, payload);

    let kp = stored_key_map.entry(key.to_string()).or_default();
    kp.set_size(result as u32);
    kp.set_type(lattice_type);

    if expiry_epoch_ms > 0 {
        // Client-specified absolute expiry (ms → s).
        kp.expiry_epoch_s = (expiry_epoch_ms / 1000) as u32;
    } else if result == 0 {
        // Tombstone (delete): set expiry for GC.
        let now_s = now_epoch_s();
        if kp.expiry_epoch_s == 0 || kp.size() > 0 {
            kp.expiry_epoch_s = now_s + DEFAULT_TOMBSTONE_GC_S;
        }
    } else if result > 0 && expiry_epoch_ms == 0 {
        // Non-empty value with no expiry: clear any previous expiry.
        kp.expiry_epoch_s = 0;
    }

    result
}

/// Build gossip KeyRequest messages from an address-keyset map.
/// Returns a list of outgoing messages to send via ZMQ PUSH.
pub(crate) fn build_gossip_messages(
    addr_keyset_map: &AddressKeysetMap,
    serializers: &HashMap<i32, Box<dyn Serializer>>,
    stored_key_map: &HashMap<Key, KeyProperty>,
) -> Vec<OutgoingMessage> {
    let mut messages = Vec::new();

    for (addr, keys) in addr_keyset_map {
        let mut request = KeyRequest {
            r#type: RequestType::Put as i32,
            ..Default::default()
        };

        for key in keys {
            let kp = match stored_key_map.get(key.as_str()) {
                Some(kp) => kp,
                None => continue,
            };
            let lt = kp.lattice_type();
            let serializer = match serializers.get(&(lt as i32)) {
                Some(s) => s,
                None => continue,
            };
            let (payload, err) = process_get(key, serializer.as_ref());
            if err != AnnaError::NoError as i32 {
                continue;
            }

            let mut tuple = KeyTuple {
                key: key.clone(),
                lattice_type: lt as i32,
                payload,
                ..Default::default()
            };
            if kp.expiry_epoch_s > 0 {
                tuple.expiry_epoch_ms = kp.expiry_epoch_s as u64 * 1000;
            }
            request.tuples.push(tuple);
        }

        if !request.tuples.is_empty() {
            messages.push((addr.clone(), request.encode_to_vec()));
        }
    }

    messages
}

/// Remove expired keys from the store.
/// Returns the number of keys reaped.
pub(crate) fn gc_reap_expired_keys(
    stored_key_map: &mut HashMap<Key, KeyProperty>,
    serializers: &mut SerializerMap,
) -> usize {
    let now_s = now_epoch_s();
    let expired: Vec<Key> = stored_key_map
        .iter()
        .filter(|(k, kp)| kp.expiry_epoch_s > 0 && now_s >= kp.expiry_epoch_s && !is_metadata(k))
        .map(|(k, _)| k.clone())
        .collect();

    let count = expired.len();
    for key in &expired {
        if let Some(kp) = stored_key_map.get(key) {
            let lt = kp.lattice_type() as i32;
            if let Some(serializer) = serializers.get_mut(&lt) {
                serializer.remove(key);
            }
        }
        stored_key_map.remove(key);
    }
    count
}

/// Current epoch time in seconds.
pub(crate) fn now_epoch_s() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

/// Generate a monotonic timestamp combining wall-clock millis with a thread ID.
/// Mirrors C++ `generate_timestamp` — dynamically scales the multiplier so
/// that any thread ID fits without collision.
pub(crate) fn generate_timestamp(tid: u32) -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut pow: u64 = 10;
    while (tid as u64) >= pow {
        pow *= 10;
    }
    millis * pow + tid as u64
}

/// Default tombstone GC timeout in seconds (gossip_epoch * tombstone_gc_multiplier).
/// The actual values come from config but this provides a reasonable default.
const DEFAULT_TOMBSTONE_GC_S: u32 = 300; // 10s gossip * 30 multiplier

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::LwwSerializer;

    #[test]
    fn process_get_missing_key() {
        let serializer = LwwSerializer::new();
        let (_, err) = process_get("missing", &serializer);
        assert_eq!(err, AnnaError::KeyDne as i32);
    }

    #[test]
    fn process_put_and_get() {
        let mut serializer = LwwSerializer::new();
        let mut stored = HashMap::new();

        let payload = b"\x08\x01\x12\x05hello"; // LWW: ts=1, value="hello"
        let result = process_put(
            "key1",
            LatticeType::Lww,
            payload,
            &mut serializer,
            &mut stored,
            0,
        );
        assert!(result > 0);
        assert!(stored.contains_key("key1"));

        let (data, err) = process_get("key1", &serializer);
        assert_eq!(err, AnnaError::NoError as i32);
        assert!(!data.is_empty());
    }

    #[test]
    fn gc_reap_removes_expired() {
        let mut serializer = LwwSerializer::new();
        let mut stored = HashMap::new();
        let mut serializers: SerializerMap = HashMap::new();

        let payload = b"\x08\x01\x12\x05hello";
        process_put(
            "expire_me",
            LatticeType::Lww,
            payload,
            &mut serializer,
            &mut stored,
            0,
        );
        // Force expiry to the past
        stored.get_mut("expire_me").unwrap().expiry_epoch_s = 1;

        serializers.insert(LatticeType::Lww as i32, Box::new(serializer));

        let reaped = gc_reap_expired_keys(&mut stored, &mut serializers);
        assert_eq!(reaped, 1);
        assert!(!stored.contains_key("expire_me"));
    }

    #[test]
    fn generate_timestamp_monotonic() {
        let t1 = generate_timestamp(0);
        let t2 = generate_timestamp(0);
        assert!(t2 >= t1);
    }

    #[test]
    fn generate_timestamp_different_tids_no_collision() {
        // tid 0 and tid 10 in the same millisecond must produce different values
        let t0 = generate_timestamp(0);
        let t10 = generate_timestamp(10);
        // They may be in different milliseconds, but if same ms, they differ
        // because tid=10 uses pow=100 while tid=0 uses pow=10.
        assert_ne!(t0, t10);
    }
}
