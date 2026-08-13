//! Replication response handler — processes replication factor lookups
//! and drains pending address requests.
//!
//! Mirrors `server/cpp/src/route/replication_response_handler.cpp`.

use anna_server_common::metadata::{get_key_from_metadata, init_replication, Tier};
use anna_server_common::proto::kvs::{
    key_address_response::KeyAddress, AnnaError, KeyAddressResponse, KeyResponse, KeyTuple,
    LwwValue,
};
use anna_server_common::proto::metadata::ReplicationFactor;
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use prost::Message;

use crate::context::{OutgoingMessage, RouteContext};

fn tier_from_i32(v: i32) -> Option<Tier> {
    match v {
        1 => Some(Tier::Memory),
        2 => Some(Tier::Disk),
        3 => Some(Tier::Routing),
        _ => None,
    }
}

/// Handle a replication factor response from a KVS node.
pub(crate) fn handle(ctx: &mut RouteContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let response = match KeyResponse::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("replication_response: decode failed: {}", e);
            return vec![];
        }
    };

    if response.tuples.is_empty() {
        return vec![];
    }

    let tuple = &response.tuples[0];
    let key = match get_key_from_metadata(&tuple.key) {
        Some(k) => k.to_string(),
        None => return vec![],
    };

    let error = tuple.error;

    if error == AnnaError::NoError as i32 {
        if let Ok(lww) = LwwValue::decode(tuple.payload.as_slice()) {
            if let Ok(rep_data) = ReplicationFactor::decode(lww.value.as_slice()) {
                let kr = ctx.key_replication_map.entry(key.clone()).or_default();
                for global in &rep_data.global {
                    if let Some(tier) = tier_from_i32(global.tier) {
                        kr.global_replication.insert(tier, global.value);
                    }
                }
                for local in &rep_data.local {
                    if let Some(tier) = tier_from_i32(local.tier) {
                        kr.local_replication.insert(tier, local.value);
                    }
                }
            }
        }
    } else if error == AnnaError::KeyDne as i32 {
        init_replication(
            &mut ctx.key_replication_map,
            &key,
            &std::collections::HashMap::new(), // no tier_metadata in route
            ctx.default_local_replication,
        );
    } else if error == AnnaError::WrongThread as i32 {
        // Will be retried on next address request.
        return vec![];
    } else {
        return vec![];
    }

    // Drain pending requests for this key.
    let mut outgoing = Vec::new();

    if let Some(pending) = ctx.pending_requests.remove(&key) {
        let is_meta = anna_server_common::metadata::is_metadata(&key);
        let result = get_responsible_threads(
            &key,
            is_meta,
            &ctx.global_hash_rings,
            &ctx.local_hash_rings,
            &ctx.key_replication_map,
            ctx.metadata_replication_factor,
            ctx.default_local_replication,
        );

        if let ResponsibleResult::Ok(threads) = result {
            for (response_addr, request_id) in &pending {
                let mut addr_resp = KeyAddressResponse {
                    response_id: request_id.clone(),
                    ..Default::default()
                };
                let mut key_addr = KeyAddress {
                    key: key.clone(),
                    ..Default::default()
                };
                for thread in &threads {
                    key_addr.ips.push(thread.key_request_connect_address());
                }
                addr_resp.addresses.push(key_addr);
                outgoing.push((response_addr.clone(), addr_resp.encode_to_vec()));
            }
        }
    }

    outgoing
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
    use anna_server_common::metadata::Tier;
    use anna_server_common::proto::metadata::replication_factor::ReplicationValue;

    fn make_rep_response(data_key: &str, mem_rep: u32) -> Vec<u8> {
        let rep = ReplicationFactor {
            key: data_key.into(),
            global: vec![ReplicationValue {
                tier: Tier::Memory as i32,
                value: mem_rep,
            }],
            local: vec![],
        };
        let lww = LwwValue {
            timestamp: 1,
            value: rep.encode_to_vec(),
        };
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: format!("ANNA_METADATA|replication|{}", data_key),
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    #[test]
    fn updates_replication_map() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let data = make_rep_response("my_key", 2);
        let _ = handle(&mut ctx, &data);
        assert_eq!(
            ctx.key_replication_map["my_key"].global_replication[&Tier::Memory],
            2
        );
    }

    #[test]
    fn drains_pending_requests() {
        let mut ctx = crate::context::tests::make_test_ctx();
        // Add a node to the ring so we can resolve addresses.
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

        // Park a pending request.
        ctx.pending_requests.insert(
            "pending_key".into(),
            vec![("tcp://127.0.0.1:6650".into(), "req_1".into())],
        );

        let data = make_rep_response("pending_key", 1);
        let msgs = handle(&mut ctx, &data);

        assert!(!ctx.pending_requests.contains_key("pending_key"));
        assert!(!msgs.is_empty());
    }

    #[test]
    fn key_dne_uses_defaults() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: "ANNA_METADATA|replication|default_key".into(),
                error: AnnaError::KeyDne as i32,
                ..Default::default()
            }],
            ..Default::default()
        };
        let _ = handle(&mut ctx, &response.encode_to_vec());
        assert!(ctx.key_replication_map.contains_key("default_key"));
    }

    #[test]
    fn wrong_thread_returns_empty() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: "ANNA_METADATA|replication|wt_key".into(),
                error: AnnaError::WrongThread as i32,
                ..Default::default()
            }],
            ..Default::default()
        };
        let msgs = handle(&mut ctx, &response.encode_to_vec());
        assert!(msgs.is_empty());
    }
}
