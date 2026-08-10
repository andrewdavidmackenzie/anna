//! Replication response handler — processes replication factor lookups
//! and drains pending requests/gossip.
//!
//! Mirrors `server/cpp/src/kvs/replication_response_handler.cpp`.

use std::time::Instant;

use anna_server_common::metadata::{get_key_from_metadata, init_replication, is_metadata, Tier};
use anna_server_common::proto::kvs::{
    AnnaError, KeyRequest, KeyResponse, KeyTuple, LatticeType, LwwValue, RequestType,
};
use anna_server_common::proto::metadata::ReplicationFactor;
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage};
use crate::handlers::utils::{process_get, process_put};

fn tier_from_i32(v: i32) -> Option<Tier> {
    match v {
        1 => Some(Tier::Memory),
        2 => Some(Tier::Disk),
        3 => Some(Tier::Routing),
        _ => None,
    }
}

/// Handle a replication factor response — update the replication map
/// and drain any pending requests/gossip for the resolved key.
pub(crate) fn handle(ctx: &mut KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
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
        None => {
            log::warn!("replication_response: bad metadata key: {}", tuple.key);
            return vec![];
        }
    };

    let error = tuple.error;

    if error == AnnaError::NoError as i32 {
        // Parse the replication factor from the LWW payload.
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
        // No replication data stored — use defaults.
        init_replication(
            &mut ctx.key_replication_map,
            &key,
            &ctx.tier_metadata,
            ctx.default_local_replication,
        );
    } else if error == AnnaError::WrongThread as i32 {
        // The node we queried wasn't responsible — will retry on next request.
        return vec![];
    } else {
        log::error!("Unexpected error {} in replication response", error);
        return vec![];
    }

    let mut outgoing = Vec::new();

    // Drain pending requests for this key.
    if let Some(pending) = ctx.pending_requests.remove(&key) {
        let is_meta = is_metadata(&key);
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
            let responsible = threads.iter().any(|t| *t == ctx.wt);

            for req in &pending {
                if !responsible && !req.addr.is_empty() {
                    // Not responsible — send WRONG_THREAD back to client.
                    let resp = KeyResponse {
                        r#type: req.r#type,
                        response_id: req.response_id.clone(),
                        tuples: vec![KeyTuple {
                            key: key.clone(),
                            error: AnnaError::WrongThread as i32,
                            ..Default::default()
                        }],
                        ..Default::default()
                    };
                    outgoing.push((req.addr.clone(), resp.encode_to_vec()));
                } else if responsible {
                    // Process the pending request.
                    let tp = process_pending_request(ctx, &key, req);
                    ctx.key_access_tracker
                        .entry(key.clone())
                        .or_default()
                        .insert(Instant::now());
                    ctx.access_count += 1;

                    if !req.addr.is_empty() {
                        let resp = KeyResponse {
                            r#type: req.r#type,
                            response_id: req.response_id.clone(),
                            tuples: vec![tp],
                            ..Default::default()
                        };
                        outgoing.push((req.addr.clone(), resp.encode_to_vec()));
                    }
                }
            }
        }
    }

    // Drain pending gossip for this key.
    if let Some(pending) = ctx.pending_gossip.remove(&key) {
        let is_meta = is_metadata(&key);
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
            if threads.iter().any(|t| *t == ctx.wt) {
                // Responsible — apply the gossip.
                for gossip in &pending {
                    let lt = LatticeType::try_from(gossip.lattice_type).unwrap_or(LatticeType::Lww);
                    if let Some(serializer) = ctx.serializers.get_mut(&(lt as i32)) {
                        process_put(
                            &key,
                            lt,
                            &gossip.payload,
                            serializer.as_mut(),
                            &mut ctx.stored_key_map,
                            gossip.expiry_epoch_ms,
                        );
                    }
                }
            } else {
                // Not responsible — forward the gossip.
                for thread in &threads {
                    let mut req = KeyRequest {
                        r#type: RequestType::Put as i32,
                        ..Default::default()
                    };
                    for gossip in &pending {
                        req.tuples.push(KeyTuple {
                            key: key.clone(),
                            lattice_type: gossip.lattice_type,
                            payload: gossip.payload.clone(),
                            expiry_epoch_ms: gossip.expiry_epoch_ms,
                            ..Default::default()
                        });
                    }
                    outgoing.push((thread.gossip_connect_address(), req.encode_to_vec()));
                }
            }
        }
    }

    outgoing
}

/// Process a single pending request (GET or PUT).
fn process_pending_request(
    ctx: &mut KvsContext,
    key: &str,
    req: &crate::context::PendingRequest,
) -> KeyTuple {
    let mut tp = KeyTuple {
        key: key.to_string(),
        ..Default::default()
    };

    if req.r#type == RequestType::Get as i32 {
        match ctx.stored_key_map.get(key) {
            None => {
                tp.error = AnnaError::KeyDne as i32;
            }
            Some(kp) if kp.lattice_type() == LatticeType::None => {
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
    } else if req.r#type == RequestType::Put as i32 {
        let lt = LatticeType::try_from(req.lattice_type).unwrap_or(LatticeType::None);
        if lt != LatticeType::None {
            if let Some(serializer) = ctx.serializers.get_mut(&(lt as i32)) {
                process_put(
                    key,
                    lt,
                    &req.payload,
                    serializer.as_mut(),
                    &mut ctx.stored_key_map,
                    req.expiry_epoch_ms,
                );
                tp.lattice_type = lt as i32;
                ctx.local_changeset.insert(key.to_string());
            }
        }
    }

    tp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PendingRequest;
    use crate::storage::memory::LwwSerializer;
    use anna_server_common::metadata::KeyReplication;
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
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));

        // Add a pending PUT request.
        let lww = LwwValue {
            timestamp: 1,
            value: b"pending_val".to_vec(),
        };
        ctx.pending_requests.insert(
            "pending_key".into(),
            vec![PendingRequest {
                r#type: RequestType::Put as i32,
                lattice_type: LatticeType::Lww as i32,
                payload: lww.encode_to_vec(),
                addr: String::new(), // no response needed
                response_id: String::new(),
                expiry_epoch_ms: 0,
            }],
        );

        let data = make_rep_response("pending_key", 1);
        let _ = handle(&mut ctx, &data);

        // Pending request should be drained.
        assert!(!ctx.pending_requests.contains_key("pending_key"));
        // Key should be stored.
        assert!(ctx.stored_key_map.contains_key("pending_key"));
    }

    #[test]
    fn key_dne_uses_default_replication() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.tier_metadata.insert(
            Tier::Memory,
            anna_server_common::metadata::TierMetadata {
                id: Tier::Memory,
                thread_number: 1,
                default_replication: 3,
                node_capacity: 1024,
            },
        );

        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: "ANNA_METADATA|replication|default_key".into(),
                error: AnnaError::KeyDne as i32,
                ..Default::default()
            }],
            ..Default::default()
        };

        let _ = handle(&mut ctx, &response.encode_to_vec());

        assert_eq!(
            ctx.key_replication_map["default_key"].global_replication[&Tier::Memory],
            3
        );
    }
}
