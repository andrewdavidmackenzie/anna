//! Lattice Map: a map where values are merged per-key using lattice semantics.

use crate::Lattice;
use std::collections::HashMap;
use std::hash::Hash;

/// A map whose values are lattices. On merge, existing keys have their
/// values merged via the value lattice's `merge()`. New keys are inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatticeMap<K: Eq + Hash, V: Lattice>(pub HashMap<K, V>);

impl<K: Eq + Hash, V: Lattice> LatticeMap<K, V> {
    /// Create an empty lattice map.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Insert a key-value pair, merging if the key already exists.
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(existing) = self.0.get_mut(&key) {
            existing.merge(value);
        } else {
            self.0.insert(key, value);
        }
    }

    /// Get a reference to a value.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<K: Eq + Hash, V: Lattice> Default for LatticeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash, V: Lattice + PartialEq> Lattice for LatticeMap<K, V> {
    fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for (key, value) in other.0 {
            if let Some(existing) = self.0.get_mut(&key) {
                if existing.merge(value) {
                    changed = true;
                }
            } else {
                self.0.insert(key, value);
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Max;

    #[test]
    fn per_key_merge() {
        let mut a = LatticeMap::new();
        a.insert("k1", Max(3));
        a.insert("k2", Max(5));

        let mut b = LatticeMap::new();
        b.insert("k1", Max(7));
        b.insert("k3", Max(1));

        assert!(a.merge(b));
        assert_eq!(a.get(&"k1").unwrap().0, 7);
        assert_eq!(a.get(&"k2").unwrap().0, 5);
        assert_eq!(a.get(&"k3").unwrap().0, 1);
    }

    #[test]
    fn idempotent() {
        let mut a = LatticeMap::new();
        a.insert("k1", Max(3));
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn commutative() {
        let mut m1 = LatticeMap::new();
        m1.insert("k1", Max(3));
        let mut m2 = LatticeMap::new();
        m2.insert("k1", Max(5));
        m2.insert("k2", Max(1));

        let mut a = m1.clone();
        let mut b = m2.clone();
        a.merge(m2);
        b.merge(m1);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_merge() {
        let mut a: LatticeMap<&str, Max<i32>> = LatticeMap::new();
        a.insert("k1", Max(3));
        assert!(!a.merge(LatticeMap::new()));
    }

    #[test]
    fn insert_merges_existing() {
        let mut m = LatticeMap::new();
        m.insert("k1", Max(3));
        m.insert("k1", Max(7));
        assert_eq!(m.get(&"k1").unwrap().0, 7);
    }
}
