//! Actor thread — processes requests sequentially with its own storage.
//!
//! Each actor owns a `KvsContext` and `SerializerMap`. Requests arrive
//! via a channel, are processed inline, and the response is sent back
//! through a oneshot-style channel.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use anna_kvs::handlers::utils::{
    gc_reap_expired_keys, generate_timestamp, now_epoch_s, process_get, process_put,
};
use anna_kvs::storage::{self, SerializerMap};
use anna_server_common::metadata::{is_metadata, KeyProperty};
use anna_server_common::proto::kvs::{AnnaError, LatticeType, LwwValue};
use anna_server_common::types::Key;
use prost::Message;

use crate::ScanEntry;

/// A request sent to an actor thread.
pub(crate) enum Request {
    Put {
        key: String,
        value: Vec<u8>,
        lattice_type: LatticeType,
        expiry_epoch_ms: u64,
    },
    Get {
        key: String,
    },
    Delete {
        key: String,
    },
    Scan {
        prefix: String,
    },
    Shutdown,
}

/// A response from an actor thread.
pub(crate) enum Response {
    Ok,
    Value(Vec<u8>),
    NotFound,
    ScanResult(Vec<ScanEntry>),
    Error(String),
}

/// An envelope wrapping a request and a reply channel.
struct Envelope {
    request: Request,
    reply: mpsc::Sender<Response>,
}

/// Handle to a running actor thread. Holds the send side of the channel.
pub(crate) struct ActorHandle {
    sender: mpsc::Sender<Envelope>,
}

impl ActorHandle {
    /// Spawn a new actor thread with the given thread ID.
    pub(crate) fn spawn(tid: u32) -> Self {
        let (tx, rx) = mpsc::channel::<Envelope>();

        thread::Builder::new()
            .name(format!("anna-actor-{}", tid))
            .spawn(move || {
                actor_loop(tid, rx);
            })
            .expect("failed to spawn actor thread");

        ActorHandle { sender: tx }
    }

    /// Send a request and wait for the response.
    pub(crate) fn send(&self, request: Request) -> crate::Result<Response> {
        let (reply_tx, reply_rx) = mpsc::channel();
        let envelope = Envelope {
            request,
            reply: reply_tx,
        };
        self.sender
            .send(envelope)
            .map_err(|_| crate::Error::ActorGone)?;
        reply_rx.recv().map_err(|_| crate::Error::ActorGone)
    }

    /// Signal the actor to shut down (best-effort, does not block).
    pub(crate) fn shutdown(&self) {
        let (reply_tx, _) = mpsc::channel();
        let _ = self.sender.send(Envelope {
            request: Request::Shutdown,
            reply: reply_tx,
        });
    }
}

/// Generate a strictly monotonic timestamp for this actor.
///
/// Combines `generate_timestamp(tid)` with a local sequence counter
/// to guarantee that every operation within an actor gets a unique,
/// strictly increasing timestamp — even when multiple operations
/// happen within the same millisecond.
fn monotonic_timestamp(tid: u32, seq: &mut u64) -> u64 {
    let wall = generate_timestamp(tid);
    *seq = (*seq).max(wall) + 1;
    *seq
}

/// The actor event loop — processes requests one at a time.
fn actor_loop(tid: u32, rx: mpsc::Receiver<Envelope>) {
    let mut serializers = storage::create_memory_serializers();
    let mut stored_key_map: HashMap<Key, KeyProperty> = HashMap::new();
    let mut ts_seq: u64 = 0;

    // Periodic GC tracking.
    let mut last_gc = std::time::Instant::now();
    let gc_interval = std::time::Duration::from_secs(30);

    log::debug!("Actor {} started", tid);

    while let Ok(envelope) = rx.recv() {
        let response = match envelope.request {
            Request::Put {
                key,
                value,
                lattice_type,
                expiry_epoch_ms,
            } => handle_put(
                &key,
                &value,
                lattice_type,
                expiry_epoch_ms,
                &mut serializers,
                &mut stored_key_map,
                monotonic_timestamp(tid, &mut ts_seq),
            ),
            Request::Get { key } => handle_get(&key, &serializers, &stored_key_map),
            Request::Delete { key } => handle_delete(
                &key,
                &mut serializers,
                &mut stored_key_map,
                monotonic_timestamp(tid, &mut ts_seq),
            ),
            Request::Scan { prefix } => handle_scan(&prefix, &serializers, &stored_key_map),
            Request::Shutdown => {
                log::debug!("Actor {} shutting down", tid);
                break;
            }
        };

        // Send response (ignore error if caller dropped the receiver).
        let _ = envelope.reply.send(response);

        // Periodic GC.
        if last_gc.elapsed() >= gc_interval {
            let reaped = gc_reap_expired_keys(&mut stored_key_map, &mut serializers);
            if reaped > 0 {
                log::debug!("Actor {}: GC reaped {} expired keys", tid, reaped);
            }
            last_gc = std::time::Instant::now();
        }
    }

    log::debug!("Actor {} stopped", tid);
}

