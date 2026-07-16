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

### Consistency Level Details

#### Eventual Consistency (LWW)

- **Definition**: All replicas eventually converge to the same value. Concurrent
  writes are resolved by timestamp (last writer wins).
- **Implementation**: `LWWPairLattice` in the server's storage kernel. The merge
  function compares timestamps and keeps the higher one.
- **Where**: Server (`server/cpp/src/kvs/`), all client libraries.
- **Use case**: Session data, user preferences, caches — any scenario where the
  most recent write is the correct one and brief inconsistency is acceptable.
- **Comparison**: Similar to DynamoDB's default eventual consistency, Cassandra
  with `CONSISTENCY ONE`, and Redis replication.

#### Causal Consistency

- **Definition**: If operation A causally precedes operation B (A happened before
  B, or B read A's result), then all replicas observe A before B. Concurrent
  operations (neither caused the other) may be observed in any order.
- **Implementation**: `MultiKeyCausalLattice` composed of a `VectorClock`
  (MapLattice of client IDs to MaxIntLattice counters), a dependency map, and
  a `SetLattice` of values. The merge function compares vector clocks: if one
  dominates, it wins; if concurrent, both values are kept.
- **Where**: Server (`server/cpp/src/kvs/`), client libraries (`GET_CAUSAL`/
  `PUT_CAUSAL` commands).
- **Use case**: Social media feeds (a reply should appear after the post it
  replies to), collaborative editing, distributed caches where causal ordering
  matters.
- **Comparison**: COPS and Bolt-on provide causal consistency in distributed
  settings. MongoDB offers causal consistency sessions. Anna's implementation
  is unique in being coordination-free — no consensus protocol or total ordering.

#### Read Committed

- **Definition**: Transactions only see committed data. No dirty reads (reading
  uncommitted writes) and no dirty writes (overwriting uncommitted data).
- **Implementation**: Appends a transaction timestamp to each write using a
  `MaxIntLattice`. The merge function keeps the value with the higher
  transaction timestamp, ensuring writes within a transaction are atomic.
  Dirty reads are prevented by buffering writes at the client proxy until
  commit time.
- **Where**: Client proxy logic (transaction buffering).
- **Use case**: Banking transactions, inventory management — anywhere partial
  transaction results must not be visible.
- **Comparison**: PostgreSQL's default isolation level. Most SQL databases
  support this. Anna achieves it without coordination through lattice composition.

#### Read Uncommitted

- **Definition**: Like Read Committed but allows dirty reads (seeing uncommitted
  data from other transactions).
- **Implementation**: Same lattice as Read Committed but without client-side
  write buffering.
- **Where**: Client proxy logic.
- **Use case**: Analytics queries where approximate results are acceptable and
  performance is prioritized over strict correctness.

#### Item Cut Isolation

- **Definition**: Within a transaction, reading the same key twice returns the
  same value (repeatable read for individual keys).
- **Implementation**: The client proxy caches read values for the duration of
  the transaction. Subsequent reads of the same key return the cached value.
  No modification to the lattice composition is needed.
- **Where**: Client proxy (read cache).
- **Use case**: Report generation where consistency within a single query
  matters, even if the data changes between queries.

#### Monotonic Reads

- **Definition**: Once a client reads a value, subsequent reads return the same
  or a newer value — never an older one.
- **Implementation**: The base eventual consistency lattice already guarantees
  this because lattice merge is monotonically increasing.
- **Where**: Inherent in the lattice design.
- **Use case**: Newsfeeds, notification systems — users should never see items
  disappear.

#### Monotonic Writes

- **Definition**: Writes from the same client are applied in the order they
  were issued.
- **Implementation**: Inherent in the LWW lattice with monotonically increasing
  timestamps.
- **Where**: Inherent in the lattice design.
- **Use case**: Logging, audit trails — events must appear in order.

#### Writes Follow Reads

- **Definition**: A write that follows a read is ordered after the version that
  was read.
- **Implementation**: The client includes the version it read in its write
  request. The lattice merge ensures the write is ordered after the read.
- **Where**: Client proxy + lattice composition.
- **Use case**: Comment systems (a reply is always ordered after the post
  it responds to).

#### PRAM (Pipelined RAM)

- **Definition**: Combines monotonic reads, monotonic writes, and read-your-writes.
  All operations from a single client are seen by all replicas in the order
  the client issued them.
- **Implementation**: Composition of the lattices for the three constituent
  properties.
- **Where**: Client proxy + lattice composition.
- **Use case**: Interactive applications where a user expects to see their own
  actions reflected immediately and in order.

### Comparison with Other Systems

| System | Per-Key Consistency | Multi-Key Consistency |
|---|---|---|
| **Anna** | Eventual, Causal, Item Cut, Monotonic R/W, PRAM, ... | Read Committed, Read Uncommitted |
| **Redis** | Linearizable | Serializable |
| **MongoDB** | Linearizable | Linearizable |
| **etcd** | Linearizable (Raft consensus) | Serializable |
| **DynamoDB** | Linearizable, Eventual | None |
| **Cassandra** | Linearizable, Eventual | None |
| **CockroachDB** | Serializable | Serializable |

Anna is unique in offering the widest range of coordination-free consistency
levels. Stronger levels (linearizability, serializability) require coordination
and are outside Anna's design goals — Anna trades those for performance and
scalability. Systems like Redis and etcd offer strong consistency but cannot
scale horizontally without coordination overhead.

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
