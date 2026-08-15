//! Memory-tier serializers for all lattice types.
//!
//! Each serializer stores values as raw protobuf bytes in a HashMap.
//! On PUT, it decodes protobuf, converts to a lattice type from
//! `anna_lattice` via `From` impls (enabled by the `proto` feature),
//! merges, converts back, and stores.

use super::{GetResult, Serializer};
use anna_lattice::{
    CausalRegister, GSet, Lattice, LwwRegister, MultiCausalRegister, OrSet, PnCounter,
    PriorityRegister,
};
use anna_server_common::proto::kvs::*;
use prost::Message;
use std::collections::HashMap;

// ── LWW (Last-Writer-Wins) ─────────────────────────────────────────

pub struct LwwSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl LwwSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for LwwSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = LwwValue::decode(payload).unwrap_or_default();
        let new_lattice = LwwRegister::from(&new_proto);

        if let Some(existing) = self.store.get(key) {
            let old_proto = LwwValue::decode(existing.as_slice()).unwrap_or_default();
            let mut merged = LwwRegister::from(&old_proto);
            if !merged.merge(new_lattice) {
                return existing.len(); // Old value wins.
            }
            let encoded = LwwValue::from(&merged).encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            size
        } else {
            let encoded = new_proto.encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            size
        }
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => {
                // Check for tombstone: LWW with empty value.
                if let Ok(lww) = LwwValue::decode(v.as_slice()) {
                    if lww.value.is_empty() {
                        return (vec![], 1); // KEY_DNE (tombstone)
                    }
                }
                (v.clone(), 0)
            }
            None => (vec![], 1), // KEY_DNE
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── SET (grow-only, union merge) ────────────────────────────────────

pub struct SetSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl SetSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for SetSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = SetValue::decode(payload).unwrap_or_default();
        let new_lattice = GSet::from(&new_proto);

        let merged = if let Some(existing) = self.store.get(key) {
            let old_proto = SetValue::decode(existing.as_slice()).unwrap_or_default();
            let mut old_lattice = GSet::from(&old_proto);
            old_lattice.merge(new_lattice);
            old_lattice
        } else {
            new_lattice
        };

        let encoded = SetValue::from(&merged).encode_to_vec();
        let size = encoded.len();
        self.store.insert(key.to_string(), encoded);
        size
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── PRIORITY (lowest priority wins) ─────────────────────────────────

pub struct PrioritySerializer {
    store: HashMap<String, Vec<u8>>,
}

impl PrioritySerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for PrioritySerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = PriorityValue::decode(payload).unwrap_or_default();
        let new_lattice = PriorityRegister::from(&new_proto);

        if let Some(existing) = self.store.get(key) {
            let old_proto = PriorityValue::decode(existing.as_slice()).unwrap_or_default();
            let mut merged = PriorityRegister::from(&old_proto);
            if !merged.merge(new_lattice) {
                return existing.len();
            }
            let encoded = PriorityValue::from(&merged).encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            size
        } else {
            let encoded = new_proto.encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            size
        }
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── COUNTER (PN-Counter CRDT) ───────────────────────────────────────

pub struct CounterSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl CounterSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for CounterSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = CounterValue::decode(payload).unwrap_or_default();
        let new_lattice = PnCounter::from(&new_proto);

        let merged = if let Some(existing) = self.store.get(key) {
            let old_proto = CounterValue::decode(existing.as_slice()).unwrap_or_default();
            let mut old_lattice = PnCounter::from(&old_proto);
            old_lattice.merge(new_lattice);
            old_lattice
        } else {
            new_lattice
        };

        let encoded = CounterValue::from(&merged).encode_to_vec();
        let size = encoded.len();
        self.store.insert(key.to_string(), encoded);
        size
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── OR-SET (Observed-Remove Set) ────────────────────────────────────

pub struct OrSetSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl OrSetSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for OrSetSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = OrSetValue::decode(payload).unwrap_or_default();
        let new_lattice = OrSet::from(&new_proto);

        let merged = if let Some(existing) = self.store.get(key) {
            let old_proto = OrSetValue::decode(existing.as_slice()).unwrap_or_default();
            let mut old_lattice = OrSet::from(&old_proto);
            old_lattice.merge(new_lattice);
            old_lattice
        } else {
            new_lattice
        };

        let encoded = OrSetValue::from(&merged).encode_to_vec();
        let size = encoded.len();
        self.store.insert(key.to_string(), encoded);
        size
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── SINGLE_CAUSAL ───────────────────────────────────────────────────

pub struct SingleCausalSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl SingleCausalSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for SingleCausalSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = SingleKeyCausalValue::decode(payload).unwrap_or_default();
        let new_lattice = CausalRegister::from(&new_proto);

        if let Some(existing) = self.store.get(key) {
            let old_proto = SingleKeyCausalValue::decode(existing.as_slice()).unwrap_or_default();
            let mut merged = CausalRegister::from(&old_proto);
            merged.merge(new_lattice);
            let encoded = SingleKeyCausalValue::from(&merged).encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            return size;
        }
        let encoded = new_proto.encode_to_vec();
        let size = encoded.len();
        self.store.insert(key.to_string(), encoded);
        size
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── MULTI_CAUSAL ────────────────────────────────────────────────────

