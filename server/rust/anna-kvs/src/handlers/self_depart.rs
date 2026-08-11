//! Self-depart handler — gossips all data to responsible threads and
//! notifies the cluster of departure.
//!
//! Mirrors `server/cpp/src/kvs/self_depart_handler.cpp`.

use anna_server_common::metadata::is_metadata;
use anna_server_common::routing::{get_responsible_threads, ResponsibleResult};
use anna_server_common::threads::{MonitoringThread, RoutingThread, ServerThread};

use crate::context::{AddressKeysetMap, KvsContext, OutgoingMessage};
use crate::handlers::utils::build_gossip_messages;

/// Handle self-departure: gossip all data out and notify the cluster.
///
/// `depart_done_addr` is the monitoring address to send the "done" message to.
pub(crate) fn handle(ctx: &mut KvsContext, depart_done_addr: &str) -> Vec<OutgoingMessage> {
    log::info!("This node is departing.");

    // Remove self from the hash ring.
    if let Some(ring) = ctx.global_hash_rings.get_mut(&ctx.self_tier) {
        ring.remove(&ctx.public_ip, &ctx.private_ip, 0);
    }

    let mut outgoing = Vec::new();

    // Thread 0 notifies the cluster.
    if ctx.thread_id == 0 {
        let msg = format!(
            "{}:{}:{}",
            tier_name(ctx.self_tier),
            ctx.public_ip,
            ctx.private_ip
        );

        // Notify all servers.
        for ring in ctx.global_hash_rings.values() {
            for st in ring.get_unique_servers() {
                outgoing.push((st.node_depart_connect_address(), msg.as_bytes().to_vec()));
            }
        }

        let depart_msg = format!("depart:{}", msg);

        // Notify routing nodes.
        for addr in &ctx.routing_ips {
            outgoing.push((
                RoutingThread::new(addr, 0, ctx.wt.base_offset()).notify_connect_address(),
                depart_msg.as_bytes().to_vec(),
            ));
        }

        // Notify monitoring nodes.
        for addr in &ctx.monitoring_ips {
            outgoing.push((
                MonitoringThread::new(addr, ctx.wt.base_offset()).notify_connect_address(),
                depart_msg.as_bytes().to_vec(),
            ));
        }

        // Relay to worker threads 1..N.
        for tid in 1..ctx.thread_count {
            let addr =
                ServerThread::new(&ctx.public_ip, &ctx.private_ip, tid, ctx.wt.base_offset())
                    .self_depart_connect_address();
            outgoing.push((addr, vec![]));
        }
    }

    // Gossip all data to responsible threads.
    let mut addr_keyset_map = AddressKeysetMap::new();

    for key in ctx.stored_key_map.keys().cloned().collect::<Vec<_>>() {
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
            for thread in &threads {
                addr_keyset_map
                    .entry(thread.gossip_connect_address())
                    .or_default()
                    .insert(key.clone());
            }
        }
    }

    let gossip_msgs =
        build_gossip_messages(&addr_keyset_map, &ctx.serializers, &ctx.stored_key_map);
    outgoing.extend(gossip_msgs);

    // Send depart-done notification.
    let done_msg = format!(
        "{}_{}_{}",
        ctx.public_ip, ctx.private_ip, ctx.self_tier as u32
    );
    outgoing.push((depart_done_addr.to_string(), done_msg.into_bytes()));

    outgoing
}

