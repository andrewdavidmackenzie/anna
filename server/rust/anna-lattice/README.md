# anna-lattice

Pure lattice (CRDT) types for the [anna](https://github.com/andrewdavidmackenzie/anna) key-value store.

## Overview

This crate provides composable, well-tested lattice types with **no external dependencies**.
Each type implements the `Lattice` trait, which defines a join-semilattice merge operation
that is commutative, associative, and idempotent.

Lattice types are **independent of serialization** -- they carry no protobuf, serde, or
other encoding dependency. Conversion to/from wire formats belongs in the consumer
crate (e.g., `anna-kvs`).

## Types

| Type | Merge semantics | CRDT category |
|------|----------------|---------------|
| `Max<T>` | Keep the maximum value | Max-register |
| `Min<T>` | Keep the minimum value | Min-register |
| `GSet<T>` | Grow-only set (union) | G-Set |
| `LwwRegister<T>` | Higher timestamp wins | LWW-Register |
| `PriorityRegister<T>` | Lower priority wins | Priority-Register |
| `PnCounter` | Per-node max of increments/decrements | PN-Counter |
| `OrSet<T>` | Union of elements + tombstones (add wins) | OR-Set |
| `VectorClock` | Per-node max of logical clocks | Vector Clock |
| `CausalRegister<T>` | VC-based domination/concurrent merge | Causal Register |
| `MultiCausalRegister<T>` | Causal register with cross-key deps | Multi-Key Causal |
| `LatticeMap<K, V>` | Per-key lattice merge | Map of lattices |

## The `Lattice` trait

```rust
pub trait Lattice {
    /// Join another value into self. Returns `true` if self changed.
    fn merge(&mut self, other: Self) -> bool;
}
```

All implementations guarantee:
- **Commutativity**: `a.merge(b)` produces the same state as `b.merge(a)`
- **Associativity**: merging in any order yields the same result
- **Idempotency**: `a.merge(a)` is a no-op

## Usage

```rust
use anna_lattice::{Lattice, Max, GSet, LwwRegister};

// Max semilattice
let mut a = Max(3);
a.merge(Max(5));
assert_eq!(a.0, 5);

// Grow-only set
let mut s: GSet<&str> = ["a", "b"].into_iter().collect();
s.merge(["b", "c"].into_iter().collect());
assert_eq!(s.len(), 3);

// Last-writer-wins register
let mut r = LwwRegister::new(1, "old");
r.merge(LwwRegister::new(2, "new"));
assert_eq!(r.value, "new");
```
