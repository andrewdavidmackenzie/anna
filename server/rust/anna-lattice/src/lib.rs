//! Pure lattice (CRDT) types for the anna key-value store.
//!
//! This crate provides composable, well-tested lattice types with no
//! external dependencies. Each type implements the [`Lattice`] trait,
//! which defines a join-semilattice merge operation.
//!
//! Lattice types are **independent of serialization** -- they carry no
//! protobuf, serde, or other encoding dependency. Conversion to/from
//! wire formats belongs in the consumer crate (e.g., `anna-kvs`).
//!
//! # Types
//!
//! | Type | Merge semantics |
//! |------|----------------|
//! | [`Max<T>`] | Keep the maximum value |
//! | [`Min<T>`] | Keep the minimum value |
//! | [`GSet<T>`] | Grow-only set (union) |
//! | [`LwwRegister<T>`] | Last-writer-wins by timestamp |
//! | [`PriorityRegister<T>`] | Lowest priority wins |
//! | [`PnCounter`] | Per-node max of increments/decrements |
//! | [`OrSet<T>`] | Observed-remove set (add wins) |
//! | [`VectorClock`] | Per-node max of logical clocks |
//! | [`CausalRegister<T>`] | Vector-clock causal register |
//! | [`MultiCausalRegister<T>`] | Causal register with cross-key deps |
//! | [`LatticeMap<K, V>`] | Map with per-key lattice merge |

mod causal;
mod counter;
mod gset;
mod lattice_map;
mod lww;
mod max_min;
mod or_set;
mod priority;
mod vector_clock;

pub use causal::{CausalRegister, MultiCausalRegister};
pub use counter::PnCounter;
pub use gset::GSet;
pub use lattice_map::LatticeMap;
pub use lww::LwwRegister;
pub use max_min::{Max, Min};
pub use or_set::OrSet;
pub use priority::PriorityRegister;
pub use vector_clock::VectorClock;

/// A join-semilattice: a type with a commutative, associative, idempotent
/// merge (join) operation.
///
/// Implementors must guarantee:
/// - **Commutativity**: `a.merge(b)` produces the same state as `b.merge(a)`
/// - **Associativity**: merging in any order yields the same result
/// - **Idempotency**: `a.merge(a)` is a no-op
///
/// The `merge` method returns `true` if `self` was modified.
pub trait Lattice {
    /// Join another value into self. Returns `true` if self changed.
    fn merge(&mut self, other: Self) -> bool;
}
