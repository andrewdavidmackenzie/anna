//! Replication change handler — updates cached replication factors.
//!
//! Mirrors `server/cpp/src/route/replication_change_handler.cpp`.

use anna_server_common::metadata::Tier;
use anna_server_common::proto::metadata::ReplicationFactorUpdate;
use anna_server_common::threads::RoutingThread;
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

/// Handle a replication factor change notification.
pub(crate) fn handle(ctx: &mut RouteContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let mut outgoing = Vec::new();

    // Thread 0 relays to sibling routing threads.
    if ctx.thread_id == 0 {
        for tid in 1..ctx.thread_count {
            let rt = RoutingThread::new(&ctx.ip, tid, ctx.rt.base_offset());
            outgoing.push((rt.replication_change_connect_address(), data.to_vec()));
        }
    }

    let update = match ReplicationFactorUpdate::decode(data) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("replication_change: decode failed: {}", e);
            return outgoing;
        }
    };

    for key_rep in &update.updates {
        let kr = ctx
            .key_replication_map
            .entry(key_rep.key.clone())
            .or_default();
        for global in &key_rep.global {
            if let Some(t) = tier_from_i32(global.tier) {
                kr.global_replication.insert(t, global.value);
            }
        }
        for local in &key_rep.local {
            if let Some(t) = tier_from_i32(local.tier) {
                kr.local_replication.insert(t, local.value);
            }
        }
    }

    outgoing
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::proto::metadata::{
        replication_factor::ReplicationValue, ReplicationFactor,
    };

    #[test]
    fn updates_replication_factor() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "rep_key".into(),
                global: vec![ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 2,
                }],
                local: vec![],
            }],
        };
        let _ = handle(&mut ctx, &update.encode_to_vec());
        assert_eq!(
            ctx.key_replication_map["rep_key"].global_replication[&Tier::Memory],
            2
        );
    }

    #[test]
    fn thread0_relays() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.thread_id = 0;
        ctx.thread_count = 2;
        let update = ReplicationFactorUpdate {
            updates: vec![ReplicationFactor {
                key: "k".into(),
                global: vec![],
                local: vec![],
            }],
        };
        let msgs = handle(&mut ctx, &update.encode_to_vec());
        assert!(!msgs.is_empty());
    }
}
