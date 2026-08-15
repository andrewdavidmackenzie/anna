//! Observed-Remove Set (OR-Set): add-wins semantics with tombstones.

use crate::Lattice;
use std::collections::{HashMap, HashSet};

/// An Observed-Remove Set (OR-Set) CRDT.
///
/// Each `add` operation creates a unique tag for the element. `remove`
/// tombstones specific tags. Concurrent add and remove of the same
/// element results in the add winning (because the new add generates
/// a fresh tag not covered by the tombstone).
///
/// Merge is union of both the element map and the tombstone set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrSet<T: Clone + Eq> {
    /// Tag -> element value. Each add gets a unique tag.
    pub elements: HashMap<String, T>,
    /// Set of tombstoned (removed) tags.
    pub tombstones: HashSet<String>,
}

impl<T: Clone + Eq> OrSet<T> {
    /// Create an empty OR-Set.
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
        }
    }

    /// Compute the live set: elements whose tags are not tombstoned.
    pub fn live_elements(&self) -> Vec<&T> {
        self.elements
            .iter()
            .filter(|(tag, _)| !self.tombstones.contains(tag.as_str()))
            .map(|(_, v)| v)
            .collect()
    }
}

impl<T: Clone + Eq> Lattice for OrSet<T> {
    fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for (tag, val) in other.elements {
            if self.elements.insert(tag.clone(), val).is_none() {
                changed = true;
            }
        }
        for tag in other.tombstones {
            if self.tombstones.insert(tag) {
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
    fn union_of_elements_and_tombstones() {
        let mut a = OrSet::new();
        a.elements.insert("t1".into(), "apple".to_string());

        let mut b = OrSet::new();
        b.elements.insert("t2".into(), "banana".to_string());
        b.tombstones.insert("t1".into());

        assert!(a.merge(b));
        assert_eq!(a.elements.len(), 2);
        assert_eq!(a.tombstones.len(), 1);
        assert!(a.tombstones.contains("t1"));
    }

    #[test]
    fn live_elements_excludes_tombstoned() {
        let mut s = OrSet::new();
        s.elements.insert("t1".into(), "x".to_string());
        s.elements.insert("t2".into(), "y".to_string());
        s.tombstones.insert("t1".into());
        let live = s.live_elements();
        assert_eq!(live.len(), 1);
        assert_eq!(*live[0], "y");
    }

    #[test]
    fn add_wins_over_concurrent_remove() {
        // Simulates: node A adds "x" with tag t1, node B removes "x"
        // (tombstones t1) and concurrently node C adds "x" with tag t2.
        let mut a = OrSet::new();
        a.elements.insert("t1".into(), "x".to_string());

        let mut b = OrSet::new();
        b.tombstones.insert("t1".into());
        b.elements.insert("t2".into(), "x".to_string());

        a.merge(b);
        let live = a.live_elements();
        // t1 is tombstoned but t2 is fresh -> "x" still live
        assert_eq!(live.len(), 1);
        assert_eq!(*live[0], "x");
    }

    #[test]
    fn idempotent() {
        let mut a = OrSet::new();
        a.elements.insert("t1".into(), "x".to_string());
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn commutative() {
        let mut a = OrSet::new();
        a.elements.insert("t1".into(), "x".to_string());
        let mut b = OrSet::new();
        b.elements.insert("t2".into(), "y".to_string());
        b.tombstones.insert("t1".into());

        let mut x = a.clone();
        let mut y = b.clone();
        x.merge(b);
        y.merge(a);
        assert_eq!(x, y);
    }
}
