# Anna Feature List

A comprehensive list of features implemented in the Anna KVS, derived from
the research papers, documentation, and server source code. This list serves
as the basis for black-box system testing.

The "System Tested" column indicates whether the feature is tested against a
**live server** (system tests or CLI smoke tests), not just unit tested with
mocks.

## Client Operations

| Feature                    | Description                                                  | System Tested |
|----------------------------|--------------------------------------------------------------|---------------|
| GET                        | Retrieve a value by key                                      | Yes           |
| PUT                        | Store a value by key (lattice merge)                         | Yes           |
| DELETE                     | Remove a key (PUT with empty value and dominating timestamp) | Yes           |
| GET_SET                    | Retrieve a set-valued key                                    | Yes           |
| PUT_SET                    | Add values to a set (union semantics)                        | Yes           |
| GET_CAUSAL                 | Retrieve with causal metadata (vector clock, dependencies)   | Yes           |
| PUT_CAUSAL                 | Store with causal metadata                                   | Yes           |
| GET_ORDERED_SET            | Retrieve an ordered set-valued key                           | Yes           |
| PUT_ORDERED_SET            | Add values to an ordered set                                 | Yes           |
| GET_SINGLE_CAUSAL          | Retrieve with single-key causal metadata                     | Yes           |
| PUT_SINGLE_CAUSAL          | Store with single-key causal metadata                        | Yes           |
| GET_PRIORITY               | Retrieve a priority-valued key                               | Yes           |
| PUT_PRIORITY               | Store a priority-value pair (lowest priority wins)           | Yes           |
| Address cache invalidation | Server signals client to refresh address cache               | Yes           |
| Multi-key GET              | Retrieve multiple keys in one request                        | Yes           |

## Lattice Types

| Lattice                | Description                                         | System Tested |
|------------------------|-----------------------------------------------------|---------------|
| LWW (Last-Writer-Wins) | Timestamp-based conflict resolution                 | Yes           |
| SET                    | Unordered set with union merge                      | Yes           |
| ORDERED_SET            | Ordered set with union merge                        | Yes           |
| SINGLE_CAUSAL          | Single-key causal with vector clock                 | Yes           |
| MULTI_CAUSAL           | Multi-key causal with vector clock and dependencies | Yes           |
| PRIORITY               | Priority-value pair, lowest priority wins           | Yes           |

## Single-Node Features

| Feature           | Description                                           | System Tested |
|-------------------|-------------------------------------------------------|---------------|
| YAML config file  | All settings in a single YAML file                    | Yes           |
| Thread counts     | `threads.memory`, `threads.ebs`, `threads.routing`    | Yes           |
| Standalone mode   | `mgmt_ip: "NULL"` for local/non-k8s deployment       | Yes           |
| Cluster topology  | seed_ip, mgmt_ip, monitoring/routing IPs              | Yes           |
| ZeroMQ PUSH/PULL  | Async messaging between all components                | Yes           |
| Protocol Buffers  | Structured message serialization                      | Yes           |
| Socket cache      | Lazy-created, cached ZMQ push sockets                 | Yes           |
| Graceful shutdown | SIGTERM handler for clean exit                         | Yes           |
| Memory tier       | In-memory storage using hash tables                   | Yes           |

## Error Handling (#355)

| Error Code   | Description                                      | System Tested |
|--------------|--------------------------------------------------|---------------|
| NO_ERROR     | Operation succeeded                              | Yes           |
| KEY_DNE      | Key does not exist                                | Yes           |
| WRONG_THREAD | This thread is not responsible for the key        | Yes           |
| LATTICE      | Lattice type mismatch                             | Yes           |
| NO_SERVERS   | No servers available (routing tier)               | Yes           |

Note: The protobuf defines a `TIMEOUT` error code but the server never sets it.
It is a client-side construct — see `docs/client-feature-list.md`.

## Multi-Tiered Storage (#356)

| Feature                       | Description                                           | System Tested |
|-------------------------------|-------------------------------------------------------|---------------|
| Disk tier                     | File-based storage on configurable path               | Yes           |
| Tier selection                | `SERVER_TYPE` env var selects storage medium           | Yes           |
| Identical kernel across tiers | Same storage kernel, different serialization layer     | Yes           |
| Node capacities               | `capacities.memory-cap`, `capacities.ebs-cap`         | Yes           |
| Cross-tier data movement      | Promote hot data to memory, demote cold to disk       | Yes           |

