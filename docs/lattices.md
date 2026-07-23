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
simple lattice building blocks:

```
Key-Value Store = MapLattice<Key, PairLattice<VectorClock, ValueLattice>>
```

The private state of each worker is a `MapLattice` parameterized by `Key` and
value types. The merge operator of `MapLattice`:
1. Takes the union of key sets from both inputs
2. For keys appearing in both, merges the values using `ValueLattice`'s merge

This compositional approach means:
- Adding a new consistency level requires changing only the lattice type
  (typically under 60 lines of C++)
- Each component's ACI properties can be verified independently
- The composed system inherits ACI properties by construction

## Consistency Levels

### Implemented

Anna provides six lattice types with full implementations (lattice code,
protobuf types, server handlers, serializers, and client APIs in all
four languages):

| Consistency Level       | Lattice Type     | Description                                       |
|-------------------------|------------------|---------------------------------------------------|
| **Eventual (LWW)**      | `LWW`            | Last write wins by timestamp                      |
| **Set merge**           | `SET`             | Grow-only set with union merge                   |
| **Ordered set merge**   | `ORDERED_SET`     | Set with insertion order preserved               |
| **Single-key causal**   | `SINGLE_CAUSAL`   | Vector clocks track causal ordering per key      |
| **Multi-key causal**    | `MULTI_CAUSAL`    | Causal ordering with cross-key dependency tracking |
| **Priority**            | `PRIORITY`        | Lowest priority value wins                       |

### Theoretical (not yet implemented)

The VLDB 2019 paper (Section 4.2.1) notes that Anna's lattices "can be
composed to offer the full range of coordination-free consistency
guarantees." The following levels are theoretically achievable through
lattice composition but have **no implementation** in the current codebase
— no transactions, no session state, no write buffering, and no
version-tracking protocol exist:

| Consistency Level       | What would be needed                                                   |
|-------------------------|------------------------------------------------------------------------|
| **Read Committed**      | Client-side transaction buffering + commit protocol                    |
| **Read Uncommitted**    | Same as Read Committed without write buffering                         |
| **Item Cut Isolation**  | Client-side read cache per transaction                                 |
| **Writes Follow Reads** | Client includes read version in write requests                         |
| **PRAM**                | Composition of monotonic reads + monotonic writes + read-your-writes   |

### Emergent (no explicit enforcement)

These properties are partially emergent from the existing lattice design
but are **not enforced across replicas** during gossip convergence:

| Consistency Level       | Status                                                                 |
|-------------------------|------------------------------------------------------------------------|
| **Monotonic Reads**     | Holds on a single replica; stale reads possible across replicas        |
| **Monotonic Writes**    | Emergent from LWW timestamps; not a separate implementation            |
| **Read Your Writes**    | Holds when reading from the same replica that accepted the write       |

### Consistency Level Details

#### Eventual Consistency (LWW)

- **Definition**: All replicas eventually converge to the same value. Concurrent
  writes are resolved by timestamp (last writer wins).
- **Implementation**: `LWWPairLattice` in the server's storage kernel. The merge
  function compares timestamps and keeps the higher one.
- **Where**: Server (`server/cpp/src/kvs/`), all client libraries.

```rust
// Rust example
client.put("key", "value1").await?;
client.put("key", "value2").await?; // later timestamp wins
let val = client.get("key").await?;
assert_eq!(val, "value2");
```
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

```rust
// Rust: Single-key causal
client.put_single_causal("key", "value").await?;
let (vector_clock, values) = client.get_single_causal("key").await?;
// Concurrent writes (independent vector clocks) keep both values

// Rust: Multi-key causal (tracks cross-key dependencies)
client.put_causal("key", "value").await?;
let (vector_clock, dependencies, value) = client.get_causal("key").await?;
```

```rust
// Rust: Set (union merge)
client.put_set("key", &["a", "b"]).await?;
client.put_set("key", &["b", "c"]).await?;
let values = client.get_set("key").await?; // ["a", "b", "c"]

// Rust: Ordered set
client.put_ordered_set("key", &["x", "y", "z"]).await?;
let values = client.get_ordered_set("key").await?;

// Rust: Priority (lowest wins)
client.put_priority("key", 10.0, "low_priority").await?;
client.put_priority("key", 1.0, "high_priority").await?;
let (priority, value) = client.get_priority("key").await?;
assert_eq!(priority, 1.0); // lowest priority wins
```
- **Use case**: Social media feeds (a reply should appear after the post it
  replies to), collaborative editing, distributed caches where causal ordering
  matters.
- **Comparison**: COPS and Bolt-on provide causal consistency in distributed
  settings. MongoDB offers causal consistency sessions. Anna's implementation
  is unique in being coordination-free — no consensus protocol or total ordering.

#### Theoretical Consistency Levels (Not Yet Implemented)

