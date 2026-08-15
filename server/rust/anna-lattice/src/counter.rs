//! PN-Counter: a positive-negative counter with per-node max merge.

use crate::Lattice;
use std::collections::HashMap;

/// A PN-Counter (Positive-Negative Counter) CRDT.
///
/// Each node maintains monotonically increasing totals for increments
/// and decrements. The effective counter value is
/// `sum(increments) - sum(decrements)`.
///
/// Merge takes the per-node maximum of both the increment and decrement maps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PnCounter {
    /// Per-node cumulative increments.
    pub increments: HashMap<String, u64>,
    /// Per-node cumulative decrements.
    pub decrements: HashMap<String, u64>,
}

impl PnCounter {
    /// Create an empty counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute the effective counter value.
    pub fn value(&self) -> i64 {
        let inc: u64 = self.increments.values().sum();
        let dec: u64 = self.decrements.values().sum();
        inc as i64 - dec as i64
    }
}

impl Lattice for PnCounter {
    fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for (node, &val) in &other.increments {
            let entry = self.increments.entry(node.clone()).or_insert(0);
            if val > *entry {
                *entry = val;
                changed = true;
            }
        }
        for (node, &val) in &other.decrements {
            let entry = self.decrements.entry(node.clone()).or_insert(0);
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

    #[test]
    fn per_node_max() {
        let mut a = PnCounter::new();
        a.increments.insert("n1".into(), 5);
        a.increments.insert("n2".into(), 3);

        let mut b = PnCounter::new();
        b.increments.insert("n1".into(), 3);
        b.increments.insert("n2".into(), 7);
        b.increments.insert("n3".into(), 1);

        assert!(a.merge(b));
        assert_eq!(a.increments["n1"], 5); // max(5,3)
        assert_eq!(a.increments["n2"], 7); // max(3,7)
        assert_eq!(a.increments["n3"], 1); // new
    }

    #[test]
    fn value_computation() {
        let mut c = PnCounter::new();
        c.increments.insert("n1".into(), 10);
        c.decrements.insert("n1".into(), 3);
        assert_eq!(c.value(), 7);
    }

    #[test]
    fn idempotent() {
        let mut a = PnCounter::new();
        a.increments.insert("n1".into(), 5);
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn commutative() {
        let mut a = PnCounter::new();
        a.increments.insert("n1".into(), 5);
        let mut b = PnCounter::new();
        b.increments.insert("n2".into(), 3);

        let mut x = a.clone();
        let mut y = b.clone();
        x.merge(b);
        y.merge(a);
        assert_eq!(x, y);
    }

    #[test]
    fn decrement_merge() {
        let mut a = PnCounter::new();
        a.decrements.insert("n1".into(), 2);
        let mut b = PnCounter::new();
        b.decrements.insert("n1".into(), 5);
        assert!(a.merge(b));
        assert_eq!(a.decrements["n1"], 5);
    }

    #[test]
    fn empty_merge() {
        let mut a = PnCounter::new();
        a.increments.insert("n1".into(), 5);
        assert!(!a.merge(PnCounter::new()));
    }
}
