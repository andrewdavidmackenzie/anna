//! A type-tagged value enum for the Anna KVS.
//!
//! [`Value`] unifies all lattice types behind a single enum so callers
//! can use `get_value` / `put_value` instead of per-type methods.

use std::collections::HashMap;
use std::fmt;

use crate::proto::kvs::LatticeType;

/// The known lattice type names accepted by the CLI and [`parse_type_name`].
pub const TYPE_NAMES: &[&str] = &[
    "lww",
    "set",
    "ordered_set",
    "lww_set",
    "lww_ordered_set",
    "union",
    "priority",
    "priority_set",
    "priority_ordered_set",
    "causal",
    "single_causal",
    "causal_set",
    "causal_ordered_set",
    "multi_causal_set",
    "multi_causal_ordered_set",
];

/// A type-tagged value from the Anna KVS.
///
/// Each variant corresponds to one of Anna's lattice types and carries
/// the deserialized payload. The [`Display`] impl formats each variant
/// in the same way the CLI has always displayed it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Last-writer-wins scalar (default merge strategy).
    Lww(String),

    /// Unordered set with union merge.
    Set(Vec<String>),

    /// Ordered set with union merge (preserves insertion order).
    OrderedSet(Vec<String>),

    /// Scalar with priority-based merge (lowest priority wins).
    Priority {
        /// The priority number (lower = wins).
        priority: f64,
        /// The value.
        value: String,
    },

    /// Scalar with single-key causal consistency.
    SingleCausal {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// The values (may contain concurrent versions).
        values: Vec<String>,
    },

    /// Last-writer-wins set: a set of values where the entire set is replaced
    /// on each write (timestamp-based), rather than merged via union.
    LwwSet(Vec<String>),

    /// Last-writer-wins ordered set: like LwwSet but preserves insertion order.
    LwwOrderedSet(Vec<String>),

    /// Priority set: the set with the lowest priority wins entirely.
    PrioritySet {
        /// The priority number (lower = wins).
        priority: f64,
        /// The set values.
        values: Vec<String>,
    },

    /// Priority ordered set: like PrioritySet but displayed in sorted order.
    PriorityOrderedSet {
        /// The priority number (lower = wins).
        priority: f64,
        /// The ordered set values.
        values: Vec<String>,
    },

    /// Single-key causal set: a set with single-key causal consistency.
    CausalSet {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// The set values (may contain concurrent versions).
        values: Vec<String>,
    },

    /// Single-key causal ordered set.
    CausalOrderedSet {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// The ordered set values.
        values: Vec<String>,
    },

    /// Multi-key causal set: a set with multi-key causal consistency.
    MultiCausalSet {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// Dependencies on other keys.
        dependencies: Vec<(String, HashMap<String, u32>)>,
        /// The set values.
        values: Vec<String>,
    },

    /// Multi-key causal ordered set.
    MultiCausalOrderedSet {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// Dependencies on other keys.
        dependencies: Vec<(String, HashMap<String, u32>)>,
        /// The ordered set values.
        values: Vec<String>,
    },

    /// Union scalar: each PUT appends a string fragment. Fragments accumulate
    /// via set union and are displayed concatenated in sorted order.
    UnionScalar(String),

    /// Scalar with multi-key causal consistency (cross-key dependencies).
    MultiCausal {
        /// The vector clock for this key.
        vector_clock: HashMap<String, u32>,
        /// Dependencies on other keys and their vector clocks.
        dependencies: Vec<(String, HashMap<String, u32>)>,
        /// The values (may contain concurrent versions).
        values: Vec<String>,
    },
}

