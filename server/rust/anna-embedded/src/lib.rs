//! Embeddable Anna KVS — a lock-free, actor-based key-value store.
//!
//! `EmbeddedKvs` spawns N actor threads, each with independent storage.
//! Keys are partitioned across actors using consistent hashing (the same
//! algorithm as the distributed Anna KVS). Each actor processes requests
//! sequentially with no locks on the hot path.
//!
//! # Example
//!
//! ```rust
//! use anna_embedded::EmbeddedKvs;
//!
//! let kvs = EmbeddedKvs::new(4).unwrap(); // 4 actor threads
//! kvs.put("hello", b"world").unwrap();
//! let value = kvs.get("hello").unwrap();
//! assert_eq!(value, Some(b"world".to_vec()));
//! kvs.delete("hello").unwrap();
//! assert_eq!(kvs.get("hello").unwrap(), None);
//! ```

mod actor;

use actor::{ActorHandle, Request, Response};
use anna_server_common::hash_ring::{ConsistentHashRing, DEFAULT_VIRTUAL_THREAD_NUM};
use anna_server_common::proto::kvs::LatticeType;
use std::fmt;

/// Errors returned by the embedded KVS.
#[derive(Debug)]
pub enum Error {
    /// The number of actors must be at least 1.
    InvalidActorCount,
    /// An actor thread has shut down or panicked.
    ActorGone,
    /// Internal error from the storage layer.
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidActorCount => write!(f, "actor count must be >= 1"),
            Error::ActorGone => write!(f, "actor thread is no longer running"),
            Error::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

/// Result type for embedded KVS operations.
pub type Result<T> = std::result::Result<T, Error>;

/// An entry returned by [`EmbeddedKvs::scan`].
#[derive(Debug, Clone)]
pub struct ScanEntry {
    /// The key.
    pub key: String,
    /// Size in bytes of the stored value.
    pub size: u32,
}

/// An embeddable, lock-free, actor-based key-value store.
///
/// Each actor thread owns independent storage and processes requests
/// sequentially. Keys are partitioned across actors using consistent
/// hashing. The public API is `Send + Sync` — multiple threads can
/// call `put`/`get`/`delete` concurrently.
pub struct EmbeddedKvs {
    actors: Vec<ActorHandle>,
    local_ring: ConsistentHashRing,
}

// The struct holds only Senders (which are Send+Sync) and an immutable ring.
// SAFETY: ActorHandle contains only std::sync::mpsc::Sender (Send) and a u32.
// ConsistentHashRing is read-only after construction.
unsafe impl Send for EmbeddedKvs {}
unsafe impl Sync for EmbeddedKvs {}

impl EmbeddedKvs {
    /// Create a new embedded KVS with `num_actors` actor threads.
    ///
    /// Each actor gets its own independent storage. Keys are partitioned
    /// across actors using the same consistent hashing algorithm as the
    /// distributed Anna KVS.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidActorCount`] if `num_actors` is 0.
    pub fn new(num_actors: u32) -> Result<Self> {
        if num_actors == 0 {
            return Err(Error::InvalidActorCount);
        }

        // Build the local hash ring with all actor threads.
        let mut local_ring = ConsistentHashRing::new();
        for tid in 0..num_actors {
            // Use dummy IPs — embedded mode has no network identity.
            local_ring.insert(
                "embedded",
                "embedded",
                tid,
                0,
                DEFAULT_VIRTUAL_THREAD_NUM,
                false,
            );
        }

        // Spawn actor threads.
        let mut actors = Vec::with_capacity(num_actors as usize);
        for tid in 0..num_actors {
            actors.push(ActorHandle::spawn(tid));
        }

        Ok(EmbeddedKvs { actors, local_ring })
    }

    /// Store a value under a key (last-writer-wins semantics).
    ///
    /// If the key already exists, the new value replaces it (LWW merge
    /// by timestamp — the newer write always wins).
    pub fn put(&self, key: &str, value: &[u8]) -> Result<()> {
        self.put_with_lattice(key, value, LatticeType::Lww, 0)
    }

    /// Store a value with a time-to-live in seconds.
    ///
    /// The key will be automatically removed after `ttl_secs` seconds.
    pub fn put_with_ttl(&self, key: &str, value: &[u8], ttl_secs: u32) -> Result<()> {
        let expiry_epoch_ms =
            (anna_kvs::handlers::utils::now_epoch_s() as u64 + ttl_secs as u64) * 1000;
        self.put_with_lattice(key, value, LatticeType::Lww, expiry_epoch_ms)
    }

    /// Retrieve the value for a key.
    ///
    /// Returns `Ok(None)` if the key does not exist.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let actor = self.route(key);
        let resp = actor.send(Request::Get {
            key: key.to_string(),
        })?;
        match resp {
            Response::Value(data) => Ok(Some(data)),
            Response::NotFound => Ok(None),
            Response::Error(msg) => Err(Error::Internal(msg)),
            _ => Err(Error::Internal("unexpected response".into())),
        }
    }

    /// Delete a key (tombstone write).
    ///
    /// After deletion, `get` returns `None`. The tombstone is eventually
    /// garbage-collected.
    pub fn delete(&self, key: &str) -> Result<()> {
        let actor = self.route(key);
        let resp = actor.send(Request::Delete {
            key: key.to_string(),
        })?;
        match resp {
            Response::Ok | Response::NotFound => Ok(()),
            Response::Error(msg) => Err(Error::Internal(msg)),
            _ => Err(Error::Internal("unexpected response".into())),
        }
    }

    /// List keys matching an optional prefix.
    ///
    /// Returns entries from all actors, sorted by key. Pass an empty
    /// string to list all keys.
    pub fn scan(&self, prefix: &str) -> Result<Vec<ScanEntry>> {
        let mut all_entries = Vec::new();
        for actor in &self.actors {
            let resp = actor.send(Request::Scan {
                prefix: prefix.to_string(),
            })?;
            if let Response::ScanResult(entries) = resp {
                all_entries.extend(entries);
            }
        }
        all_entries.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(all_entries)
    }

    /// Return the number of actor threads.
    pub fn num_actors(&self) -> u32 {
        self.actors.len() as u32
    }

    /// Internal: put with explicit lattice type and expiry.
    fn put_with_lattice(
        &self,
        key: &str,
        value: &[u8],
        lattice_type: LatticeType,
        expiry_epoch_ms: u64,
    ) -> Result<()> {
        let actor = self.route(key);
        let resp = actor.send(Request::Put {
            key: key.to_string(),
            value: value.to_vec(),
            lattice_type,
            expiry_epoch_ms,
        })?;
        match resp {
            Response::Ok => Ok(()),
            Response::Error(msg) => Err(Error::Internal(msg)),
            _ => Err(Error::Internal("unexpected response".into())),
        }
    }

    /// Route a key to its responsible actor using the local hash ring.
    fn route(&self, key: &str) -> &ActorHandle {
        let tids = self.local_ring.find_responsible_local(key, 1);
        let tid = tids.first().copied().unwrap_or(0);
        &self.actors[tid as usize]
    }
}

impl Drop for EmbeddedKvs {
    fn drop(&mut self) {
        for actor in &self.actors {
            actor.shutdown();
        }
    }
}
