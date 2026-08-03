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
| GET {key}                  | Retrieve any key (auto-detects lattice type)                 | Yes           |
| PUT {key} {value}          | Store a scalar value (LWW, default)                          | Yes           |
| PUT set {key} {vals...}    | Store a set (union merge)                                    | Yes           |
| PUT ordered_set {key} ...  | Store an ordered set                                         | Yes           |
| PUT lww_set {key} {vals...} | Store a set (LWW, replaces entire set on write)             | Yes           |
| PUT lww_ordered_set {key} ... | Store an ordered set (LWW, replaces on write)             | Yes           |
| PUT priority_set {key} {p} ... | Store a set (lowest priority wins)                       | Yes           |
| PUT priority_ordered_set ...  | Store ordered set (lowest priority wins)                  | Yes           |
| PUT causal_set {key} {vals...} | Store a set (single-key causal)                          | Yes           |
| PUT causal_ordered_set ...    | Store ordered set (single-key causal)                    | Yes           |
| PUT multi_causal_set ...      | Store a set (multi-key causal)                           | Yes           |
| PUT multi_causal_ordered_set ... | Store ordered set (multi-key causal)                  | Yes           |
| PUT union {key} {value}    | Append a value (accumulates via set union)                   | Yes           |
| PUT priority {key} {p} {v} | Store with priority (lowest wins)                            | Yes           |
| PUT causal {key} {value}   | Store with multi-key causal consistency                      | Yes           |
| PUT single_causal {key} {value} | Store with single-key causal consistency               | Yes           |
| DELETE {key}               | Remove a key (PUT with empty value and dominating timestamp) | Yes           |
| PUT_TTL {key} {val} {secs} | Store with TTL (auto-expires after N seconds)                | Yes           |
| INCREMENT {key} [amount]   | Increment a PN-Counter (default +1)                          | Yes           |
| DECREMENT {key} [amount]   | Decrement a PN-Counter (default -1)                          | Yes           |
| GET_COUNTER {key}          | Get counter value (sum of increments - decrements)           | Yes           |
| Address cache invalidation | Server signals client to refresh address cache               | Yes           |
| Multi-key GET              | Retrieve multiple keys in one request                        | Yes           |

Legacy commands (`GET_SET`, `PUT_SET`, `GET_CAUSAL`, `PUT_CAUSAL`, etc.) are
still supported as aliases for backward compatibility.

## Lattice Types

| Lattice                | Description                                         | System Tested |
|------------------------|-----------------------------------------------------|---------------|
| LWW (Last-Writer-Wins) | Timestamp-based conflict resolution                 | Yes           |
| SET                    | Unordered set with union merge                      | Yes           |
| ORDERED_SET            | Ordered set with union merge                        | Yes           |
| SINGLE_CAUSAL          | Single-key causal with vector clock                 | Yes           |
| MULTI_CAUSAL           | Multi-key causal with vector clock and dependencies | Yes           |
| PRIORITY               | Priority-value pair, lowest priority wins           | Yes           |
| COUNTER                | PN-Counter CRDT (increment/decrement, per-node max) | Yes           |

## Single-Node Features

| Feature           | Description                                           | System Tested |
|-------------------|-------------------------------------------------------|---------------|
| YAML config file  | All settings in a single YAML file                    | Yes           |
| Thread counts     | `threads.memory`, `threads.disk`, `threads.routing`    | Yes           |
| Standalone mode   | `scaling_alert_ip: "NULL"` for local/standalone deployment | Yes           |
| Cluster topology  | seed_ip, scaling_alert_ip, monitoring/routing IPs     | Yes           |
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
| Node capacities               | `capacities.memory-cap`, `capacities.disk-cap` (tested via `elasticity_storage_policy` with `memory-cap-kb: 1`) | Yes           |
| Cross-tier data movement      | Promote hot data to memory, demote cold to disk       | Yes           |

Note: `SERVER_TYPE` and `disk` config should be renamed to storage-medium-agnostic
terms (e.g., `STORAGE_MEDIUM=ram|file`).

Cross-tier data movement is triggered by `anna-monitor` when `policy.tiering`
is enabled, but the mechanism is a replication factor change — a storage
feature, not an autoscaling decision.