/// Handle a PUT request: wrap the raw value in an LWW envelope and store it.
fn handle_put(
    key: &str,
    value: &[u8],
    lattice_type: LatticeType,
    expiry_epoch_ms: u64,
    serializers: &mut SerializerMap,
    stored_key_map: &mut HashMap<Key, KeyProperty>,
    timestamp: u64,
) -> Response {
    let payload = match lattice_type {
        LatticeType::Lww => {
            // Wrap raw bytes in an LWW envelope with a monotonic timestamp.
            let lww = LwwValue {
                timestamp,
                value: value.to_vec(),
            };
            lww.encode_to_vec()
        }
        _ => {
            // For other lattice types, the caller must provide the
            // correctly serialized protobuf payload.
            value.to_vec()
        }
    };

    let serializer = match serializers.get_mut(&(lattice_type as i32)) {
        Some(s) => s,
        None => return Response::Error(format!("unsupported lattice type: {:?}", lattice_type)),
    };

    process_put(
        key,
        lattice_type,
        &payload,
        serializer.as_mut(),
        stored_key_map,
        expiry_epoch_ms,
    );

    Response::Ok
}

/// Handle a GET request: retrieve and unwrap the LWW value.
fn handle_get(
    key: &str,
    serializers: &SerializerMap,
    stored_key_map: &HashMap<Key, KeyProperty>,
) -> Response {
    let kp = match stored_key_map.get(key) {
        Some(kp) => kp,
        None => return Response::NotFound,
    };

    // Check for tombstone or expired key.
    if kp.size() == 0 {
        return Response::NotFound;
    }
    if kp.expiry_epoch_s > 0 && now_epoch_s() >= kp.expiry_epoch_s {
        return Response::NotFound;
    }

    let lt = kp.lattice_type();
    let serializer = match serializers.get(&(lt as i32)) {
        Some(s) => s,
        None => return Response::NotFound,
    };

    let (payload, err) = process_get(key, serializer.as_ref());
    if err != AnnaError::NoError as i32 {
        return Response::NotFound;
    }

    // For LWW, unwrap the envelope to return just the raw value.
    match lt {
        LatticeType::Lww => match LwwValue::decode(payload.as_slice()) {
            Ok(lww) if lww.value.is_empty() => Response::NotFound,
            Ok(lww) => Response::Value(lww.value),
            Err(_) => Response::Value(payload),
        },
        _ => Response::Value(payload),
    }
}

/// Handle a DELETE request: write a tombstone (empty LWW value).
fn handle_delete(
    key: &str,
    serializers: &mut SerializerMap,
    stored_key_map: &mut HashMap<Key, KeyProperty>,
    timestamp: u64,
) -> Response {
    // A tombstone is an LWW value with an empty payload.
    let lww = LwwValue {
        timestamp,
        value: vec![],
    };
    let payload = lww.encode_to_vec();

    let serializer = match serializers.get_mut(&(LatticeType::Lww as i32)) {
        Some(s) => s,
        None => return Response::Error("LWW serializer not found".into()),
    };

    process_put(
        key,
        LatticeType::Lww,
        &payload,
        serializer.as_mut(),
        stored_key_map,
        0,
    );

    Response::Ok
}

/// Handle a SCAN request: list keys matching a prefix.
///
/// Uses the serializer's `get()` to detect tombstones (LWW with empty
/// value), since `KeyProperty::size()` tracks the protobuf encoding
/// size, not the logical value size.
fn handle_scan(
    prefix: &str,
    serializers: &SerializerMap,
    stored_key_map: &HashMap<Key, KeyProperty>,
) -> Response {
    let mut entries = Vec::new();
    for (k, kp) in stored_key_map {
        if is_metadata(k) {
            continue;
        }
        if kp.expiry_epoch_s > 0 && now_epoch_s() >= kp.expiry_epoch_s {
            continue;
        }
        if !prefix.is_empty() && !k.starts_with(prefix) {
            continue;
        }

        // Check via the serializer whether this is a live value or a tombstone.
        let lt = kp.lattice_type() as i32;
        if let Some(serializer) = serializers.get(&lt) {
            let (_, err) = process_get(k, serializer.as_ref());
            if err != AnnaError::NoError as i32 {
                continue; // Tombstone or missing — skip.
            }
        } else {
            continue;
        }

        entries.push(ScanEntry {
            key: k.clone(),
            size: kp.size(),
        });
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    Response::ScanResult(entries)
}