Note: `SERVER_TYPE` and `ebs` config should be renamed to storage-medium-agnostic
terms (e.g., `STORAGE_MEDIUM=ram|file`).

Cross-tier data movement is triggered by `anna-monitor` when `policy.tiering`
is enabled, but the mechanism is a replication factor change — a storage
feature, not an autoscaling decision.

## Multi-Node Features (#352)

### Replication

| Feature                         | Description                                                  | System Tested |
|---------------------------------|--------------------------------------------------------------|---------------|
| Per-key replication factors     | Independent replication per key per tier                      | Yes           |
| Default replication from config | `replication.memory`, `replication.ebs`, `replication.local`  | Yes           |
| Replication factor request      | Server fetches unknown factors from metadata                  | Yes           |
| Replication factor change       | Monitor can dynamically adjust replication                    | Yes           |
| Metadata stored as KVS data     | Replication info under `METADATA\|replication\|<key>`         | Yes           |
| Gossip after replication change | Data redistributed to new responsible threads                 | Yes           |

### Gossip / Multicast

| Feature                     | Description                                      | System Tested |
|-----------------------------|--------------------------------------------------|---------------|
| Periodic gossip (10s epoch) | Changesets multicast to all responsible replicas  | Yes           |
| Merge-at-sender             | Batched updates merged before sending             | Yes           |
| Gossip to caches            | Changed keys pushed to registered cache clients   | Yes           |
| Join gossip                 | Redistribute data to newly joined nodes           | Yes           |
| Cross-tier gossip           | Updates propagated between memory and disk tiers  | Yes           |

### Cluster Management

| Feature          | Description                                          | System Tested |
|------------------|------------------------------------------------------|---------------|
| Node join        | New node joins cluster, receives data via gossip     | Yes           |
| Node depart      | Node leaves, data redistributed                      | Yes           |
| Self-depart      | Node gracefully removes itself, gossips all data out | Yes           |
| Rejoin detection | Join counter distinguishes fresh joins from rejoins  | Yes           |
| Seed node        | First routing node serves cluster membership         | Yes           |

### Fault Tolerance

| Feature                                 | Description                                     | System Tested |
|-----------------------------------------|-------------------------------------------------|---------------|
| k-fault tolerance                       | k+1 replicas ensure k failures tolerable        | Yes           |
| Failure detection via timeout           | Nodes detect peer failures and update hash ring  | Yes           |
| Automatic repartitioning                | Data redistributed after node failure            | Yes           |
| Stateless routing recovery              | Routing rebuilds hash ring from KVS join messages | Yes           |
| Key migration interleaved with requests | No downtime during reconfiguration               | Yes           |

### Consistent Hashing

| Feature                         | Description                                  | System Tested |
|---------------------------------|----------------------------------------------|---------------|
| Two-level hash ring             | Global (nodes) + Local (threads within node) | Yes           |
| Virtual nodes (3000 per thread) | Even distribution across physical threads    | Yes           |
| CRC32 hashing                   | Hash function for key-to-ring mapping        | Yes           |
| Thread responsibility lookup    | Determines which threads handle a key        | Yes           |

### Routing Tier

| Feature                   | Description                                              | System Tested |
|---------------------------|----------------------------------------------------------|---------------|
| Key address lookup        | Client queries routing for server addresses              | Yes           |
| Hash ring caching         | Routing caches storage tier hash rings                   | Yes           |
| Memory-tier preference    | Returns memory addresses when available                  | Yes           |
| Replication-aware routing | Uses replication vectors to find all responsible threads | Yes           |
| Pending request queue     | Queues requests while replication factor is unknown      | Yes           |
| Multi-threaded routing    | Configurable number of routing threads                   | Yes           |

## Monitoring — `anna-monitor` (#357)

The monitoring system is a separate server process (`anna-monitor`) that
passively collects statistics from KVS nodes and detects membership changes.