## Multi-Node Features (#352)

### Replication

| Feature                         | Description                                                  | System Tested |
|---------------------------------|--------------------------------------------------------------|---------------|
| Per-key replication factors     | Independent replication per key per tier                      | Yes           |
| Default replication from config | `replication.memory`, `replication.disk`, `replication.local`  | Yes           |
| Replication factor request      | Server fetches unknown factors from metadata                  | Yes           |
| Replication factor change       | Monitor can dynamically adjust replication                    | Yes           |
| Metadata stored as KVS data     | Replication info under `METADATA\|replication\|<key>`         | Yes           |
| Gossip after replication change | Data redistributed to new responsible threads                 | Yes           |

### Gossip / Multicast

| Feature                     | Description                                      | System Tested |
|-----------------------------|--------------------------------------------------|---------------|
| Periodic gossip (10s epoch) | Changesets multicast to all responsible replicas  | Yes           |
| Merge-at-sender             | Batched updates merged before sending             | Yes           |
| Value change subscription   | Changed keys pushed to subscribed clients         | Yes           |
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

- **Server primitives** — the cluster mechanisms that enable scaling (node
  join/depart, replication changes, stats reporting). These are implemented
  in `anna-kvs`, `anna-route`, and `anna-monitor`.
- **Client library helpers** — convenience methods in all four client
  libraries for reading stats, managing replication, and reporting latency.
  These make it easy to build an autoscaler in any language. See
  [client-feature-list.md](client-feature-list.md) for per-client details.
- **Operator responsibility** — the decision logic (when to add/remove
  nodes) and the infrastructure lifecycle (provisioning/deprovisioning
  machines). Not part of the Anna project.

### Server primitives

| Feature                     | Description                                                   | System Tested |
|-----------------------------|---------------------------------------------------------------|---------------|
| Hot-key replication         | Selectively replicate hot keys across more threads/nodes      | Yes           |
| Grace period                | Configurable cooldown preventing rapid scaling oscillation    | Yes           |
| Policy toggles              | `policy.elasticity`, `policy.selective-rep`, `policy.tiering` | Yes           |
| Latency feedback ingestion  | Monitor accepts `UserFeedback` protobuf for SLO decisions    | Yes           |

### Client library helpers

All four client libraries (Rust, C++, Go, Python) implement the following
helpers. See [client-feature-list.md](client-feature-list.md) for the
complete per-client feature matrix.

| Feature                          | Description                                            | System Tested |
|----------------------------------|--------------------------------------------------------|---------------|
| Read storage/occupancy stats     | Helper to GET and decode `ServerThreadStatistics`      | Yes           |
| Read per-key access stats        | Helper to GET and decode `KeyAccessData`               | Yes           |
| Read per-key size stats          | Helper to GET and decode `KeySizeData`                 | Yes           |
| Report latency feedback          | Send `UserFeedback` to monitor via `LatencyReporter`   | Yes           |
| Set per-key replication factor   | Write `ReplicationFactor` protobuf to metadata key     | Yes           |
| Cluster topology discovery       | Read `ClusterTopology` from metadata key               | Yes           |
| Monitoring IP discovery          | Read monitoring IPs from metadata key                  | Yes           |

### Operator responsibility (not project features)

The following are **not features of Anna** — they are the operator's domain,
documented with examples and tested via system tests that simulate an
external autoscaler:

- **Elasticity decisions** — when to add/remove nodes based on stats
- **Infrastructure provisioning** — starting/stopping server processes or VMs
- **SLO policy logic** — latency thresholds and scaling formulas

`anna-monitor` contains a built-in policy engine (`storage_policy.cpp`,
`slo_policy.cpp`, `movement_policy.cpp`) that implements reference decision
logic, but it depends on an external system
(`tcp://<scaling_alert_ip>:<ports.scaling_alert>`, default port `6955`,
subject to `ports.base_offset`) that is not part of the project. Operators
can use the client library helpers to implement their own scaling logic.

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
| Client library helpers                 | all clients    | 7     | 7      | 100%     | —      |
| **Total**                              |                | **88**| **88** | **100%** |        |
