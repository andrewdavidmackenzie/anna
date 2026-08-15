//! Grow-only set (G-Set): merge is set union.

use crate::Lattice;
use std::collections::BTreeSet;

/// A grow-only set. Elements can be added but never removed.
/// Merge is set union.
///
/// Uses `BTreeSet` for deterministic iteration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GSet<T: Ord>(pub BTreeSet<T>);

impl<T: Ord> GSet<T> {
    /// Create an empty G-Set.
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Insert an element.
    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert(value)
    }

    /// Check if the set contains a value.
    pub fn contains(&self, value: &T) -> bool {
        self.0.contains(value)
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over elements in sorted order.
    pub fn iter(&self) -> std::collections::btree_set::Iter<'_, T> {
        self.0.iter()
    }
}

impl<T: Ord> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> Lattice for GSet<T> {
    fn merge(&mut self, other: Self) -> bool {
        let before = self.0.len();
        self.0.extend(other.0);
        self.0.len() > before
    }
}

impl<T: Ord> FromIterator<T> for GSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_merge() {
        let mut a: GSet<&str> = ["a", "b"].into_iter().collect();
        let b: GSet<&str> = ["b", "c"].into_iter().collect();
        assert!(a.merge(b));
        assert_eq!(a.len(), 3);
        assert!(a.contains(&"a"));
        assert!(a.contains(&"b"));
        assert!(a.contains(&"c"));
    }

    #[test]
    fn idempotent() {
        let mut a: GSet<i32> = [1, 2, 3].into_iter().collect();
        let b = a.clone();
        assert!(!a.merge(b));
        assert_eq!(a.len(), 3);
    }

    #[test]
    fn commutative() {
        let set1: GSet<i32> = [1, 2].into_iter().collect();
        let set2: GSet<i32> = [2, 3].into_iter().collect();
        let mut a = set1.clone();
        let mut b = set2.clone();
        a.merge(set2);
        b.merge(set1);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_merge() {
        let mut a: GSet<i32> = [1].into_iter().collect();
        assert!(!a.merge(GSet::new()));
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn merge_into_empty() {
        let mut a: GSet<i32> = GSet::new();
        assert!(a.merge([1, 2].into_iter().collect()));
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn deterministic_order() {
        let a: GSet<&str> = ["c", "a", "b"].into_iter().collect();
        let items: Vec<_> = a.iter().copied().collect();
        assert_eq!(items, vec!["a", "b", "c"]);
    }
}