impl Value {
    /// Return the CLI type name for this value (e.g. `"lww"`, `"set"`).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Lww(_) => "lww",
            Value::Set(_) => "set",
            Value::OrderedSet(_) => "ordered_set",
            Value::LwwSet(_) => "lww_set",
            Value::LwwOrderedSet(_) => "lww_ordered_set",
            Value::UnionScalar(_) => "union",
            Value::Priority { .. } => "priority",
            Value::PrioritySet { .. } => "priority_set",
            Value::PriorityOrderedSet { .. } => "priority_ordered_set",
            Value::SingleCausal { .. } => "single_causal",
            Value::CausalSet { .. } => "causal_set",
            Value::CausalOrderedSet { .. } => "causal_ordered_set",
            Value::MultiCausal { .. } => "causal",
            Value::MultiCausalSet { .. } => "multi_causal_set",
            Value::MultiCausalOrderedSet { .. } => "multi_causal_ordered_set",
        }
    }

    /// Return the protobuf `LatticeType` enum value for this value.
    pub(crate) fn lattice_type(&self) -> LatticeType {
        match self {
            Value::Lww(_) => LatticeType::Lww,
            Value::Set(_) => LatticeType::Set,
            Value::OrderedSet(_) => LatticeType::OrderedSet,
            Value::LwwSet(_) => LatticeType::LwwSet,
            Value::LwwOrderedSet(_) => LatticeType::LwwOrderedSet,
            Value::UnionScalar(_) => LatticeType::UnionScalar,
            Value::Priority { .. } => LatticeType::Priority,
            Value::PrioritySet { .. } => LatticeType::PrioritySet,
            Value::PriorityOrderedSet { .. } => LatticeType::PriorityOrderedSet,
            Value::SingleCausal { .. } => LatticeType::SingleCausal,
            Value::CausalSet { .. } => LatticeType::CausalSet,
            Value::CausalOrderedSet { .. } => LatticeType::CausalOrderedSet,
            Value::MultiCausal { .. } => LatticeType::MultiCausal,
            Value::MultiCausalSet { .. } => LatticeType::MultiCausalSet,
            Value::MultiCausalOrderedSet { .. } => LatticeType::MultiCausalOrderedSet,
        }
    }
}

/// Parse a CLI type name into a protobuf [`LatticeType`].
///
/// Returns `None` if the name is not recognized.
pub fn parse_type_name(name: &str) -> Option<LatticeType> {
    match name.to_ascii_lowercase().as_str() {
        "lww" => Some(LatticeType::Lww),
        "set" => Some(LatticeType::Set),
        "ordered_set" => Some(LatticeType::OrderedSet),
        "lww_set" => Some(LatticeType::LwwSet),
        "lww_ordered_set" => Some(LatticeType::LwwOrderedSet),
        "union" => Some(LatticeType::UnionScalar),
        "priority" => Some(LatticeType::Priority),
        "priority_set" => Some(LatticeType::PrioritySet),
        "priority_ordered_set" => Some(LatticeType::PriorityOrderedSet),
        "causal" => Some(LatticeType::MultiCausal),
        "single_causal" => Some(LatticeType::SingleCausal),
        "causal_set" => Some(LatticeType::CausalSet),
        "causal_ordered_set" => Some(LatticeType::CausalOrderedSet),
        "multi_causal_set" => Some(LatticeType::MultiCausalSet),
        "multi_causal_ordered_set" => Some(LatticeType::MultiCausalOrderedSet),
        _ => None,
    }
}

