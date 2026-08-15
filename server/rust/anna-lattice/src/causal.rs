//! Causal registers: vector-clock-based CRDT registers.
//!
//! - [`CausalRegister`]: single-key causal register (VC + values)
//! - [`MultiCausalRegister`]: multi-key causal register (VC + dependencies + values)

use crate::vector_clock::VectorClock;
use crate::Lattice;
use std::collections::HashMap;

/// A single-key causal register.
///
/// Uses a vector clock to track causality. On merge:
/// - If one version dominates, its values replace the other's
/// - If versions are concurrent, values are unioned
/// - Vector clocks are always merged (per-node max)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CausalRegister<T: Clone + Eq> {
    /// Vector clock tracking causality.
    pub vector_clock: VectorClock,
    /// The set of concurrent values.
    pub values: Vec<T>,
}

impl<T: Clone + Eq> CausalRegister<T> {
    /// Create an empty causal register.
    pub fn new() -> Self {
        Self {
            vector_clock: VectorClock::new(),
            values: Vec::new(),
        }
    }
}

impl<T: Clone + Eq> Lattice for CausalRegister<T> {
    fn merge(&mut self, other: Self) -> bool {
        let old_vc = self.vector_clock.clone();
        self.vector_clock.merge(other.vector_clock.clone());

        if other.vector_clock.dominates(&old_vc) {
            // Incoming dominates: replace values.
            self.values = other.values;
            true
        } else if !old_vc.dominates(&other.vector_clock) && old_vc != other.vector_clock {
            // Concurrent: union values.
            let mut changed = false;
            for v in other.values {
                if !self.values.contains(&v) {
                    self.values.push(v);
                    changed = true;
                }
            }
            // VC was already merged above.
            changed || self.vector_clock != old_vc
        } else {
            // Old dominates or equal: keep old values, VC may have changed.
            self.vector_clock != old_vc
        }
    }
}

/// A multi-key causal register with cross-key dependency tracking.
///
/// Like [`CausalRegister`], but also tracks dependencies on other keys'
/// vector clocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiCausalRegister<T: Clone + Eq> {
    /// Vector clock tracking causality for this key.
    pub vector_clock: VectorClock,
    /// Cross-key dependencies: key -> vector clock.
    pub dependencies: HashMap<String, VectorClock>,
    /// The set of concurrent values.
    pub values: Vec<T>,
}

impl<T: Clone + Eq> MultiCausalRegister<T> {
    /// Create an empty multi-causal register.
    pub fn new() -> Self {
        Self {
            vector_clock: VectorClock::new(),
            dependencies: HashMap::new(),
            values: Vec::new(),
        }
    }
}

impl<T: Clone + Eq> Lattice for MultiCausalRegister<T> {
    fn merge(&mut self, other: Self) -> bool {
        let old_vc = self.vector_clock.clone();
        self.vector_clock.merge(other.vector_clock.clone());

        if other.vector_clock.dominates(&old_vc) {
            // Incoming dominates: replace values and dependencies.
            self.values = other.values;
            self.dependencies = other.dependencies;
            true
        } else if !old_vc.dominates(&other.vector_clock) && old_vc != other.vector_clock {
            // Concurrent: union values and merge dependencies.
            let mut changed = false;
            for v in other.values {
                if !self.values.contains(&v) {
                    self.values.push(v);
                    changed = true;
                }
            }
            // Merge dependency VCs per-key.
            for (key, vc) in other.dependencies {
                let entry = self.dependencies.entry(key).or_default();
                if entry.merge(vc) {
                    changed = true;
                }
            }
            changed || self.vector_clock != old_vc
        } else {
            self.vector_clock != old_vc
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vc(entries: &[(&str, u32)]) -> VectorClock {
        VectorClock(entries.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }

    #[test]
    fn causal_incoming_dominates() {
        let mut a = CausalRegister {
            vector_clock: vc(&[("n1", 1)]),
            values: vec!["old".to_string()],
        };
        let b = CausalRegister {
            vector_clock: vc(&[("n1", 2)]),
            values: vec!["new".to_string()],
        };
        assert!(a.merge(b));
        assert_eq!(a.values, vec!["new".to_string()]);
        assert_eq!(a.vector_clock.0["n1"], 2);
    }

    #[test]
    fn causal_old_dominates() {
        let mut a = CausalRegister {
            vector_clock: vc(&[("n1", 3)]),
            values: vec!["keep".to_string()],
        };
        let b = CausalRegister {
            vector_clock: vc(&[("n1", 1)]),
            values: vec!["discard".to_string()],
        };
        assert!(!a.merge(b));
        assert_eq!(a.values, vec!["keep".to_string()]);
    }

    #[test]
    fn causal_concurrent_unions_values() {
        let mut a = CausalRegister {
            vector_clock: vc(&[("n1", 2), ("n2", 0)]),
            values: vec!["a".to_string()],
        };
        let b = CausalRegister {
            vector_clock: vc(&[("n1", 0), ("n2", 2)]),
            values: vec!["b".to_string()],
        };
        assert!(a.merge(b));
        assert_eq!(a.values.len(), 2);
        assert!(a.values.contains(&"a".to_string()));
        assert!(a.values.contains(&"b".to_string()));
    }

    #[test]
    fn causal_idempotent() {
        let mut a = CausalRegister {
            vector_clock: vc(&[("n1", 2)]),
            values: vec!["x".to_string()],
        };
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn multi_causal_incoming_dominates() {
        let mut a = MultiCausalRegister {
            vector_clock: vc(&[("n1", 1)]),
            dependencies: HashMap::new(),
            values: vec!["old".to_string()],
        };
        let mut deps = HashMap::new();
        deps.insert("other_key".to_string(), vc(&[("n1", 1)]));
        let b = MultiCausalRegister {
            vector_clock: vc(&[("n1", 2)]),
            dependencies: deps.clone(),
            values: vec!["new".to_string()],
        };
        assert!(a.merge(b));
        assert_eq!(a.values, vec!["new".to_string()]);
        assert_eq!(a.dependencies, deps);
    }

    #[test]
    fn multi_causal_concurrent_merges_deps() {
        let mut deps_a = HashMap::new();
        deps_a.insert("k1".to_string(), vc(&[("n1", 2)]));
        let mut a = MultiCausalRegister {
            vector_clock: vc(&[("n1", 2)]),
            dependencies: deps_a,
            values: vec!["a".to_string()],
        };

        let mut deps_b = HashMap::new();
        deps_b.insert("k1".to_string(), vc(&[("n2", 3)]));
        let b = MultiCausalRegister {
            vector_clock: vc(&[("n2", 2)]),
            dependencies: deps_b,
            values: vec!["b".to_string()],
        };

        assert!(a.merge(b));
        assert_eq!(a.values.len(), 2);
        // Dependencies for k1 should be merged: {n1:2, n2:3}
        let dep_vc = &a.dependencies["k1"];
        assert_eq!(dep_vc.0["n1"], 2);
        assert_eq!(dep_vc.0["n2"], 3);
    }
}
