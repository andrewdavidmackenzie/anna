//! Scan handler — cursor-based key listing with prefix filtering.
//!
//! Mirrors `server/cpp/src/kvs/scan_handler.cpp`.

use anna_server_common::metadata::is_metadata;
use anna_server_common::proto::kvs::{KeyRequest, KeyResponse, RequestType, ScanEntry};
use prost::Message;

use crate::context::{KvsContext, OutgoingMessage};

const DEFAULT_SCAN_COUNT: u32 = 100;
const MAX_SCAN_COUNT: u32 = 10000;

/// Handle a SCAN request.
///
/// Returns keys matching a prefix, starting from a numeric cursor position,
/// up to `scan_count` entries. Metadata keys are excluded from results.
pub fn handle(ctx: &KvsContext, data: &[u8]) -> Vec<OutgoingMessage> {
    let request = match KeyRequest::decode(data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("scan: failed to decode request: {}", e);
            return vec![];
        }
    };

    let prefix = &request.scan_prefix;
    let cursor = request.scan_cursor;
    let mut count = request.scan_count;
    if count == 0 {
        count = DEFAULT_SCAN_COUNT;
    }
    if count > MAX_SCAN_COUNT {
        count = MAX_SCAN_COUNT;
    }

    let mut response = KeyResponse {
        response_id: request.request_id.clone(),
        r#type: RequestType::Scan as i32,
        scan_total_keys: ctx
            .stored_key_map
            .keys()
            .filter(|k| !is_metadata(k))
            .count() as u64,
        ..Default::default()
    };

    // Sort keys for deterministic pagination across requests.
    // HashMap iteration order is unstable — without sorting, rehashes
    // between pages would silently skip or repeat keys.
    let mut ordered: Vec<&String> = ctx.stored_key_map.keys().collect();
    ordered.sort_unstable();

    let mut index: u64 = 0;
    let mut collected: u32 = 0;
    let mut next_cursor: u64 = 0;

    for key in ordered {
        let kp = &ctx.stored_key_map[key];
        if index < cursor {
            index += 1;
            continue;
        }

        // Skip metadata keys.
        if is_metadata(key) {
            index += 1;
            continue;
        }

        // Prefix filter.
        if !prefix.is_empty() && !key.starts_with(prefix.as_str()) {
            index += 1;
            continue;
        }

        response.scan_keys.push(ScanEntry {
            key: key.clone(),
            lattice_type: kp.lattice_type() as i32,
            size: kp.size(),
            expiry_epoch_s: kp.expiry_epoch_s,
        });
        collected += 1;
        index += 1;

        if collected >= count {
            next_cursor = index;
            break;
        }
    }

    response.scan_next_cursor = next_cursor;

    let response_addr = request.response_address.clone();
    if response_addr.is_empty() {
        return vec![];
    }

    vec![(response_addr, response.encode_to_vec())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use anna_server_common::metadata::KeyProperty;
    use anna_server_common::proto::kvs::LatticeType;

    fn make_scan_ctx() -> KvsContext {
        let mut ctx = crate::context::test_support::make_test_ctx();
        for i in 0..5 {
            let mut kp = KeyProperty::default();
            kp.set_size(10);
            kp.set_type(LatticeType::Lww);
            ctx.stored_key_map.insert(format!("user_key_{}", i), kp);
        }
        // Add a metadata key that should be excluded.
        let mut mkp = KeyProperty::default();
        mkp.set_size(5);
        mkp.set_type(LatticeType::Lww);
        ctx.stored_key_map
            .insert("ANNA_METADATA|stats|x".to_string(), mkp);
        ctx
    }

    fn make_scan_request(prefix: &str, cursor: u64, count: u32) -> Vec<u8> {
        let request = KeyRequest {
            request_id: "scan_1".to_string(),
            response_address: "tcp://127.0.0.1:6600".to_string(),
            r#type: RequestType::Scan as i32,
            scan_prefix: prefix.to_string(),
            scan_cursor: cursor,
            scan_count: count,
            ..Default::default()
        };
        request.encode_to_vec()
    }

    #[test]
    fn scan_returns_user_keys_only() {
        let ctx = make_scan_ctx();
        let data = make_scan_request("", 0, 100);
        let msgs = handle(&ctx, &data);
        assert_eq!(msgs.len(), 1);

        let response = KeyResponse::decode(msgs[0].1.as_slice()).unwrap();
        // 5 user keys, no metadata keys.
        assert_eq!(response.scan_keys.len(), 5);
    }

    #[test]
    fn scan_with_prefix_filter() {
        let ctx = make_scan_ctx();
        let data = make_scan_request("user_key_3", 0, 100);
        let msgs = handle(&ctx, &data);
        assert_eq!(msgs.len(), 1);

        let response = KeyResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.scan_keys.len(), 1);
        assert_eq!(response.scan_keys[0].key, "user_key_3");
    }

    #[test]
    fn scan_pagination() {
        let ctx = make_scan_ctx();
        // Request only 2 keys.
        let data = make_scan_request("", 0, 2);
        let msgs = handle(&ctx, &data);
        let response = KeyResponse::decode(msgs[0].1.as_slice()).unwrap();
        assert_eq!(response.scan_keys.len(), 2);
        assert!(response.scan_next_cursor > 0);
    }
}
