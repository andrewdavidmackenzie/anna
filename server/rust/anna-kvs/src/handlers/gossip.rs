//! Gossip handler — receives gossip from other nodes, applies or forwards.
//!
//! Mirrors `server/cpp/src/kvs/gossip_handler.cpp`.

use anna_server_common::metadata::is_metadata;
use anna_server_common::proto::kvs::{KeyRequest, KeyTuple, LatticeType, RequestType};
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage, PendingGossip};
use crate::handlers::utils::process_put;

/// Handle incoming gossip — apply locally or forward to responsible threads.
pub(crate) fn handle(ctx: &mut KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let gossip = match KeyRequest::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("gossip: decode failed: {}", e);
            return vec![];
        }
    };

    // Collect forwarded gossip by target address.
    let mut forward_map: std::collections::HashMap<String, KeyRequest> =
        std::collections::HashMap::new();

    for tuple in &gossip.tuples {
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

        match result {
            ResponsibleResult::Ok(threads) => {
                if threads.iter().any(|t| *t == ctx.wt) {
                    // This worker is responsible — apply the gossip.
                    apply_gossip(ctx, tuple);
                } else if is_meta {
                    // Forward metadata gossip to responsible threads.
                    for thread in &threads {
                        let addr = thread.gossip_connect_address();
                        let req = forward_map.entry(addr).or_insert_with(|| KeyRequest {
                            r#type: RequestType::Put as i32,
                            ..Default::default()
                        });
                        req.tuples.push(tuple.clone());
                    }
                } else {
                    // Non-metadata key, not responsible — stash for later.
                    ctx.pending_gossip
                        .entry(key.clone())
                        .or_default()
                        .push(PendingGossip {
                            lattice_type: tuple.lattice_type,
                            payload: tuple.payload.clone(),
                            expiry_epoch_ms: tuple.expiry_epoch_ms,
                        });
                }
            }
            ResponsibleResult::NeedReplicationFactor(_) => {
                // Replication factor unknown — stash for later.
                ctx.pending_gossip
                    .entry(key.clone())
                    .or_default()
                    .push(PendingGossip {
                        lattice_type: tuple.lattice_type,
                        payload: tuple.payload.clone(),
                        expiry_epoch_ms: tuple.expiry_epoch_ms,
                    });
            }
        }
    }

    // Build outgoing messages for forwarded gossip.
    let mut outgoing = Vec::new();
    for (addr, req) in forward_map {
        outgoing.push((addr, req.encode_to_vec()));
    }
    outgoing
}

/// Apply a gossip tuple to local storage.
fn apply_gossip(ctx: &mut KvsContext, tuple: &KeyTuple) {
    let key = &tuple.key;

    // Check for lattice type mismatch.
    if let Some(kp) = ctx.stored_key_map.get(key.as_str()) {
        let stored_type = kp.lattice_type();
        let gossip_type = LatticeType::try_from(tuple.lattice_type).unwrap_or(LatticeType::None);
        if stored_type != gossip_type {
            log::error!(
                "Lattice type mismatch for {}: stored {:?} but gossip {:?}",
                key,
                stored_type,
                gossip_type
            );
            return;
        }
    }

    let lt = LatticeType::try_from(tuple.lattice_type).unwrap_or(LatticeType::Lww);
    if let Some(serializer) = ctx.serializers.get_mut(&(lt as i32)) {
        process_put(
            key,
            lt,
            &tuple.payload,
            serializer.as_mut(),
            &mut ctx.stored_key_map,
            tuple.expiry_epoch_ms,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::LwwSerializer;
    use anna_server_common::proto::kvs::LwwValue;

    fn ctx_with_lww() -> KvsContext {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));
        ctx
    }

    #[test]
    fn applies_gossip_to_responsible_thread() {
        use anna_server_common::metadata::{KeyReplication, Tier};
        let mut ctx = ctx_with_lww();
        // Add replication factor so routing succeeds.
        let mut kr = KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("gossip_key".into(), kr);

        let lww = LwwValue {
            timestamp: 100,
            value: b"gossip_val".to_vec(),
        };
        let gossip = KeyRequest {
            r#type: RequestType::Put as i32,
            tuples: vec![KeyTuple {
                key: "gossip_key".into(),
                lattice_type: LatticeType::Lww as i32,
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let msgs = handle(&mut ctx, &gossip.encode_to_vec());
        // No forwarding — we're the only node.
        assert!(msgs.is_empty());
        // Key should be stored.
        assert!(ctx.stored_key_map.contains_key("gossip_key"));
    }

    #[test]
    fn stashes_gossip_when_rep_factor_unknown() {
        let mut ctx = ctx_with_lww();
        // Don't add key to replication map — will get NeedReplicationFactor.

        let gossip = KeyRequest {
            r#type: RequestType::Put as i32,
            tuples: vec![KeyTuple {
                key: "unknown_rep_key".into(),
                lattice_type: LatticeType::Lww as i32,
                payload: vec![1, 2, 3],
                ..Default::default()
            }],
            ..Default::default()
        };

        let _ = handle(&mut ctx, &gossip.encode_to_vec());

        // Since replication factor is unknown for non-metadata keys,
        // the gossip should be pending.
        assert!(ctx.pending_gossip.contains_key("unknown_rep_key"));
    }
}
