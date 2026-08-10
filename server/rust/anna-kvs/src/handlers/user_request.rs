//! User request handler — dispatches GET/PUT requests from clients.
//!
//! Mirrors `server/cpp/src/kvs/user_request_handler.cpp`.

use std::time::Instant;

use anna_server_common::metadata::is_metadata;
use anna_server_common::proto::kvs::{
    AnnaError, KeyRequest, KeyResponse, KeyTuple, LatticeType, RequestType,
};
use anna_server_common::routing::{
    get_responsible_threads, replication_request_target, ResponsibleResult,
};
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage, PendingRequest};
use crate::handlers::utils::{now_epoch_s, process_get, process_put};

/// Handle a user GET/PUT request.
pub(crate) fn handle(ctx: &mut KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let request = match KeyRequest::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("user_request: decode failed: {}", e);
            return vec![];
        }
    };

    let mut response = KeyResponse {
        response_id: request.request_id.clone(),
        r#type: request.r#type,
        ..Default::default()
    };

    let mut outgoing: Vec<OutgoingMessage> = Vec::new();
    let request_type = request.r#type;
    let response_address = request.response_address.clone();

    for tuple in &request.tuples {
        let key = &tuple.key;
        let is_meta = is_metadata(key);

        let result = get_responsible_threads(
            key,
            is_meta,
            &ctx.global_hash_rings,
            &ctx.local_hash_rings,
            &ctx.key_replication_map,
            ctx.metadata_replication_factor,
            ctx.default_local_replication,
        );

        // Accept metadata keys stored locally regardless of hash ring.
        let is_own_metadata = is_meta && ctx.stored_key_map.contains_key(key.as_str());

        match result {
            ResponsibleResult::Ok(ref threads) => {
                let am_responsible = is_own_metadata || threads.iter().any(|t| *t == ctx.wt);

                if !am_responsible {
                    if is_meta {
                        // Not responsible for this metadata key.
                        response.tuples.push(KeyTuple {
                            key: key.clone(),
                            lattice_type: tuple.lattice_type,
                            error: AnnaError::WrongThread as i32,
                            ..Default::default()
                        });
                    } else {
                        // Unknown responsibility — stash as pending.
                        ctx.pending_requests
                            .entry(key.clone())
                            .or_default()
                            .push(PendingRequest {
                                r#type: request_type,
                                lattice_type: tuple.lattice_type,
                                payload: tuple.payload.clone(),
                                addr: response_address.clone(),
                                response_id: request.request_id.clone(),
                                expiry_epoch_ms: tuple.expiry_epoch_ms,
                            });
                    }
                } else {
                    // We are responsible — process the request.
                    let tp = process_tuple(ctx, key, tuple, request_type, threads);
                    response.tuples.push(tp);

                    ctx.key_access_tracker
                        .entry(key.clone())
                        .or_default()
                        .insert(Instant::now());
                    ctx.access_count += 1;
                }
            }
            ResponsibleResult::NeedReplicationFactor(_) => {
                // Issue a replication factor request so the pending
                // request can be drained when the response arrives.
                if let Some((target, rep_key)) = replication_request_target(
                    key,
                    &ctx.global_hash_rings,
                    &ctx.local_hash_rings,
                    ctx.metadata_replication_factor,
                    ctx.default_local_replication,
                ) {
                    let rep_req = KeyRequest {
                        request_id: format!("rep_{}_{}", ctx.rid, key),
                        response_address: ctx.wt.replication_response_connect_address(),
                        r#type: RequestType::Get as i32,
                        tuples: vec![KeyTuple {
                            key: rep_key,
                            lattice_type: LatticeType::Lww as i32,
                            ..Default::default()
                        }],
                        ..Default::default()
                    };
                    ctx.rid += 1;
                    outgoing.push((
                        target.key_request_connect_address(),
                        rep_req.encode_to_vec(),
                    ));
                }

                ctx.pending_requests
                    .entry(key.clone())
                    .or_default()
                    .push(PendingRequest {
                        r#type: request_type,
                        lattice_type: tuple.lattice_type,
                        payload: tuple.payload.clone(),
                        addr: response_address.clone(),
                        response_id: request.request_id.clone(),
                        expiry_epoch_ms: tuple.expiry_epoch_ms,
                    });
            }
        }
    }

    if !response.tuples.is_empty() && !response_address.is_empty() {
        outgoing.push((response_address, response.encode_to_vec()));
    }
    outgoing
}

