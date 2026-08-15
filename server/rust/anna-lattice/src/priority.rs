//! Priority register: merge keeps the value with the lowest priority.

use crate::Lattice;

/// A priority register. On merge, the entry with the strictly lower
/// priority wins. Equal priorities keep the existing value.
///
/// Uses `f64` for priority. `NaN` is treated as infinity (never wins).
#[derive(Debug, Clone, PartialEq)]
pub struct PriorityRegister<T> {
    /// Priority value (lower wins).
    pub priority: f64,
    /// The value.
    pub value: T,
}

impl<T> PriorityRegister<T> {
    /// Create a new priority register.
    pub fn new(priority: f64, value: T) -> Self {
        Self { priority, value }
    }

    /// Normalize priority: NaN becomes infinity.
    fn effective_priority(&self) -> f64 {
        if self.priority.is_nan() {
            f64::INFINITY
        } else {
            self.priority
        }
    }
}

impl<T> Lattice for PriorityRegister<T> {
    fn merge(&mut self, other: Self) -> bool {
        let self_pri = self.effective_priority();
        let other_pri = other.effective_priority();
        if other_pri < self_pri {
            self.priority = other.priority;
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
    fn lower_priority_wins() {
        let mut a = PriorityRegister::new(10.0, "low");
        assert!(a.merge(PriorityRegister::new(1.0, "high")));
        assert_eq!(a.priority, 1.0);
        assert_eq!(a.value, "high");
    }

    #[test]
    fn higher_priority_ignored() {
        let mut a = PriorityRegister::new(1.0, "keep");
        assert!(!a.merge(PriorityRegister::new(10.0, "discard")));
        assert_eq!(a.value, "keep");
    }

    #[test]
    fn equal_priority_keeps_existing() {
        let mut a = PriorityRegister::new(5.0, "existing");
        assert!(!a.merge(PriorityRegister::new(5.0, "incoming")));
        assert_eq!(a.value, "existing");
    }

    #[test]
    fn nan_treated_as_infinity() {
        let mut a = PriorityRegister::new(f64::NAN, "nan");
        assert!(a.merge(PriorityRegister::new(1.0, "real")));
        assert_eq!(a.value, "real");
    }

    #[test]
    fn nan_never_wins() {
        let mut a = PriorityRegister::new(1.0, "real");
        assert!(!a.merge(PriorityRegister::new(f64::NAN, "nan")));
        assert_eq!(a.value, "real");
    }

    #[test]
    fn nan_vs_nan() {
        let mut a = PriorityRegister::new(f64::NAN, "a");
        assert!(!a.merge(PriorityRegister::new(f64::NAN, "b")));
        assert_eq!(a.value, "a");
    }

    #[test]
    fn commutative() {
        let r1 = PriorityRegister::new(3.0, "a");
        let r2 = PriorityRegister::new(1.0, "b");
        let mut a = r1.clone();
        let mut b = r2.clone();
        a.merge(r2);
        b.merge(r1);
        assert_eq!(a.priority, b.priority);
        assert_eq!(a.value, b.value);
    }
}
