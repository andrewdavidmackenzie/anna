//! Max and Min semilattice types.

use crate::Lattice;

/// A max-semilattice: merge keeps the larger value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Max<T: Ord>(pub T);

impl<T: Ord> Lattice for Max<T> {
    fn merge(&mut self, other: Self) -> bool {
        if other.0 > self.0 {
            self.0 = other.0;
            true
        } else {
            false
        }
    }
}

/// A min-semilattice: merge keeps the smaller value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Min<T: Ord>(pub T);

impl<T: Ord> Lattice for Min<T> {
    fn merge(&mut self, other: Self) -> bool {
        if other.0 < self.0 {
            self.0 = other.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_keeps_larger() {
        let mut a = Max(3);
        assert!(a.merge(Max(5)));
        assert_eq!(a.0, 5);
    }

    #[test]
    fn max_ignores_smaller() {
        let mut a = Max(5);
        assert!(!a.merge(Max(3)));
        assert_eq!(a.0, 5);
    }

    #[test]
    fn max_idempotent() {
        let mut a = Max(5);
        assert!(!a.merge(Max(5)));
        assert_eq!(a.0, 5);
    }

    #[test]
    fn max_commutative() {
        let mut a = Max(3);
        let mut b = Max(5);
        a.merge(Max(5));
        b.merge(Max(3));
        assert_eq!(a, b);
    }

    #[test]
    fn min_keeps_smaller() {
        let mut a = Min(5);
        assert!(a.merge(Min(3)));
        assert_eq!(a.0, 3);
    }

    #[test]
    fn min_ignores_larger() {
        let mut a = Min(3);
        assert!(!a.merge(Min(5)));
        assert_eq!(a.0, 3);
    }

    #[test]
    fn min_idempotent() {
        let mut a = Min(3);
        assert!(!a.merge(Min(3)));
        assert_eq!(a.0, 3);
    }

    #[test]
    fn min_commutative() {
        let mut a = Min(3);
        let mut b = Min(5);
        a.merge(Min(5));
        b.merge(Min(3));
        assert_eq!(a, b);
    }
}
