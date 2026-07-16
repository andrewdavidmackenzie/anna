# Key Concepts

This page defines the core concepts used throughout Anna's design and
documentation.

## Coordination-Free Actors

Anna's execution model is based on coordination-free actors:

- Each worker thread is an **actor** with private memory, pinned to a CPU core
- Actors never share memory or use locks/atomic instructions
- Communication is via **asynchronous message passing** (ZeroMQ)
- Each actor runs a continuous event loop, always doing useful work

This model avoids the scalability bottleneck of shared-memory architectures,
where synchronization (locks, atomics, cache coherence) becomes the dominant
cost as core count increases.

## Consistent Hashing

Anna uses consistent hashing with virtual nodes to partition keys:

- Each actor maps to multiple **virtual nodes** on a hash ring
- A key's hash determines which actors store it
- The **replication factor** controls how many clockwise successors hold copies
- Virtual nodes ensure even distribution and support heterogeneous hardware
- When nodes join/leave, only keys in the affected ring segments need to move

## Replication Vector

Each key has a **replication vector** describing how many replicas exist in each
tier and how they are distributed:

```
[< R_M, R_E >, < T_M, T_E >]
```

- R_M: number of memory-tier nodes storing this key
- R_E: number of EBS-tier nodes storing this key
- T_M: number of threads per memory node storing this key
- T_E: number of threads per EBS node storing this key

The policy engine adjusts replication vectors dynamically based on workload.

## Hash Rings

Anna maintains two types of hash rings per tier:

- **Global hash ring (G)**: Determines which nodes in a tier store a key
- **Local hash ring (L)**: Determines which threads within a node store a key

During request handling, both rings are consulted to find the responsible threads.

## Gossip / Multicast

Anna actors periodically exchange updates via a gossip protocol:

- Each actor maintains a **changeset** of keys updated since the last epoch
- At epoch end, the actor multicasts merged updates to relevant replicas
- Updates are merged using lattice operations, ensuring convergence
- The **multicast epoch** is a tunable parameter (default: 10 seconds)
- Merge-at-sender optimization reduces network overhead for hot keys

## Storage Tiers

Anna supports multiple storage tiers:

| Tier       | Medium    | Performance                  | Cost  |
|------------|-----------|------------------------------|-------|
| Memory     | RAM       | Low latency, high throughput | High  |
| Disk (EBS) | Flash/SSD | Higher latency               | Lower |

The storage kernel is identical across tiers — only the serialization
("serde") layer differs. Memory-tier threads read/write memory buffers;
disk-tier threads read/write files on mounted volumes.

## Client Proxy

Client proxies serve user requests:

- Support GET, PUT, and DELETE operations
- Query the routing tier to find key locations
- Cache key-address mappings locally
- Handle cache invalidation when the cluster reconfigures
- For advanced consistency levels, maintain a per-transaction state 
  (message buffers, read caches)

## Operations

Anna supports these operations:

| Operation      | Description                                                |
|----------------|------------------------------------------------------------|
| **GET**        | Retrieve a value from a single replica                     |
| **PUT**        | Merge a new value using the lattice merge function         |
| **DELETE**     | Special PUT with empty value and dominating timestamp      |
| **GET_SET**    | Retrieve a set-valued key                                  |
| **PUT_SET**    | Add values to a set (union semantics)                      |
| **GET_CAUSAL** | Retrieve with causal metadata (vector clock, dependencies) |
| **PUT_CAUSAL** | Store with causal metadata                                 |

## Metadata

Anna manages three types of metadata:

1. **Hash rings** — Which nodes/threads are responsible for which keys
2. **Replication vectors** — Per-key replication configuration
3. **Monitoring statistics** — Access frequency, storage consumption per node

All metadata is stored in the key-value store itself, using the same
lattice-based merge for consistency. Metadata changes propagate via the
same gossip mechanism as regular data.
