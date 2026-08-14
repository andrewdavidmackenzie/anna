//! Cache registration handler — registers a cache node and its watched keys.
//!
//! Mirrors `server/cpp/src/kvs/cache_registration_handler.cpp`.

use anna_server_common::proto::shared::StringSet;
use prost::Message;

use crate::context::KvsContext;

/// Handle a cache registration message.
///
/// Format: `StringSet` protobuf where `keys[0]` is the cache IP and
/// `keys[1..]` are the keys that cache is watching.
pub fn handle(ctx: &mut KvsContext, data: &[u8]) {
    let msg = match StringSet::decode(data) {
        Ok(m) => m,
        Err(e) => {
            log::error!("cache_registration: decode failed: {}", e);
            return;
        }
    };

    if msg.keys.is_empty() {
        log::error!("Cache registration message with no cache IP.");
        return;
    }

    let cache_ip = &msg.keys[0];
    ctx.extant_caches.insert(cache_ip.clone());

    for key in &msg.keys[1..] {
        ctx.cache_ip_to_keys
            .entry(cache_ip.clone())
            .or_default()
            .insert(key.clone());
        ctx.key_to_cache_ips
            .entry(key.clone())
            .or_default()
            .insert(cache_ip.clone());
    }

    log::info!(
        "Registered cache {} watching {} keys.",
        cache_ip,
        msg.keys.len() - 1
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn registers_cache_and_keys() {
        let mut ctx = crate::context::test_support::make_test_ctx();
        let msg = StringSet {
            keys: vec!["tcp://10.0.0.5:6850".into(), "key_a".into(), "key_b".into()],
        };
        handle(&mut ctx, &msg.encode_to_vec());

        assert!(ctx.extant_caches.contains("tcp://10.0.0.5:6850"));
        assert!(ctx.cache_ip_to_keys["tcp://10.0.0.5:6850"].contains("key_a"));
        assert!(ctx.key_to_cache_ips["key_a"].contains("tcp://10.0.0.5:6850"));
    }

    #[test]
    fn empty_message_ignored() {
        let mut ctx = crate::context::test_support::make_test_ctx();
        let msg = StringSet { keys: vec![] };
        handle(&mut ctx, &msg.encode_to_vec());
        assert!(ctx.extant_caches.is_empty());
    }
}
