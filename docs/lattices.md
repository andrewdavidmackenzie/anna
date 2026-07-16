# Lattices and Consistency

Lattices are central to Anna's design. They enable coordination-free consistency
by providing data structures whose merge operations are insensitive to the order
in which updates are applied.

## What is a Lattice?

A **bounded join semilattice** consists of:
- A set of possible states S
- A binary "least upper bound" operator (merge)
- A bottom value

The merge operator satisfies three properties (ACI):
- **Associativity**: merge(merge(a, b), c) = merge(a, merge(b, c))
- **Commutativity**: merge(a, b) = merge(b, a)
- **Idempotence**: merge(a, a) = a

These properties mean that replicas can receive and process updates in any order,
with any amount of duplication, and still converge to the same state.

## Lattice Types in Anna

### Last-Writer-Wins (LWW)

The simplest and default lattice type. Each value is paired with a timestamp.
The merge function keeps the value with the higher timestamp.

```
LWWPairLattice = PairLattice<MaxIntLattice, ValueLattice>
```

- Merge: if timestamp A > timestamp B, keep A; otherwise keep B
- Guarantees: eventual consistency (last write wins)
- Use case: simple key-value storage where latest value is desired

### Set Lattice

A set where the merge operation is set union.

```
SetLattice<T> = { values: Set<T> }
```

- Merge: union of both sets
- Guarantees: elements are never lost; sets only grow
- Use case: collecting tags, maintaining membership lists

### Ordered Set Lattice

Similar to Set but maintains insertion order.

### Single-Key Causal Lattice

Tracks causal relationships for a single key using a vector clock.

```
SingleKeyCausalLattice = (VectorClock, SetLattice<Value>)
```

- The vector clock tracks which client versions have been incorporated
- Merge: compare vector clocks; if one dominates, keep it; if concurrent,
  merge both values and vector clocks

### Multi-Key Causal Lattice

Extends causal consistency across multiple keys by tracking dependencies.

```
MultiKeyCausalLattice = (VectorClock, Dependencies, SetLattice<Value>)
```

- Dependencies: a map of key -> vector clock for cross-key causal relationships
- Enables causal consistency for transactions spanning multiple keys

### Priority Lattice

A pair of (priority, value) where higher priority wins.

## Lattice Composition

Anna achieves its consistency guarantees through **monotone composition** of
simple lattice building blocks, inspired by the Bloom language:

```
Key-Value Store = MapLattice<Key, PairLattice<VectorClock, ValueLattice>>
```

The private state of each worker is a `MapLattice` parameterized by key and
value types. The merge operator of `MapLattice`:
1. Takes the union of key sets from both inputs
2. For keys appearing in both, merges the values using `ValueLattice`'s merge

This compositional approach means:
- Adding a new consistency level requires changing only the lattice type
  (typically under 60 lines of C++)
- Each component's ACI properties can be verified independently
- The composed system inherits ACI properties by construction

## Consistency Levels

Anna supports a wide range of coordination-free consistency levels, all
implemented through lattice composition:

| Consistency Level | Description |
|---|---|
| **Eventual** | Default. LWW lattice. Last write wins. |
| **Causal** | Vector clocks track causal ordering between updates |
| **Read Committed** | Transaction timestamps ensure no dirty reads/writes |
| **Read Uncommitted** | Like Read Committed but allows dirty reads |
| **Item Cut Isolation** | Buffered reads ensure same value on re-read within transaction |
| **Monotonic Reads** | Once a value is read, subsequent reads return same or newer |
| **Monotonic Writes** | Writes from same client are applied in order |
| **Writes Follow Reads** | A write after a read is ordered after the read's version |
| **Read Your Writes** | A client always sees its own writes |
| **PRAM** | Pipelined RAM — combines monotonic reads, writes, and read-your-writes |

The key insight is that switching between these levels requires changing only
which lattice type wraps the value — the server code, communication protocol,
and actor model remain unchanged.

## Merge-at-Sender Optimization

For frequently-updated "hot" keys, exchanging every individual update would be
expensive. Anna exploits lattice associativity to **merge at the sender**:

Instead of sending updates {u1, u2, ..., un} individually, the sender computes
merge(u1, u2, ..., un) and sends only the single merged result. This is
equivalent because:

```
merge(s, merge(u1, u2, ..., un)) = merge(...merge(merge(s, u1), u2), ...un)
```

This dramatically reduces network overhead for hot keys.
