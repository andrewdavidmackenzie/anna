//! Consistent hash ring implementation.
//!
//! Mirrors `server/cpp/src/hash_ring/consistent_hash_map.hpp` and
//! `server/cpp/src/hash_ring/hash_ring.hpp`.

use crate::threads::ServerThread;
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Default number of virtual nodes per thread on the hash ring.
pub const DEFAULT_VIRTUAL_THREAD_NUM: u32 = 3000;

/// A consistent hash ring mapping hash values to server threads.
///
/// Uses a `BTreeMap` for sorted key lookup with `O(log n)` find.
#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    ring: BTreeMap<u64, ServerThread>,
}

impl ConsistentHashRing {
    pub fn new() -> Self {
        Self {
            ring: BTreeMap::new(),
        }
    }

    /// Insert a server thread with `virtual_num` virtual nodes.
    pub fn insert(
        &mut self,
        public_ip: &str,
        private_ip: &str,
        tid: u32,
        base_offset: u32,
        virtual_nodes: u32,
        global: bool,
    ) {
        for vn in 0..virtual_nodes {
            let st = ServerThread::with_virtual(public_ip, private_ip, tid, vn, base_offset);
            let hash = if global {
                global_hash_thread(&st)
            } else {
                local_hash_thread(&st)
            };
            self.ring.insert(hash, st);
        }
    }

    /// Remove all virtual nodes for a server identified by public_ip and private_ip.
    pub fn remove(&mut self, public_ip: &str, private_ip: &str, tid: u32) {
        self.ring.retain(|_, st| {
            !(st.public_ip() == public_ip && st.private_ip() == private_ip && st.tid() == tid)
        });
    }

    /// Find the server thread responsible for a key.
    /// Returns the first thread at or after the key's hash position.
    /// Wraps around to the beginning if needed.
    pub fn find(&self, key: &str, global: bool) -> Option<&ServerThread> {
        if self.ring.is_empty() {
            return None;
        }
        let hash = if global {
            global_hash_key(key)
        } else {
            local_hash_key(key)
        };
        self.ring
            .range(hash..)
            .next()
            .or_else(|| self.ring.iter().next())
            .map(|(_, st)| st)
    }

    /// Verify that all requested virtual nodes were inserted (no hash collisions).
    /// Returns the actual ring size.
    pub fn verify_size(&self, expected: usize) -> bool {
        self.ring.len() == expected
    }

    /// Find `rep_count` unique responsible servers for a key.
    ///
    /// Walks the ring clockwise from hash(key), collecting servers with
    /// unique IDs. Mirrors `responsible_global` in C++.
    pub fn find_responsible(&self, key: &str, rep_count: u32, global: bool) -> Vec<&ServerThread> {
        if self.ring.is_empty() || rep_count == 0 {
            return vec![];
        }

        let hash = if global {
            global_hash_key(key)
        } else {
            local_hash_key(key)
        };

        let mut result = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // Start from hash position, walk clockwise.
        let mut iter = self.ring.range(hash..).chain(self.ring.iter());
        while result.len() < rep_count as usize {
            match iter.next() {
                Some((_, st)) => {
                    if seen_ids.insert(st.id()) {
                        result.push(st);
                    }
                }
                None => break,
            }
        }

        result
    }

    /// Find `rep_count` unique responsible thread IDs for a key (local ring).
    ///
    /// Mirrors `responsible_local` in C++.
    pub fn find_responsible_local(&self, key: &str, rep_count: u32) -> Vec<u32> {
        if self.ring.is_empty() || rep_count == 0 {
            return vec![];
        }

        let hash = local_hash_key(key);
        let mut result = Vec::new();
        let mut seen_tids = std::collections::HashSet::new();

        let mut iter = self.ring.range(hash..).chain(self.ring.iter());
        while result.len() < rep_count as usize {
            match iter.next() {
                Some((_, st)) => {
                    if seen_tids.insert(st.tid()) {
                        result.push(st.tid());
                    }
                }
                None => break,
            }
        }

        result
    }

