# Anna Architecture

Anna is a partitioned, multi-mastered key-value store that achieves high performance
and elasticity via wait-free execution and coordination-free consistency. It was
developed at UC Berkeley's RISE Lab.

## Design Principles

Anna's design rests on four requirements:

1. **Partitioning** — The key space is sharded not only across nodes but also across
   cores for high performance.
2. **Multi-master replication** — Multiple replicas can concurrently serve puts and
   gets against a single key from multiple threads.
3. **Wait-free execution** — Each thread is always doing useful work, never waiting
   for other threads for coordination or consistency.
4. **Coordination-free consistency** — A unified implementation supports a wide range
   of consistency models without coordination protocols.

## System Components

An Anna deployment consists of four types of nodes:

```
┌──────────────────────────────────────────────────────────┐
│                    Routing Tier                          │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                   │
│  │ Routing │  │ Routing │  │ Routing │  ...              │
│  │  Node   │  │  Node   │  │  Node   │                   │
│  └─────────┘  └─────────┘  └─────────┘                   │
├──────────────────────────────────────────────────────────┤
│                   Memory Tier                            │
│  ┌────────────────┐  ┌────────────────┐                  │
│  │ Storage Kernel │  │ Storage Kernel │  ...             │
│  │  (anna-kvs)    │  │  (anna-kvs)    │                  │
│  │ Memory Buffer  │  │ Memory Buffer  │                  │
│  └────────────────┘  └────────────────┘                  │
├──────────────────────────────────────────────────────────┤
│                    Disk Tier                             │
│  ┌────────────────┐  ┌────────────────┐                  │
│  │ Storage Kernel │  │ Storage Kernel │  ...             │
│  │  (anna-kvs)    │  │  (anna-kvs)    │                  │
│  │  EBS Volume    │  │  EBS Volume    │                  │
│  └────────────────┘  └────────────────┘                  │
├──────────────────────────────────────────────────────────┤
│  ┌────────────┐  ┌───────────────┐                       │
│  │ Monitoring │  │   Management  │                       │
│  │   Node     │  │    System     │                       │
│  └────────────┘  └───────────────┘                       │
└──────────────────────────────────────────────────────────┘
```

### Storage Nodes (anna-kvs)

Storage nodes run the Anna storage kernel. Each node contains multiple worker
threads, each pinned to a CPU core. Each thread:

- Maintains a **private hash table** (no shared memory between threads)
- Runs a tight event loop processing client requests via ZeroMQ
- Periodically **multicasts** updates to replicas via a "gossip" protocol
- Uses **consistent hashing** with virtual nodes for key partitioning

The storage kernel is identical across memory and disk tiers — the only
difference is the serialization layer (memory buffer vs. EBS volume).

### Routing Nodes (anna-route)

The routing service isolates clients from the storage layer. A client asks
the routing tier where to find a key and receives valid server addresses.
Routing nodes:

- Cache the storage tiers' hash rings and replication vectors
- Return memory-tier addresses when available (for performance)
- Are stateless — only maintain soft state (cached metadata)
- Handle cluster configuration changes transparently to clients

### Monitoring Node (anna-monitor)

The monitoring system collects statistics and drives the policy engine:

- Tracks per-key access frequency and per-node storage consumption
- Triggers **elasticity** actions (add/remove nodes)
- Triggers **hot-key replication** (replicate popular keys across more cores/nodes)
- Triggers **cross-tier data movement** (promote hot data to memory, demote cold to disk)

### Cluster Management

In a cloud deployment, Anna uses Kubernetes for allocating and deallocating nodes.
The cluster management pod receives REST requests from the policy engine and
executes node additions/removals.

## Actor Model and Event Loop

Each Anna worker thread implements a **coordination-free actor**:

1. The thread checks for incoming requests (PUT, GET) via ZeroMQ poll
2. It serves the request against its private hash table
3. It appends updated keys to a local **changeset**
4. At the end of each **multicast epoch**, the thread:
   - Multicasts its changeset to other masters responsible for those keys
   - Checks for incoming multicasts from other actors
   - Merges received updates into its local state using lattice merge

This design ensures:
- No thread synchronization (wait-free)
- No shared memory contention
- Updates propagate asynchronously via gossip
- Lattice merge guarantees eventual consistency despite message reordering

## Communication

Anna uses ZeroMQ for all communication:

- **Intra-node** (between threads on same machine): ZeroMQ `inproc` transport
  with shared memory buffers for zero-copy messaging
- **Inter-node** (between machines): ZeroMQ `tcp` transport with Protocol
  Buffer serialization
- **Client-to-routing**: PUSH/PULL sockets on port 6450
- **Client-to-KVS**: PUSH/PULL sockets on configurable ports (6800/6850)

## Consistent Hashing

Anna partitions keys across actors using consistent hashing with virtual nodes:

- Each physical node/thread maps to multiple virtual nodes on the hash ring
- CRC32 hash of the key determines which actors store it
- The **replication factor** determines how many clockwise successors store copies
- Virtual nodes enable even distribution and support for heterogeneous hardware

## Fault Tolerance

Anna guarantees k-fault tolerance by ensuring k+1 replicas are live:

- When a storage node fails, other nodes detect via timeout and remove it
  from the hash ring
- Data is automatically repartitioned using the updated hash ring
- The management system spawns a replacement node
- Key migration is interleaved with request handling (no downtime)
- Routing and monitoring nodes are stateless and recover by querying peers
