//! Vector Clock: per-node max of logical clocks.

use crate::Lattice;
use std::collections::HashMap;

/// A vector clock. Each entry maps a node ID to a logical clock value.
/// Merge takes the per-node maximum.
///
/// Equality is **logical**: missing entries are treated as zero.
/// `{n1: 2}` and `{n1: 2, n2: 0}` are considered equal.
#[derive(Debug, Clone, Default)]
pub struct VectorClock(pub HashMap<String, u32>);

impl PartialEq for VectorClock {
    fn eq(&self, other: &Self) -> bool {
        // Missing entries treated as zero.
        for (node, &val) in &self.0 {
            if val != other.0.get(node).copied().unwrap_or(0) {
                return false;
            }
        }
        for (node, &val) in &other.0 {
            if val != self.0.get(node).copied().unwrap_or(0) {
                return false;
            }
        }
        true
    }
}

impl Eq for VectorClock {}

impl VectorClock {
    /// Create an empty vector clock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if `self` strictly dominates `other`.
    ///
    /// `a` dominates `b` if every entry in `b` has `a[node] >= b[node]`,
    /// and `a` has at least one entry strictly greater than `b` (or an
    /// entry that `b` doesn't have).
    pub fn dominates(&self, other: &Self) -> bool {
        let mut strictly_greater = false;

        // Every entry in other must have self >= other.
        for (node, &other_val) in &other.0 {
            let self_val = self.0.get(node).copied().unwrap_or(0);
            if self_val < other_val {
                return false;
            }
            if self_val > other_val {
                strictly_greater = true;
            }
        }

        // Check entries in self that aren't in other.
        if !strictly_greater {
            for (node, &self_val) in &self.0 {
                if !other.0.contains_key(node) && self_val > 0 {
                    strictly_greater = true;
                    break;
                }
            }
        }

        strictly_greater
    }
}

impl Lattice for VectorClock {
    fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for (node, &val) in &other.0 {
            let entry = self.0.entry(node.clone()).or_insert(0);
            if val > *entry {
                *entry = val;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vc(entries: &[(&str, u32)]) -> VectorClock {
        VectorClock(entries.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }

    #[test]
    fn per_node_max() {
        let mut a = vc(&[("n1", 3), ("n2", 5)]);
        let b = vc(&[("n1", 5), ("n2", 2), ("n3", 1)]);
        assert!(a.merge(b));
        assert_eq!(a.0["n1"], 5);
        assert_eq!(a.0["n2"], 5);
        assert_eq!(a.0["n3"], 1);
    }

    #[test]
    fn dominates_strictly_greater() {
        let a = vc(&[("n1", 3), ("n2", 2)]);
        let b = vc(&[("n1", 2), ("n2", 1)]);
        assert!(a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn dominates_extra_entry() {
        let a = vc(&[("n1", 1), ("n2", 1)]);
        let b = vc(&[("n1", 1)]);
        assert!(a.dominates(&b));
    }

    #[test]
    fn concurrent_neither_dominates() {
        let a = vc(&[("n1", 3), ("n2", 1)]);
        let b = vc(&[("n1", 1), ("n2", 3)]);
        assert!(!a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn equal_does_not_dominate() {
        let a = vc(&[("n1", 2)]);
        let b = vc(&[("n1", 2)]);
        assert!(!a.dominates(&b));
    }

    #[test]
    fn idempotent() {
        let mut a = vc(&[("n1", 3)]);
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn logical_equality_with_explicit_zeros() {
        let a = vc(&[("n1", 1)]);
        let b = vc(&[("n1", 1), ("n2", 0)]);
        assert_eq!(a, b);
    }

    #[test]
    fn logical_inequality() {
        let a = vc(&[("n1", 1)]);
        let b = vc(&[("n1", 2)]);
        assert_ne!(a, b);
    }

    #[test]
    fn commutative() {
        let c1 = vc(&[("n1", 3)]);
        let c2 = vc(&[("n2", 5)]);
        let mut a = c1.clone();
        let mut b = c2.clone();
        a.merge(c2);
        b.merge(c1);
        assert_eq!(a, b);
    }
}