pub struct MultiCausalSerializer {
    store: HashMap<String, Vec<u8>>,
}

impl MultiCausalSerializer {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl Serializer for MultiCausalSerializer {
    fn put(&mut self, key: &str, payload: &[u8]) -> usize {
        let new_proto = MultiKeyCausalValue::decode(payload).unwrap_or_default();
        let new_lattice = MultiCausalRegister::from(&new_proto);

        if let Some(existing) = self.store.get(key) {
            let old_proto = MultiKeyCausalValue::decode(existing.as_slice()).unwrap_or_default();
            let mut merged = MultiCausalRegister::from(&old_proto);
            merged.merge(new_lattice);
            let encoded = MultiKeyCausalValue::from(&merged).encode_to_vec();
            let size = encoded.len();
            self.store.insert(key.to_string(), encoded);
            return size;
        }
        let encoded = new_proto.encode_to_vec();
        let size = encoded.len();
        self.store.insert(key.to_string(), encoded);
        size
    }

    fn get(&self, key: &str) -> GetResult {
        match self.store.get(key) {
            Some(v) => (v.clone(), 0),
            None => (vec![], 1),
        }
    }

    fn remove(&mut self, key: &str) -> bool {
        self.store.remove(key).is_some()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lww_put_get() {
        let mut s = LwwSerializer::new();
        let lww = LwwValue {
            timestamp: 1,
            value: b"hello".to_vec(),
        };
        s.put("k", &lww.encode_to_vec());
        let (data, err) = s.get("k");
        assert_eq!(err, 0);
        let got = LwwValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.value, b"hello");
    }

    #[test]
    fn lww_higher_timestamp_wins() {
        let mut s = LwwSerializer::new();
        let v1 = LwwValue {
            timestamp: 1,
            value: b"old".to_vec(),
        };
        let v2 = LwwValue {
            timestamp: 2,
            value: b"new".to_vec(),
        };
        s.put("k", &v1.encode_to_vec());
        s.put("k", &v2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = LwwValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.value, b"new");
    }

    #[test]
    fn lww_lower_timestamp_ignored() {
        let mut s = LwwSerializer::new();
        let v1 = LwwValue {
            timestamp: 5,
            value: b"first".to_vec(),
        };
        let v2 = LwwValue {
            timestamp: 3,
            value: b"second".to_vec(),
        };
        s.put("k", &v1.encode_to_vec());
        s.put("k", &v2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = LwwValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.value, b"first");
    }

    #[test]
    fn set_union_merge() {
        let mut s = SetSerializer::new();
        let s1 = SetValue {
            values: vec![b"a".to_vec(), b"b".to_vec()],
        };
        let s2 = SetValue {
            values: vec![b"b".to_vec(), b"c".to_vec()],
        };
        s.put("k", &s1.encode_to_vec());
        s.put("k", &s2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = SetValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.values.len(), 3); // {a, b, c}
    }

    #[test]
    fn priority_lowest_wins() {
        let mut s = PrioritySerializer::new();
        let p1 = PriorityValue {
            priority: 10.0,
            value: b"low".to_vec(),
        };
        let p2 = PriorityValue {
            priority: 1.0,
            value: b"high".to_vec(),
        };
        s.put("k", &p1.encode_to_vec());
        s.put("k", &p2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = PriorityValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.priority, 1.0);
        assert_eq!(got.value, b"high");
    }

    #[test]
    fn counter_merge() {
        let mut s = CounterSerializer::new();
        let mut c1 = CounterValue::default();
        c1.increments.insert("node1".into(), 5);
        let mut c2 = CounterValue::default();
        c2.increments.insert("node1".into(), 3);
        c2.increments.insert("node2".into(), 2);
        s.put("k", &c1.encode_to_vec());
        s.put("k", &c2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = CounterValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.increments["node1"], 5); // max(5,3)
        assert_eq!(got.increments["node2"], 2);
    }

    #[test]
    fn or_set_merge() {
        let mut s = OrSetSerializer::new();
        let mut os1 = OrSetValue::default();
        os1.elements.insert("tag1".into(), b"apple".to_vec());
        let mut os2 = OrSetValue::default();
        os2.elements.insert("tag2".into(), b"banana".to_vec());
        os2.tombstones.push("tag1".into());
        s.put("k", &os1.encode_to_vec());
        s.put("k", &os2.encode_to_vec());
        let (data, _) = s.get("k");
        let got = OrSetValue::decode(data.as_slice()).unwrap();
        assert_eq!(got.elements.len(), 2); // Both tags present
        assert_eq!(got.tombstones.len(), 1); // tag1 tombstoned
    }

    #[test]
    fn get_nonexistent_returns_key_dne() {
        let s = LwwSerializer::new();
        let (_, err) = s.get("missing");
        assert_eq!(err, 1);
    }

    #[test]
    fn remove_returns_true_if_existed() {
        let mut s = LwwSerializer::new();
        let v = LwwValue {
            timestamp: 1,
            value: b"x".to_vec(),
        };
        s.put("k", &v.encode_to_vec());
        assert!(s.remove("k"));
        assert!(!s.remove("k")); // Already removed.
    }
}
