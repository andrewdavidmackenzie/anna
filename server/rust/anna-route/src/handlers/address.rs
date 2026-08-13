//! Address handler — resolves key-to-server routing queries from clients.
//!
//! Mirrors `server/cpp/src/route/address_handler.cpp`.

use anna_server_common::metadata::is_metadata;
use anna_server_common::proto::kvs::{
    key_address_response::KeyAddress, AnnaError, KeyAddressRequest, KeyAddressResponse,
};
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use prost::Message;

use crate::context::{OutgoingMessage, PendingRequest, RouteContext};

/// Handle a key address request from a client.
pub(crate) fn handle(ctx: &mut RouteContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let request = match KeyAddressRequest::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("address: decode failed: {}", e);
            return vec![];
        }
    };

    let mut response = KeyAddressResponse {
        response_id: request.request_id.clone(),
        ..Default::default()
    };

    for key in &request.keys {
        let is_meta = is_metadata(key);

        // Check if any servers exist.
        if ctx.global_hash_rings.values().all(|r| r.is_empty()) {
            response.error = AnnaError::NoServers as i32;
            break;
        }

        let result = get_responsible_threads(
            key,
            is_meta,
            &ctx.global_hash_rings,
            &ctx.local_hash_rings,
            &ctx.key_replication_map,
            ctx.metadata_replication_factor,
            ctx.default_local_replication,
        );

        match result {
            ResponsibleResult::Ok(threads) => {
                let mut addr = KeyAddress {
                    key: key.clone(),
                    ..Default::default()
                };
                for thread in &threads {
                    addr.ips.push(thread.key_request_connect_address());
                }
                response.addresses.push(addr);
            }
            ResponsibleResult::NeedReplicationFactor(_) => {
                // Initialize with defaults and retry immediately.
                anna_server_common::metadata::init_replication(
                    &mut ctx.key_replication_map,
                    &key.to_string(),
                    &std::collections::HashMap::new(),
                    ctx.default_local_replication,
                );
                let retry = get_responsible_threads(
                    key,
                    is_meta,
                    &ctx.global_hash_rings,
                    &ctx.local_hash_rings,
                    &ctx.key_replication_map,
                    ctx.metadata_replication_factor,
                    ctx.default_local_replication,
                );
                if let ResponsibleResult::Ok(threads) = retry {
                    let mut addr = KeyAddress {
                        key: key.clone(),
                        ..Default::default()
                    };
                    for thread in &threads {
                        addr.ips.push(thread.key_request_connect_address());
                    }
                    response.addresses.push(addr);
                }
            }
        }
    }

    // Only send response if we have addresses (not parked).
    if !response.addresses.is_empty() || response.error != AnnaError::NoError as i32 {
        let response_addr = request.response_address.clone();
        if !response_addr.is_empty() {
            return vec![(response_addr, response.encode_to_vec())];
        }
    }

    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
    use anna_server_common::metadata::{KeyReplication, Tier};

    fn ctx_with_node() -> RouteContext {
        let mut ctx = crate::context::tests::make_test_ctx();
        let mut g_ring = ConsistentHashRing::new();
        g_ring.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            0,
            DEFAULT_VIRTUAL_THREAD_NUM,
            true,
        );
        let mut l_ring = ConsistentHashRing::new();
        l_ring.insert(
            "1.2.3.4",
            "10.0.0.1",
            0,
            0,
            DEFAULT_VIRTUAL_THREAD_NUM,
            false,
        );
        ctx.global_hash_rings.insert(Tier::Memory, g_ring);
        ctx.local_hash_rings.insert(Tier::Memory, l_ring);

        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("test_key".into(), kr);
        ctx
    }

    #[test]
    fn resolves_key_address() {
        let mut ctx = ctx_with_node();
        let request = KeyAddressRequest {
            request_id: "req_1".into(),
            response_address: "tcp://127.0.0.1:6650".into(),
            keys: vec!["test_key".into()],
        };
        let msgs = handle(&mut ctx, &request.encode_to_vec());
        assert_eq!(msgs.len(), 1);

        let response = KeyAddressResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.addresses.len(), 1);
        assert!(!response.addresses[0].ips.is_empty());
    }

    #[test]
    fn no_servers_returns_error() {
        let mut ctx = crate::context::tests::make_test_ctx();
        // Empty rings.
        let request = KeyAddressRequest {
            request_id: "req_2".into(),
            response_address: "tcp://127.0.0.1:6650".into(),
            keys: vec!["any_key".into()],
        };
        let msgs = handle(&mut ctx, &request.encode_to_vec());
        assert_eq!(msgs.len(), 1);

        let response = KeyAddressResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.error, AnnaError::NoServers as i32);
    }

    #[test]
    fn unknown_replication_uses_defaults() {
        let mut ctx = ctx_with_node();
        // Remove replication factor.
        ctx.key_replication_map.clear();

        let request = KeyAddressRequest {
            request_id: "req_3".into(),
            response_address: "tcp://127.0.0.1:6650".into(),
            keys: vec!["unknown_key".into()],
        };
        let msgs = handle(&mut ctx, &request.encode_to_vec());
        // Should resolve using default replication.
        assert!(!msgs.is_empty());
        // Replication map should have defaults.
        assert!(ctx.key_replication_map.contains_key("unknown_key"));
    }
}