    /// Get all unique server threads (deduplicated by id).
    pub fn get_unique_servers(&self) -> Vec<&ServerThread> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for st in self.ring.values() {
            if seen.insert(st.id()) {
                result.push(st);
            }
        }
        result
    }

    /// Number of entries in the ring (including virtual nodes).
    pub fn size(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

impl Default for ConsistentHashRing {
    fn default() -> Self {
        Self::new()
    }
}

// ── Hash functions ──────────────────────────────────────────────────

/// Global hasher for server threads: hash("GLOBAL" + virtual_id).
/// Mirrors `GlobalHasher::operator()(const ServerThread&)` in C++.
fn global_hash_thread(st: &ServerThread) -> u64 {
    let input = format!("GLOBAL{}", st.virtual_id());
    std_hash(&input)
}

/// Global hasher for keys: hash("GLOBAL" + key).
/// Mirrors `GlobalHasher::operator()(const Key&)` in C++.
fn global_hash_key(key: &str) -> u64 {
    let input = format!("GLOBAL{}", key);
    std_hash(&input)
}

/// Local hasher for server threads: hash(tid + "_" + virtual_num).
/// Mirrors `LocalHasher::operator()(const ServerThread&)` in C++.
fn local_hash_thread(st: &ServerThread) -> u64 {
    let input = format!("{}_{}", st.tid(), st.virtual_num());
    std_hash(&input)
}

/// Local hasher for keys: hash(key).
/// Mirrors `LocalHasher::operator()(const Key&)` in C++.
fn local_hash_key(key: &str) -> u64 {
    std_hash(key)
}

/// Compute a u64 hash using Rust's DefaultHasher.
///
/// Note: This does not produce the same values as C++ std::hash<string>.
/// This is acceptable because the hash ring is rebuilt from the same
/// cluster membership data on each node — all Rust nodes agree with
/// each other. Rust and C++ servers communicate via protobuf, not
/// shared hash rings.
fn std_hash(input: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_find_returns_none() {
        let ring = ConsistentHashRing::new();
        assert!(ring.find("key", true).is_none());
    }

    #[test]
    fn insert_and_find() {
        let mut ring = ConsistentHashRing::new();
        ring.insert("1.2.3.4", "10.0.0.1", 0, 0, 100, true);

        let found = ring.find("test_key", true);
        assert!(found.is_some());
        assert_eq!(found.expect("found").public_ip(), "1.2.3.4");
    }

    #[test]
    fn unique_servers() {
        let mut ring = ConsistentHashRing::new();
        ring.insert("1.2.3.4", "10.0.0.1", 0, 0, 100, true);
        ring.insert("5.6.7.8", "10.0.0.2", 0, 0, 100, true);

        let unique = ring.get_unique_servers();
        assert_eq!(unique.len(), 2);
    }

    #[test]
    fn remove_server() {
        let mut ring = ConsistentHashRing::new();
        ring.insert("1.2.3.4", "10.0.0.1", 0, 0, 100, true);
        ring.insert("5.6.7.8", "10.0.0.2", 0, 0, 100, true);
        assert_eq!(ring.size(), 200);

        ring.remove("1.2.3.4", "10.0.0.1", 0);
        assert_eq!(ring.size(), 100);
        assert_eq!(ring.get_unique_servers().len(), 1);
    }

    #[test]
    fn ring_wraps_around() {
        let mut ring = ConsistentHashRing::new();
        ring.insert("1.2.3.4", "10.0.0.1", 0, 0, 1, true);

        // With only one node, all keys should find it.
        let a = ring.find("key_a", true);
        let b = ring.find("key_z", true);
        assert!(a.is_some());
        assert!(b.is_some());
        assert_eq!(a.expect("a").id(), b.expect("b").id());
    }

    #[test]
    fn local_ring_distributes_by_key() {
        let mut ring = ConsistentHashRing::new();
        ring.insert("1.2.3.4", "10.0.0.1", 0, 0, 3000, false);
        ring.insert("1.2.3.4", "10.0.0.1", 1, 0, 3000, false);

        // With 2 threads and 3000 virtual nodes each, keys should
        // distribute roughly evenly.
        let mut tid0_count = 0;
        let mut tid1_count = 0;
        for i in 0..100 {
            let key = format!("test_key_{}", i);
            if let Some(st) = ring.find(&key, false) {
                if st.tid() == 0 {
                    tid0_count += 1;
                } else {
                    tid1_count += 1;
                }
            }
        }
        // Both threads should get some keys (with 3000 vnodes each).
        assert!(tid0_count > 10, "tid0 got {} keys", tid0_count);
        assert!(tid1_count > 10, "tid1 got {} keys", tid1_count);
    }
}