The following consistency levels are described in the academic literature
as achievable through lattice composition (see Bailis et al., "Coordination
Avoidance in Database Systems", VLDB 2015). Anna's architecture could
support them, but no implementation exists in this codebase.

- **Read Committed / Read Uncommitted**: Would require client-side
  transaction buffering and a commit protocol.
- **Item Cut Isolation**: Would require a client-side read cache per
  transaction.
- **Writes Follow Reads**: Would require the client to include the read
  version in write requests.
- **PRAM**: Would compose monotonic reads, monotonic writes, and
  read-your-writes — each of which would need explicit enforcement.

**Emergent properties** (not enforced, but partially hold):
- **Monotonic Reads**: Holds on a single replica because lattice merge is
  monotonically increasing. Not guaranteed across replicas during gossip
  convergence (stale reads are possible).
- **Monotonic Writes**: Emergent from LWW's monotonically increasing
  timestamps. Not a separate implementation.
- **Read Your Writes**: Holds when reading from the same replica that
  accepted the write. Not guaranteed across replicas.

### Stronger Consistency Levels (Not Supported by Anna)

Anna deliberately does not implement the two strongest consistency levels.
Understanding what they offer — and what they cost — clarifies why Anna
makes this trade-off.

#### Linearizability

- **Definition**: Every operation appears to take effect instantaneously at some
  point between its invocation and completion. All clients observe the same
  order of operations, and that order is consistent with real-time ordering.
  In other words, once a write completes, every subsequent read (by any client)
  returns that value or a newer one.
- **What it offers over Anna's levels**: Linearizability is the strongest
  single-key guarantee. It eliminates stale reads entirely — a client can never
  read an old value after a newer one has been written. Anna's eventual
  consistency allows stale reads bounded by the gossip epoch (up to 10 seconds).
  Even Anna's causal consistency only guarantees ordering between causally
  related operations; concurrent operations may be observed in any order.
- **Cost**: Requires coordination on every write — typically a consensus
  protocol (Paxos, Raft) or quorum reads/writes. This coordination becomes
  the throughput bottleneck: every write must wait for a majority of replicas
  to acknowledge before completing. Under high contention, this means threads
  spend most of their time waiting rather than doing useful work.
- **Systems that offer it**: Redis, MongoDB, etcd, DynamoDB (optional),
  Cassandra (with quorum), CockroachDB.

#### Serializability

- **Definition**: The result of executing a set of transactions is equivalent
  to some serial (one-at-a-time) execution of those transactions. This is the
  strongest multi-key guarantee — it ensures that concurrent transactions
  behave as if they ran sequentially, even when they touch different keys.
- **What it offers over Anna's levels**: Anna's Read Committed prevents dirty
  reads/writes but allows non-repeatable reads and phantom reads across keys.
  Serializability prevents all anomalies: if transaction T1 reads key A and
  writes key B, and transaction T2 reads key B and writes key A, serializability
  guarantees a consistent outcome as if one ran before the other. Anna has no
  mechanism to enforce this across keys without coordination.
- **Cost**: Requires either two-phase locking (pessimistic, blocks concurrent
  access) or optimistic concurrency control with validation (aborts conflicting
  transactions). Both approaches require global coordination that fundamentally
  limits throughput. Two-phase commit for distributed transactions adds
  additional latency and failure modes.
- **Systems that offer it**: Redis (single-threaded, trivially serializable),
  H-Store, CockroachDB, etcd (via Raft log ordering).

#### Why Anna Doesn't Support These

Anna's design is built on the principle that coordination is the enemy of
scalability. Both linearizability and serializability require coordination
on every operation:

- **Linearizability** requires consensus or quorum acknowledgment per write,
  introducing latency proportional to network round-trip time and limiting
  throughput to the speed of the slowest replica.
- **Serializability** requires global ordering of transactions, either via
  locks (which create contention hotspots) or via a total-order broadcast
  protocol (which limits throughput to a single serialization point).

Anna's benchmarks show that coordination-based systems (TBB hash map, Masstree)
spend 92-95% of CPU time on atomic instructions under high contention, while
Anna spends 90%+ on useful request handling. This is the fundamental trade-off:
Anna sacrifices the strongest consistency guarantees in exchange for orders of
magnitude better performance and seamless horizontal scalability.

For applications that need linearizability or serializability, systems like
CockroachDB, etcd, or Redis are more appropriate — but they cannot match
Anna's throughput or cost-efficiency at scale.

### Comparison with Other Systems

| System          | Per-Key Consistency                                  | Multi-Key Consistency            |
|-----------------|------------------------------------------------------|----------------------------------|
| **Anna**        | Eventual, Causal, Item Cut, Monotonic R/W, PRAM, ... | Read Committed, Read Uncommitted |
| **Redis**       | Linearizable                                         | Serializable                     |
| **MongoDB**     | Linearizable                                         | Linearizable                     |
| **etcd**        | Linearizable (Raft consensus)                        | Serializable                     |
| **DynamoDB**    | Linearizable, Eventual                               | None                             |
| **Cassandra**   | Linearizable, Eventual                               | None                             |
| **CockroachDB** | Serializable                                         | Serializable                     |

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
