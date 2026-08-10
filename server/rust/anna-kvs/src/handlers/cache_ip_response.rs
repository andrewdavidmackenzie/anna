//! Cache IP response handler — updates cache-to-key mappings from metadata lookups.
//!
//! Mirrors `server/cpp/src/kvs/cache_ip_response_handler.cpp`.

use std::collections::HashSet;

use anna_server_common::proto::kvs::{AnnaError, KeyResponse, LwwValue};
use anna_server_common::proto::shared::StringSet;
use prost::Message;

use crate::context::KvsContext;

/// Extract the cache IP from a user metadata key.
/// Format: `ANNA_METADATA|cache_ip|<cache_address>`
fn get_cache_ip_from_metadata(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("ANNA_METADATA|cache_ip|")?;
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// Handle a cache IP response — updates the bidirectional cache↔key mappings.
pub(crate) fn handle(ctx: &mut KvsContext, data: &[u8]) {
    let response = match KeyResponse::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("cache_ip_response: decode failed: {}", e);
            return;
        }
    };

    for tuple in &response.tuples {
        if tuple.error != AnnaError::NoError as i32 {
            continue; // KEY_DNE or WRONG_THREAD — ignore
        }

        let cache_ip = match get_cache_ip_from_metadata(&tuple.key) {
            Some(ip) => ip.to_string(),
            None => continue,
        };

        // Decode payload: LWWValue → StringSet
        let lww = match LwwValue::decode(tuple.payload.as_slice()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let key_set = match StringSet::decode(lww.value.as_slice()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let new_keys: HashSet<String> = key_set.keys.into_iter().collect();

        // Find keys that were in the old set but not in the new set.
        if let Some(old_keys) = ctx.cache_ip_to_keys.get(&cache_ip) {
            let deleted: Vec<String> = old_keys.difference(&new_keys).cloned().collect();
            for key in &deleted {
                if let Some(caches) = ctx.key_to_cache_ips.get_mut(key) {
                    caches.remove(&cache_ip);
                }
            }
        }

        // Replace the old mapping with the new one.
        ctx.cache_ip_to_keys
            .insert(cache_ip.clone(), new_keys.clone());

        // Add new keys to the reverse mapping.
        for key in &new_keys {
            ctx.key_to_cache_ips
                .entry(key.clone())
                .or_default()
                .insert(cache_ip.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::proto::kvs::KeyTuple;

    fn make_cache_response(cache_ip: &str, keys: &[&str]) -> Vec<u8> {
        let key_set = StringSet {
            keys: keys.iter().map(|s| s.to_string()).collect(),
        };
        let lww = LwwValue {
            timestamp: 1,
            value: key_set.encode_to_vec(),
        };
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: format!("ANNA_METADATA|cache_ip|{}", cache_ip),
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    #[test]
    fn updates_cache_key_mappings() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let data = make_cache_response("10.0.0.5", &["key_a", "key_b"]);
        handle(&mut ctx, &data);

        assert!(ctx.cache_ip_to_keys["10.0.0.5"].contains("key_a"));
        assert!(ctx.key_to_cache_ips["key_a"].contains("10.0.0.5"));
    }

    #[test]
    fn removes_stale_keys() {
        let mut ctx = crate::context::tests::make_test_ctx();

        // Initial: cache has key_a and key_b.
        let data1 = make_cache_response("10.0.0.5", &["key_a", "key_b"]);
        handle(&mut ctx, &data1);
        assert_eq!(ctx.cache_ip_to_keys["10.0.0.5"].len(), 2);

        // Update: cache now only has key_b (key_a removed).
        let data2 = make_cache_response("10.0.0.5", &["key_b"]);
        handle(&mut ctx, &data2);

        assert_eq!(ctx.cache_ip_to_keys["10.0.0.5"].len(), 1);
        assert!(!ctx
            .key_to_cache_ips
            .get("key_a")
            .map_or(false, |s| s.contains("10.0.0.5")));
    }
}
