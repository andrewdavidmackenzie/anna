//! Management node response handler — updates the list of extant caches
//! and queries their cached keys.
//!
//! Mirrors `server/cpp/src/kvs/management_node_response_handler.cpp`.

use anna_server_common::metadata::get_metadata_key;
use anna_server_common::proto::kvs::{KeyRequest, KeyTuple, LatticeType, RequestType};
use anna_server_common::proto::shared::StringSet;
use anna_server_common::routing::metadata_request_target;
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage};

/// Handle a management node response containing the list of cache nodes.
pub fn handle(ctx: &mut KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let func_nodes = match StringSet::decode(data) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("management_node_response: decode failed: {}", e);
            return vec![];
        }
    };

    // Replace extant_caches, tracking deleted caches.
    let mut deleted_caches = std::mem::take(&mut ctx.extant_caches);
    for node in &func_nodes.keys {
        deleted_caches.remove(node);
        ctx.extant_caches.insert(node.clone());
    }

    // Remove mappings for deleted caches.
    for cache_ip in &deleted_caches {
        ctx.cache_ip_to_keys.remove(cache_ip);
        for (_, caches) in ctx.key_to_cache_ips.iter_mut() {
            caches.remove(cache_ip);
        }
    }

    // Query cached keys for each extant cache.
    let mut outgoing = Vec::new();
    for cache_ip in &ctx.extant_caches {
        let meta_key = get_metadata_key(cache_ip, "cache_ip");
        let target = metadata_request_target(
            &meta_key,
            &ctx.global_hash_rings,
            &ctx.local_hash_rings,
            ctx.metadata_replication_factor,
            ctx.default_local_replication,
        );

        if let Some(thread) = target {
            ctx.rid += 1;
            let request = KeyRequest {
                request_id: format!("{}:{}", ctx.wt.cache_ip_response_connect_address(), ctx.rid),
                response_address: ctx.wt.cache_ip_response_connect_address(),
                r#type: RequestType::Get as i32,
                tuples: vec![KeyTuple {
                    key: meta_key,
                    lattice_type: LatticeType::Lww as i32,
                    ..Default::default()
                }],
                ..Default::default()
            };
            outgoing.push((
                thread.key_request_connect_address(),
                request.encode_to_vec(),
            ));
        }
    }

    outgoing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_extant_caches() {
        let mut ctx = crate::context::test_support::make_test_ctx();
        ctx.extant_caches.insert("old_cache".into());

        let msg = StringSet {
            keys: vec!["new_cache".into()],
        };
        let _ = handle(&mut ctx, &msg.encode_to_vec());

        assert!(ctx.extant_caches.contains("new_cache"));
        assert!(!ctx.extant_caches.contains("old_cache"));
    }

    #[test]
    fn cleans_up_deleted_cache_mappings() {
        let mut ctx = crate::context::test_support::make_test_ctx();
        ctx.extant_caches.insert("dead_cache".into());
        ctx.cache_ip_to_keys
            .entry("dead_cache".into())
            .or_default()
            .insert("key1".into());
        ctx.key_to_cache_ips
            .entry("key1".into())
            .or_default()
            .insert("dead_cache".into());

        let msg = StringSet { keys: vec![] };
        let _ = handle(&mut ctx, &msg.encode_to_vec());

        assert!(!ctx.cache_ip_to_keys.contains_key("dead_cache"));
        assert!(!ctx
            .key_to_cache_ips
            .get("key1")
            .map_or(false, |s| s.contains("dead_cache")));
    }
}
