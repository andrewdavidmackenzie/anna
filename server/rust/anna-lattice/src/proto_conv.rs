//! `From` trait conversions between protobuf types and lattice types.
//!
//! Enabled by the `proto` feature flag. Requires the `anna-server-common`
//! crate for protobuf type definitions.

use crate::{
    CausalRegister, GSet, LwwRegister, MultiCausalRegister, OrSet, PnCounter, PriorityRegister,
    VectorClock,
};
use anna_server_common::proto::kvs::*;
use anna_server_common::proto::shared::KeyVersion;
use std::collections::HashMap;

// ── LWW ─────────────────────────────────────────────────────────────

impl From<&LwwValue> for LwwRegister<Vec<u8>> {
    fn from(proto: &LwwValue) -> Self {
        LwwRegister::new(proto.timestamp, proto.value.clone())
    }
}

impl From<&LwwRegister<Vec<u8>>> for LwwValue {
    fn from(lattice: &LwwRegister<Vec<u8>>) -> Self {
        LwwValue {
            timestamp: lattice.timestamp,
            value: lattice.value.clone(),
        }
    }
}

// ── SET ─────────────────────────────────────────────────────────────

impl From<&SetValue> for GSet<Vec<u8>> {
    fn from(proto: &SetValue) -> Self {
        proto.values.iter().cloned().collect()
    }
}

impl From<&GSet<Vec<u8>>> for SetValue {
    fn from(lattice: &GSet<Vec<u8>>) -> Self {
        SetValue {
            values: lattice.iter().cloned().collect(),
        }
    }
}

// ── PRIORITY ────────────────────────────────────────────────────────

impl From<&PriorityValue> for PriorityRegister<Vec<u8>> {
    fn from(proto: &PriorityValue) -> Self {
        PriorityRegister::new(proto.priority, proto.value.clone())
    }
}

impl From<&PriorityRegister<Vec<u8>>> for PriorityValue {
    fn from(lattice: &PriorityRegister<Vec<u8>>) -> Self {
        PriorityValue {
            priority: lattice.priority,
            value: lattice.value.clone(),
        }
    }
}

// ── COUNTER ─────────────────────────────────────────────────────────

impl From<&CounterValue> for PnCounter {
    fn from(proto: &CounterValue) -> Self {
        PnCounter {
            increments: proto.increments.clone(),
            decrements: proto.decrements.clone(),
        }
    }
}

impl From<&PnCounter> for CounterValue {
    fn from(lattice: &PnCounter) -> Self {
        CounterValue {
            increments: lattice.increments.clone(),
            decrements: lattice.decrements.clone(),
        }
    }
}

// ── OR-SET ──────────────────────────────────────────────────────────

impl From<&OrSetValue> for OrSet<Vec<u8>> {
    fn from(proto: &OrSetValue) -> Self {
        OrSet {
            elements: proto.elements.clone(),
            tombstones: proto.tombstones.iter().cloned().collect(),
        }
    }
}

impl From<&OrSet<Vec<u8>>> for OrSetValue {
    fn from(lattice: &OrSet<Vec<u8>>) -> Self {
        let mut tombstones: Vec<String> = lattice.tombstones.iter().cloned().collect();
        tombstones.sort();
        OrSetValue {
            elements: lattice.elements.clone(),
            tombstones,
        }
    }
}

// ── SINGLE_CAUSAL ───────────────────────────────────────────────────

impl From<&SingleKeyCausalValue> for CausalRegister<Vec<u8>> {
    fn from(proto: &SingleKeyCausalValue) -> Self {
        CausalRegister {
            vector_clock: VectorClock(proto.vector_clock.clone()),
            values: proto.values.clone(),
        }
    }
}

impl From<&CausalRegister<Vec<u8>>> for SingleKeyCausalValue {
    fn from(lattice: &CausalRegister<Vec<u8>>) -> Self {
        SingleKeyCausalValue {
            vector_clock: lattice.vector_clock.0.clone(),
            values: lattice.values.clone(),
        }
    }
}

// ── MULTI_CAUSAL ────────────────────────────────────────────────────

impl From<&MultiKeyCausalValue> for MultiCausalRegister<Vec<u8>> {
    fn from(proto: &MultiKeyCausalValue) -> Self {
        let mut deps = HashMap::new();
        for kv in &proto.dependencies {
            deps.insert(kv.key.clone(), VectorClock(kv.vector_clock.clone()));
        }
        MultiCausalRegister {
            vector_clock: VectorClock(proto.vector_clock.clone()),
            dependencies: deps,
            values: proto.values.clone(),
        }
    }
}

impl From<&MultiCausalRegister<Vec<u8>>> for MultiKeyCausalValue {
    fn from(lattice: &MultiCausalRegister<Vec<u8>>) -> Self {
        let mut dep_keys: Vec<&String> = lattice.dependencies.keys().collect();
        dep_keys.sort();
        let deps = dep_keys
            .into_iter()
            .map(|key| KeyVersion {
                key: key.clone(),
                vector_clock: lattice.dependencies[key].0.clone(),
            })
            .collect();
        MultiKeyCausalValue {
            vector_clock: lattice.vector_clock.0.clone(),
            dependencies: deps,
            values: lattice.values.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lattice;

    #[test]
    fn lww_roundtrip() {
        let proto = LwwValue {
            timestamp: 42,
            value: b"hello".to_vec(),
        };
        let lattice = LwwRegister::from(&proto);
        let back = LwwValue::from(&lattice);
        assert_eq!(proto.timestamp, back.timestamp);
        assert_eq!(proto.value, back.value);
    }

    #[test]
    fn set_roundtrip() {
        let proto = SetValue {
            values: vec![b"a".to_vec(), b"b".to_vec()],
        };
        let lattice = GSet::from(&proto);
        let back = SetValue::from(&lattice);
        assert_eq!(back.values.len(), 2);
    }

    #[test]
    fn counter_roundtrip() {
        let mut proto = CounterValue::default();
        proto.increments.insert("n1".into(), 5);
        proto.decrements.insert("n1".into(), 2);
        let lattice = PnCounter::from(&proto);
        assert_eq!(lattice.value(), 3);
        let back = CounterValue::from(&lattice);
        assert_eq!(back.increments["n1"], 5);
        assert_eq!(back.decrements["n1"], 2);
    }

    #[test]
    fn lww_merge_via_from() {
        let p1 = LwwValue {
            timestamp: 1,
            value: b"old".to_vec(),
        };
        let p2 = LwwValue {
            timestamp: 2,
            value: b"new".to_vec(),
        };
        let mut l1 = LwwRegister::from(&p1);
        let l2 = LwwRegister::from(&p2);
        l1.merge(l2);
        let result = LwwValue::from(&l1);
        assert_eq!(result.value, b"new");
        assert_eq!(result.timestamp, 2);
    }
}