/// Process a single key tuple (GET or PUT).
fn process_tuple(
    ctx: &mut KvsContext,
    key: &str,
    tuple: &KeyTuple,
    request_type: i32,
    threads: &[anna_server_common::threads::ServerThread],
) -> KeyTuple {
    let mut tp = KeyTuple {
        key: key.to_string(),
        ..Default::default()
    };

    if request_type == RequestType::Get as i32 {
        // GET
        match ctx.stored_key_map.get(key) {
            None => {
                tp.error = AnnaError::KeyDne as i32;
            }
            Some(kp) if kp.lattice_type() == LatticeType::None => {
                tp.error = AnnaError::KeyDne as i32;
            }
            Some(kp) if kp.expiry_epoch_s > 0 && now_epoch_s() >= kp.expiry_epoch_s => {
                // Expired (TTL or tombstone past GC threshold).
                tp.error = AnnaError::KeyDne as i32;
            }
            Some(kp) if kp.size() == 0 => {
                // Tombstone (deleted key, not yet GC'd).
                tp.error = AnnaError::KeyDne as i32;
            }
            Some(kp) => {
                let lt = kp.lattice_type();
                if let Some(serializer) = ctx.serializers.get(&(lt as i32)) {
                    let (payload, err) = process_get(key, serializer.as_ref());
                    tp.lattice_type = lt as i32;
                    tp.payload = payload;
                    tp.error = err;
                } else {
                    tp.error = AnnaError::KeyDne as i32;
                }
            }
        }
    } else if request_type == RequestType::Put as i32 {
        // PUT
        let lt = LatticeType::try_from(tuple.lattice_type).unwrap_or(LatticeType::None);
        if lt == LatticeType::None {
            log::error!("PUT request missing lattice type for key {}", key);
        } else if let Some(kp) = ctx.stored_key_map.get(key) {
            let stored_lt = kp.lattice_type();
            if stored_lt != LatticeType::None && stored_lt != lt {
                log::error!(
                    "Lattice type mismatch for {}: query {:?} but stored {:?}",
                    key,
                    lt,
                    stored_lt
                );
            } else {
                do_put(ctx, key, lt, &tuple.payload, tuple.expiry_epoch_ms);
                tp.lattice_type = lt as i32;
            }
        } else {
            do_put(ctx, key, lt, &tuple.payload, tuple.expiry_epoch_ms);
            tp.lattice_type = lt as i32;
        }
        ctx.local_changeset.insert(key.to_string());
    }

    // Signal cache invalidation if the client's address cache is stale.
    if tuple.address_cache_size > 0 && tuple.address_cache_size != threads.len() as u32 {
        tp.invalidate = true;
    }

    tp
}

fn do_put(ctx: &mut KvsContext, key: &str, lt: LatticeType, payload: &[u8], expiry: u64) {
    if let Some(serializer) = ctx.serializers.get_mut(&(lt as i32)) {
        process_put(
            key,
            lt,
            payload,
            serializer.as_mut(),
            &mut ctx.stored_key_map,
            expiry,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::LwwSerializer;
    use anna_server_common::metadata::{KeyReplication, Tier};
    use anna_server_common::proto::kvs::LwwValue;

    fn ctx_with_lww_and_key(key: &str, value: &[u8]) -> KvsContext {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));

        // Add replication factor.
        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert(key.to_string(), kr);

        // PUT the initial value.
        let lww = LwwValue {
            timestamp: 1,
            value: value.to_vec(),
        };
        let payload = lww.encode_to_vec();
        if let Some(s) = ctx.serializers.get_mut(&(LatticeType::Lww as i32)) {
            process_put(
                key,
                LatticeType::Lww,
                &payload,
                s.as_mut(),
                &mut ctx.stored_key_map,
                0,
            );
        }

        ctx
    }

    fn make_get_request(key: &str) -> Vec<u8> {
        KeyRequest {
            request_id: "req_1".into(),
            response_address: "tcp://127.0.0.1:6600".into(),
            r#type: RequestType::Get as i32,
            tuples: vec![KeyTuple {
                key: key.into(),
                ..Default::default()
            }],
            ..Default::default()
        }
        .encode_to_vec()
    }

    fn make_put_request(key: &str, ts: u64, value: &[u8]) -> Vec<u8> {
        let lww = LwwValue {
            timestamp: ts,
            value: value.to_vec(),
        };
        KeyRequest {
            request_id: "req_2".into(),
            response_address: "tcp://127.0.0.1:6600".into(),
            r#type: RequestType::Put as i32,
            tuples: vec![KeyTuple {
                key: key.into(),
                lattice_type: LatticeType::Lww as i32,
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        }
        .encode_to_vec()
    }

    #[test]
    fn get_existing_key() {
        let mut ctx = ctx_with_lww_and_key("test_key", b"hello");
        let data = make_get_request("test_key");
        let msgs = handle(&mut ctx, &data);

        assert_eq!(msgs.len(), 1);
        let response = KeyResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.tuples.len(), 1);
        assert_eq!(response.tuples[0].error, AnnaError::NoError as i32);
        assert!(!response.tuples[0].payload.is_empty());
    }

    #[test]
    fn get_missing_key_returns_key_dne() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));
        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("missing".into(), kr);

        let data = make_get_request("missing");
        let msgs = handle(&mut ctx, &data);

        let response = KeyResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.tuples[0].error, AnnaError::KeyDne as i32);
    }

    #[test]
    fn put_creates_key() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));
        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("new_key".into(), kr);

        let data = make_put_request("new_key", 1, b"world");
        let msgs = handle(&mut ctx, &data);

        assert_eq!(msgs.len(), 1);
        assert!(ctx.stored_key_map.contains_key("new_key"));
        assert!(ctx.local_changeset.contains("new_key"));
    }

    #[test]
    fn request_without_rep_factor_goes_pending() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));
        // No replication factor for "pending_key".

        let data = make_get_request("pending_key");
        let msgs = handle(&mut ctx, &data);

        // Request is pending, and a replication factor request is sent.
        assert!(ctx.pending_requests.contains_key("pending_key"));
        // The outgoing messages include the rep factor request (no client response).
        assert!(
            msgs.iter().all(|(addr, _)| !addr.contains("6600")),
            "Should not send client response, only rep factor request"
        );
    }
}