Statistics are reported by `anna-kvs` via `ServerThreadStatistics` protobuf
messages, stored as internal metadata keys
(`ANNA_METADATA|<type>|<public_ip>|<private_ip>|<tid>|<tier>`), and read
by `anna-monitor` each monitoring cycle.

| Feature                              | Description                                  | System Tested |
|--------------------------------------|----------------------------------------------|---------------|
| Storage consumption reporting        | Per-thread storage size in KB                | Yes           |
| CPU occupancy reporting              | Ratio of working time to wall-clock time     | Yes           |
| Access count reporting               | Total accesses per epoch                     | Yes           |
| Per-key access frequency             | Tracked over 60-second window                | Yes           |
| Per-key size for primary replicas    | Size of data for keys this thread owns       | Yes           |
| Per-event-type occupancy logging     | Performance profiling of event handlers      | Yes           |

## Autoscaling Support

Anna provides the **primitives** for autoscaling but delegates the scaling
**decisions** and **infrastructure lifecycle** to the operator. This is a
deliberate split:

- **Server features** — the cluster primitives that enable scaling (node
  join/depart, replication changes, stats reporting). These are implemented
  in `anna-kvs`, `anna-route`, and `anna-monitor`.
- **Client library helpers** — convenience methods for reading stats and
  triggering scaling actions. These make it easy to build an autoscaler
  in any language.
- **Operator responsibility** — the decision logic (when to add/remove
  nodes) and the infrastructure lifecycle (provisioning/deprovisioning
  machines). Not part of the Anna project.

### Server features (autoscaling primitives)

| Feature                     | Description                                                   | System Tested |
|-----------------------------|---------------------------------------------------------------|---------------|
| Hot-key replication         | Selectively replicate hot keys across more threads/nodes      | Yes           |
| Grace period                | Configurable cooldown preventing rapid scaling oscillation    | Yes           |
| Policy toggles              | `policy.elasticity`, `policy.selective-rep`, `policy.tiering` | Yes           |
| Latency feedback ingestion  | Monitor accepts `UserFeedback` protobuf for SLO decisions    | Yes           |

### Client library helpers (#410)

| Feature                          | Description                                            | Implemented |
|----------------------------------|--------------------------------------------------------|-------------|
| Read storage/occupancy stats     | Helper to GET and decode `ServerThreadStatistics`      | No          |
| Read per-key access stats        | Helper to GET and decode `KeyAccessData`               | No          |
| Read per-key size stats          | Helper to GET and decode `KeySizeData`                 | No          |
| Report latency feedback          | Send `UserFeedback` to monitor for SLO enforcement    | No          |

### Operator responsibility (not project features)

The following are **not features of Anna** — they are the operator's domain,
documented with examples and tested via system tests that simulate an
external autoscaler:

- **Elasticity decisions** — when to add/remove nodes based on stats
- **Infrastructure provisioning** — starting/stopping server processes or VMs
- **SLO policy logic** — latency thresholds and scaling formulas

`anna-monitor` contains a built-in policy engine (`storage_policy.cpp`,
`slo_policy.cpp`, `movement_policy.cpp`) that implements reference decision
logic, but it depends on a management node (`tcp://<mgmt_ip>:7001`) that
is not part of the project. Operators can use the client library helpers
to implement their own scaling logic.

## Summary

| Category                               | Process        | Total | Tested | Coverage | Issue  |
|----------------------------------------|----------------|-------|--------|----------|--------|
| Client Operations                      | all clients    | 15    | 15     | 100%     | —      |
| Lattice Types                          | `anna-kvs`     | 6     | 6      | 100%     | —      |
| Single-Node Features                   | `anna-kvs`     | 9     | 9      | 100%     | —      |
| Error Handling                         | `anna-kvs`     | 5     | 5      | 100%     | —      |
| Multi-Tiered Storage                   | `anna-kvs`     | 5     | 5      | 100%     | —      |
| Multi-Node Features                    | `anna-kvs`     | 25    | 25     | 100%     | —      |
| Routing Tier                           | `anna-route`   | 6     | 6      | 100%     | —      |
| Monitoring                             | `anna-monitor` | 6     | 6      | 100%     | —      |
| Autoscaling (server primitives)        | `anna-monitor` | 4     | 4      | 100%     | —      |
| **Total**                              |                | **81**| **81** | **100%** |        |
