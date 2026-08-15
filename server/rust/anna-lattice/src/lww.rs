//! Last-Writer-Wins register: merge keeps the value with the higher timestamp.

use crate::Lattice;

/// A Last-Writer-Wins register. On merge, the entry with the strictly
/// higher timestamp wins. Equal timestamps keep the existing value
/// (unlike the C++ implementation which lets the incoming value win on ties).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LwwRegister<T> {
    /// Monotonic timestamp (e.g., wall clock or logical clock).
    pub timestamp: u64,
    /// The value.
    pub value: T,
}

impl<T> LwwRegister<T> {
    /// Create a new LWW register.
    pub fn new(timestamp: u64, value: T) -> Self {
        Self { timestamp, value }
    }
}

impl<T> Lattice for LwwRegister<T> {
    fn merge(&mut self, other: Self) -> bool {
        if other.timestamp > self.timestamp {
            self.timestamp = other.timestamp;
            self.value = other.value;
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
    fn higher_timestamp_wins() {
        let mut a = LwwRegister::new(1, "old");
        assert!(a.merge(LwwRegister::new(2, "new")));
        assert_eq!(a.value, "new");
        assert_eq!(a.timestamp, 2);
    }

    #[test]
    fn lower_timestamp_ignored() {
        let mut a = LwwRegister::new(5, "keep");
        assert!(!a.merge(LwwRegister::new(3, "discard")));
        assert_eq!(a.value, "keep");
        assert_eq!(a.timestamp, 5);
    }

    #[test]
    fn equal_timestamp_keeps_existing() {
        let mut a = LwwRegister::new(5, "existing");
        assert!(!a.merge(LwwRegister::new(5, "incoming")));
        assert_eq!(a.value, "existing");
    }

    #[test]
    fn idempotent() {
        let mut a = LwwRegister::new(3, vec![1, 2, 3]);
        let b = a.clone();
        assert!(!a.merge(b));
    }

    #[test]
    fn commutative() {
        let r1 = LwwRegister::new(1, "a");
        let r2 = LwwRegister::new(2, "b");
        let mut a = r1.clone();
        let mut b = r2.clone();
        a.merge(r2);
        b.merge(r1);
        assert_eq!(a, b);
    }
}