/// Format a vector clock as `{key : value}` pairs, sorted by key.
fn format_vector_clock(vc: &HashMap<String, u32>) -> String {
    let mut sorted: Vec<_> = vc.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_string());
    sorted
        .iter()
        .map(|(k, v)| format!("{{{} : {}}}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Lww(s) => write!(f, "{}", s),
            Value::Set(values) => {
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "{{ {} }}", sorted.join(" "))
            }
            Value::LwwSet(values) => {
                // Sort for deterministic display (same as union Set).
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "{{ {} }}", sorted.join(" "))
            }
            Value::LwwOrderedSet(values) => {
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "[ {} ]", sorted.join(" "))
            }
            Value::PrioritySet { priority, values } => {
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "priority: {}\n{{ {} }}", priority, sorted.join(" "))
            }
            Value::PriorityOrderedSet { priority, values } => {
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "priority: {}\n[ {} ]", priority, sorted.join(" "))
            }
            Value::CausalSet {
                vector_clock,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "\n{{ {} }}", sorted.join(" "))
            }
            Value::CausalOrderedSet {
                vector_clock,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "\n[ {} ]", sorted.join(" "))
            }
            Value::MultiCausalSet {
                vector_clock,
                dependencies,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                let mut sorted_deps = dependencies.clone();
                sorted_deps.sort_by(|(a, _), (b, _)| a.cmp(b));
                for (dep_key, dep_vc) in &sorted_deps {
                    let mut sorted_vc: Vec<_> = dep_vc.iter().collect();
                    sorted_vc.sort_by_key(|(k, _)| k.to_string());
                    let vc_str: Vec<String> = sorted_vc
                        .iter()
                        .map(|(k, v)| format!("{{{} : {}}}", k, v))
                        .collect();
                    write!(f, "\n{} : {}", dep_key, vc_str.join(" "))?;
                }
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "\n{{ {} }}", sorted.join(" "))
            }
            Value::MultiCausalOrderedSet {
                vector_clock,
                dependencies,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                let mut sorted_deps = dependencies.clone();
                sorted_deps.sort_by(|(a, _), (b, _)| a.cmp(b));
                for (dep_key, dep_vc) in &sorted_deps {
                    let mut sorted_vc: Vec<_> = dep_vc.iter().collect();
                    sorted_vc.sort_by_key(|(k, _)| k.to_string());
                    let vc_str: Vec<String> = sorted_vc
                        .iter()
                        .map(|(k, v)| format!("{{{} : {}}}", k, v))
                        .collect();
                    write!(f, "\n{} : {}", dep_key, vc_str.join(" "))?;
                }
                let mut sorted = values.clone();
                sorted.sort();
                write!(f, "\n[ {} ]", sorted.join(" "))
            }
            Value::UnionScalar(s) => write!(f, "{}", s),
            Value::OrderedSet(values) => {
                write!(f, "[ {} ]", values.join(" "))
            }
            Value::Priority { priority, value } => {
                write!(f, "priority: {}\n{}", priority, value)
            }
            Value::SingleCausal {
                vector_clock,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                for v in values {
                    write!(f, "\n{}", v)?;
                }
                Ok(())
            }
            Value::MultiCausal {
                vector_clock,
                dependencies,
                values,
            } => {
                write!(f, "{}", format_vector_clock(vector_clock))?;
                let mut sorted_deps = dependencies.clone();
                sorted_deps.sort_by(|(a, _), (b, _)| a.cmp(b));
                for (dep_key, dep_vc) in &sorted_deps {
                    let mut sorted_vc: Vec<_> = dep_vc.iter().collect();
                    sorted_vc.sort_by_key(|(k, _)| k.to_string());
                    let vc_str: Vec<String> = sorted_vc
                        .iter()
                        .map(|(k, v)| format!("{{{} : {}}}", k, v))
                        .collect();
                    write!(f, "\n{} : {}", dep_key, vc_str.join(" "))?;
                }
                for v in values {
                    write!(f, "\n{}", v)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_lww() {
        let v = Value::Lww("hello".into());
        assert_eq!(v.to_string(), "hello");
        assert_eq!(v.type_name(), "lww");
    }

    #[test]
    fn display_set() {
        let v = Value::Set(vec!["z".into(), "a".into(), "m".into()]);
        assert_eq!(v.to_string(), "{ a m z }");
        assert_eq!(v.type_name(), "set");
    }

    #[test]
    fn display_ordered_set() {
        let v = Value::OrderedSet(vec!["alpha".into(), "beta".into(), "gamma".into()]);
        assert_eq!(v.to_string(), "[ alpha beta gamma ]");
        assert_eq!(v.type_name(), "ordered_set");
    }

    #[test]
    fn display_priority() {
        let v = Value::Priority {
            priority: 1.5,
            value: "important".into(),
        };
        assert_eq!(v.to_string(), "priority: 1.5\nimportant");
        assert_eq!(v.type_name(), "priority");
    }

    #[test]
    fn display_single_causal() {
        let mut vc = HashMap::new();
        vc.insert("test".into(), 1);
        let v = Value::SingleCausal {
            vector_clock: vc,
            values: vec!["world".into()],
        };
        assert_eq!(v.to_string(), "{test : 1}\nworld");
        assert_eq!(v.type_name(), "single_causal");
    }

    #[test]
    fn display_multi_causal() {
        let mut vc = HashMap::new();
        vc.insert("test".into(), 1);
        let mut dep_vc = HashMap::new();
        dep_vc.insert("test1".into(), 1);
        let v = Value::MultiCausal {
            vector_clock: vc,
            dependencies: vec![("dep1".into(), dep_vc)],
            values: vec!["hello".into()],
        };
        assert_eq!(v.to_string(), "{test : 1}\ndep1 : {test1 : 1}\nhello");
        assert_eq!(v.type_name(), "causal");
    }

    #[test]
    fn display_lww_set() {
        let v = Value::LwwSet(vec!["x".into(), "y".into(), "z".into()]);
        assert_eq!(v.to_string(), "{ x y z }");
        assert_eq!(v.type_name(), "lww_set");
    }

    #[test]
    fn lww_set_sorts_for_display() {
        // LWW sets sort for deterministic display (same as union sets).
        let v = Value::LwwSet(vec!["z".into(), "a".into(), "m".into()]);
        assert_eq!(v.to_string(), "{ a m z }");
    }

    #[test]
    fn display_lww_ordered_set() {
        let v = Value::LwwOrderedSet(vec!["c".into(), "b".into(), "a".into()]);
        assert_eq!(v.to_string(), "[ a b c ]");
        assert_eq!(v.type_name(), "lww_ordered_set");
    }

    #[test]
    fn display_priority_set() {
        let v = Value::PrioritySet {
            priority: 1.5,
            values: vec!["b".into(), "a".into()],
        };
        assert_eq!(v.to_string(), "priority: 1.5\n{ a b }");
        assert_eq!(v.type_name(), "priority_set");
    }

    #[test]
    fn display_causal_set() {
        let mut vc = HashMap::new();
        vc.insert("test".into(), 1);
        let v = Value::CausalSet {
            vector_clock: vc,
            values: vec!["y".into(), "x".into()],
        };
        assert_eq!(v.to_string(), "{test : 1}\n{ x y }");
        assert_eq!(v.type_name(), "causal_set");
    }

    #[test]
    fn display_priority_ordered_set() {
        let v = Value::PriorityOrderedSet {
            priority: 2.0,
            values: vec!["c".into(), "a".into()],
        };
        assert_eq!(v.to_string(), "priority: 2\n[ a c ]");
    }

    #[test]
    fn display_causal_ordered_set() {
        let mut vc = HashMap::new();
        vc.insert("n".into(), 1);
        let v = Value::CausalOrderedSet {
            vector_clock: vc,
            values: vec!["z".into(), "a".into()],
        };
        assert_eq!(v.to_string(), "{n : 1}\n[ a z ]");
    }

    #[test]
    fn display_multi_causal_set() {
        let mut vc = HashMap::new();
        vc.insert("n".into(), 1);
        let mut dvc = HashMap::new();
        dvc.insert("d".into(), 2);
        let v = Value::MultiCausalSet {
            vector_clock: vc,
            dependencies: vec![("dep".into(), dvc)],
            values: vec!["b".into(), "a".into()],
        };
        assert!(v.to_string().contains("{ a b }"));
    }

    #[test]
    fn display_multi_causal_ordered_set() {
        let mut vc = HashMap::new();
        vc.insert("n".into(), 1);
        let v = Value::MultiCausalOrderedSet {
            vector_clock: vc,
            dependencies: vec![],
            values: vec!["y".into(), "x".into()],
        };
        assert!(v.to_string().contains("[ x y ]"));
    }

    #[test]
    fn parse_new_type_names() {
        assert_eq!(
            parse_type_name("priority_set"),
            Some(LatticeType::PrioritySet)
        );
        assert_eq!(
            parse_type_name("priority_ordered_set"),
            Some(LatticeType::PriorityOrderedSet)
        );
        assert_eq!(parse_type_name("causal_set"), Some(LatticeType::CausalSet));
        assert_eq!(
            parse_type_name("causal_ordered_set"),
            Some(LatticeType::CausalOrderedSet)
        );
        assert_eq!(
            parse_type_name("multi_causal_set"),
            Some(LatticeType::MultiCausalSet)
        );
        assert_eq!(
            parse_type_name("multi_causal_ordered_set"),
            Some(LatticeType::MultiCausalOrderedSet)
        );
    }

    #[test]
    fn display_union_scalar() {
        let v = Value::UnionScalar("hello world".into());
        assert_eq!(v.to_string(), "hello world");
        assert_eq!(v.type_name(), "union");
    }

    #[test]
    fn parse_type_name_valid() {
        assert_eq!(parse_type_name("lww"), Some(LatticeType::Lww));
        assert_eq!(parse_type_name("set"), Some(LatticeType::Set));
        assert_eq!(
            parse_type_name("ordered_set"),
            Some(LatticeType::OrderedSet)
        );
        assert_eq!(parse_type_name("lww_set"), Some(LatticeType::LwwSet));
        assert_eq!(
            parse_type_name("lww_ordered_set"),
            Some(LatticeType::LwwOrderedSet)
        );
        assert_eq!(parse_type_name("union"), Some(LatticeType::UnionScalar));
        assert_eq!(parse_type_name("priority"), Some(LatticeType::Priority));
        assert_eq!(parse_type_name("causal"), Some(LatticeType::MultiCausal));
        assert_eq!(
            parse_type_name("single_causal"),
            Some(LatticeType::SingleCausal)
        );
    }

    #[test]
    fn parse_type_name_case_insensitive() {
        assert_eq!(parse_type_name("LWW"), Some(LatticeType::Lww));
        assert_eq!(parse_type_name("SET"), Some(LatticeType::Set));
        assert_eq!(parse_type_name("Causal"), Some(LatticeType::MultiCausal));
    }

    #[test]
    fn parse_type_name_unknown() {
        assert_eq!(parse_type_name("unknown"), None);
        assert_eq!(parse_type_name(""), None);
    }

    #[test]
    fn lattice_type_roundtrip() {
        let v = Value::Set(vec!["a".into()]);
        assert_eq!(v.lattice_type(), LatticeType::Set);
    }
}