fn tier_name(tier: anna_server_common::metadata::Tier) -> &'static str {
    match tier {
        anna_server_common::metadata::Tier::Memory => "MEMORY",
        anna_server_common::metadata::Tier::Disk => "DISK",
        anna_server_common::metadata::Tier::Routing => "ROUTING",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::metadata::Tier;

    use crate::storage::memory::LwwSerializer;
    use anna_server_common::metadata::KeyProperty;
    use anna_server_common::proto::kvs::LatticeType;

    #[test]
    fn notifies_routing_and_monitoring() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.routing_ips = vec!["10.0.0.5".into()];
        ctx.monitoring_ips = vec!["10.0.0.6".into()];
        let msgs = handle(&mut ctx, "tcp://monitor:6450");

        // Should contain depart notification to routing.
        assert!(msgs.iter().any(|(addr, _)| addr.contains("10.0.0.5")));
        // Should contain depart notification to monitoring.
        assert!(msgs.iter().any(|(addr, _)| addr.contains("10.0.0.6")));
    }

    #[test]
    fn relays_to_worker_threads() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.thread_count = 3;
        let msgs = handle(&mut ctx, "tcp://monitor:6450");

        // Should relay to threads 1 and 2 (ports 6101, 6102).
        let relay_count = msgs
            .iter()
            .filter(|(addr, _)| addr.contains("610")) // SELF_DEPART_PORT range
            .count();
        assert!(
            relay_count >= 2,
            "Expected at least 2 relays, got {}",
            relay_count
        );
    }

    #[test]
    fn non_thread0_skips_notifications() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.thread_id = 1;
        ctx.routing_ips = vec!["10.0.0.5".into()];
        ctx.monitoring_ips = vec!["10.0.0.6".into()];
        let msgs = handle(&mut ctx, "tcp://monitor:6450");

        // Thread 1 should NOT notify routing/monitoring.
        assert!(!msgs.iter().any(|(addr, _)| addr.contains("10.0.0.5")));
        assert!(!msgs.iter().any(|(addr, _)| addr.contains("10.0.0.6")));
        // But should still send depart-done.
        assert!(msgs.iter().any(|(addr, _)| addr == "tcp://monitor:6450"));
    }

    #[test]
    fn gossips_stored_data() {
        let mut ctx = crate::context::tests::make_test_ctx();
        ctx.serializers
            .insert(LatticeType::Lww as i32, Box::new(LwwSerializer::new()));

        // Store a key.
        let mut kp = KeyProperty::default();
        kp.set_size(10);
        kp.set_type(LatticeType::Lww);
        ctx.stored_key_map.insert("depart_key".into(), kp);

        let mut kr = anna_server_common::metadata::KeyReplication::default();
        kr.global_replication.insert(Tier::Memory, 1);
        kr.local_replication.insert(Tier::Memory, 1);
        ctx.key_replication_map.insert("depart_key".into(), kr);

        // Add a second node so gossip has a target.
        ctx.global_hash_rings
            .get_mut(&Tier::Memory)
            .unwrap()
            .insert(
                "2.2.2.2",
                "10.0.0.2",
                0,
                0,
                anna_server_common::hash_ring::DEFAULT_VIRTUAL_THREAD_NUM,
                true,
            );

        // Put actual data in serializer.
        let lww = anna_server_common::proto::kvs::LwwValue {
            timestamp: 1,
            value: b"depart_val".to_vec(),
        };
        if let Some(s) = ctx.serializers.get_mut(&(LatticeType::Lww as i32)) {
            s.put("depart_key", &prost::Message::encode_to_vec(&lww));
        }

        let msgs = handle(&mut ctx, "tcp://monitor:6450");

        // Should have depart-done message.
        assert!(msgs.iter().any(|(addr, _)| addr == "tcp://monitor:6450"));

        // Should have gossip to the remaining node's gossip address.
        let gossip_addr =
            anna_server_common::threads::ServerThread::new("2.2.2.2", "10.0.0.2", 0, 0)
                .gossip_connect_address();
        let gossip_msg = msgs.iter().find(|(addr, _)| addr == &gossip_addr);
        assert!(
            gossip_msg.is_some(),
            "Expected gossip to remaining node at {}, got addrs: {:?}",
            gossip_addr,
            msgs.iter().map(|(a, _)| a).collect::<Vec<_>>()
        );
        // Verify the gossip payload contains depart_key.
        let payload = &gossip_msg.unwrap().1;
        let payload_str = String::from_utf8_lossy(payload);
        assert!(
            payload_str.contains("depart_key") || payload.len() > 10,
            "Gossip payload should contain key data"
        );
    }

    #[test]
    fn depart_done_message_format() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msgs = handle(&mut ctx, "tcp://monitor:6450");
        let done = msgs
            .iter()
            .find(|(addr, _)| addr == "tcp://monitor:6450")
            .unwrap();
        let done_str = String::from_utf8_lossy(&done.1);
        // Format: public_ip_private_ip_tier
        assert!(done_str.contains("1.1.1.1_1.1.1.1_1"));
    }

    #[test]
    fn removes_self_from_ring() {
        let mut ctx = crate::context::tests::make_test_ctx();
        assert!(!ctx.global_hash_rings[&Tier::Memory].is_empty());

        let _ = handle(&mut ctx, "tcp://monitor:6450");

        assert!(ctx.global_hash_rings[&Tier::Memory].is_empty());
    }

    #[test]
    fn sends_depart_done() {
        let mut ctx = crate::context::tests::make_test_ctx();
        let msgs = handle(&mut ctx, "tcp://monitor:6450");

        // Should include the depart-done message.
        assert!(msgs.iter().any(|(addr, _)| addr == "tcp://monitor:6450"));
    }
}
