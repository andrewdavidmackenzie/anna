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
| Address cache invalidation | Server signals client to refresh address cache               | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Multi-key GET              | Retrieve multiple keys in one request                        | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |

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
| Cluster topology  | seed_ip, mgmt_ip, monitoring/routing IPs              | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| ZeroMQ PUSH/PULL  | Async messaging between all components                | Yes           |
| Protocol Buffers  | Structured message serialization                      | Yes           |
| Socket cache      | Lazy-created, cached ZMQ push sockets                 | Yes           |
| Graceful shutdown | SIGTERM handler for clean exit                         | Yes           |
| Memory tier       | In-memory storage using hash tables                   | Yes           |

## Error Handling (#355)

| Error Code   | Description                                      | System Tested |
|--------------|--------------------------------------------------|---------------|
| NO_ERROR     | Operation succeeded                              | Yes           |
| KEY_DNE      | Key does not exist                                | [#355](https://github.com/andrewdavidmackenzie/anna/issues/355) |
| WRONG_THREAD | This thread is not responsible for the key        | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| LATTICE      | Lattice type mismatch                             | [#355](https://github.com/andrewdavidmackenzie/anna/issues/355) |
| NO_SERVERS   | No servers available (routing tier)               | [#355](https://github.com/andrewdavidmackenzie/anna/issues/355) |

Note: The protobuf defines a `TIMEOUT` error code but the server never sets it.
It is a client-side construct — see `docs/client-feature-list.md`.

## Multi-Tiered Storage (#356)

| Feature                       | Description                                           | System Tested |
|-------------------------------|-------------------------------------------------------|---------------|
| Disk tier                     | File-based storage on configurable path               | No            |
| Tier selection                | `SERVER_TYPE` env var selects storage medium           | No            |
| Identical kernel across tiers | Same storage kernel, different serialization layer     | No            |
| Node capacities               | `capacities.memory-cap`, `capacities.ebs-cap`         | No            |

Note: `SERVER_TYPE` and `ebs` config should be renamed to storage-medium-agnostic
terms (e.g., `STORAGE_MEDIUM=ram|file`).

## Multi-Node Features (#352)

### Replication

| Feature                         | Description                                                  | System Tested |
|---------------------------------|--------------------------------------------------------------|---------------|
| Per-key replication factors     | Independent replication per key per tier                      | No            |
| Default replication from config | `replication.memory`, `replication.ebs`, `replication.local`  | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Replication factor request      | Server fetches unknown factors from metadata                  | No            |
| Replication factor change       | Monitor can dynamically adjust replication                    | No            |
| Metadata stored as KVS data     | Replication info under `METADATA\|replication\|<key>`         | No            |
| Gossip after replication change | Data redistributed to new responsible threads                 | No            |

### Gossip / Multicast

| Feature                     | Description                                      | System Tested |
|-----------------------------|--------------------------------------------------|---------------|
| Periodic gossip (10s epoch) | Changesets multicast to all responsible replicas  | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Merge-at-sender             | Batched updates merged before sending             | No            |
| Gossip to caches            | Changed keys also sent to function executor nodes | No            |
| Join gossip                 | Redistribute data to newly joined nodes           | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Cross-tier gossip           | Updates propagated between memory and disk tiers  | No            |

### Cluster Management

| Feature          | Description                                          | System Tested |
|------------------|------------------------------------------------------|---------------|
| Node join        | New node joins cluster, receives data via gossip     | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Node depart      | Node leaves, data redistributed                      | [#365](https://github.com/andrewdavidmackenzie/anna/issues/365) |
| Self-depart      | Node gracefully removes itself, gossips all data out | No            |
| Rejoin detection | Join counter distinguishes fresh joins from rejoins  | [#372](https://github.com/andrewdavidmackenzie/anna/issues/372) |
| Seed node        | First routing node serves cluster membership         | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |

### Fault Tolerance

| Feature                                 | Description                                     | System Tested |
|-----------------------------------------|-------------------------------------------------|---------------|
| k-fault tolerance                       | k+1 replicas ensure k failures tolerable        | [#364](https://github.com/andrewdavidmackenzie/anna/issues/364) |
| Failure detection via timeout           | Nodes detect peer failures and update hash ring  | No            |
| Automatic repartitioning                | Data redistributed after node failure            | No            |
| Stateless routing/monitoring            | Recovers by querying peers/storage               | [#372](https://github.com/andrewdavidmackenzie/anna/issues/372) |
| Key migration interleaved with requests | No downtime during reconfiguration               | No            |

### Consistent Hashing

| Feature                         | Description                                  | System Tested |
|---------------------------------|----------------------------------------------|---------------|
| Two-level hash ring             | Global (nodes) + Local (threads within node) | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Virtual nodes (3000 per thread) | Even distribution across physical threads    | [#373](https://github.com/andrewdavidmackenzie/anna/issues/373) |
| CRC32 hashing                   | Hash function for key-to-ring mapping        | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |
| Thread responsibility lookup    | Determines which threads handle a key        | [#352](https://github.com/andrewdavidmackenzie/anna/issues/352) |

### Routing Tier

| Feature                   | Description                                              | System Tested |
|---------------------------|----------------------------------------------------------|---------------|
| Key address lookup        | Client queries routing for server addresses              | Yes           |
| Hash ring caching         | Routing caches storage tier hash rings                   | Yes           |
| Memory-tier preference    | Returns memory addresses when available                  | No            |
| Replication-aware routing | Uses replication vectors to find all responsible threads | No            |
| Pending request queue     | Queues requests while replication factor is unknown      | No            |
| Multi-threaded routing    | Configurable number of routing threads                   | No            |

## Monitoring & Policy Engine (#357)

| Feature                     | Description                                                   | System Tested |
|-----------------------------|---------------------------------------------------------------|---------------|
| Statistics collection       | Per-key access frequency, per-node storage, CPU occupancy     | No            |
| Elasticity policy           | Add/remove nodes based on storage/compute capacity            | No            |
| Hot-key replication         | Selectively replicate hot keys across more threads/nodes      | No            |
| Cross-tier data movement    | Promote hot data to memory, demote cold to disk               | No            |
| SLO enforcement             | Latency-based scaling (target: 3ms)                           | No            |
| Underutilization scale-down | Remove nodes when occupancy is low                            | No            |
| Grace period                | Prevent over-correction during data redistribution            | No            |
| Policy toggles              | `policy.elasticity`, `policy.selective-rep`, `policy.tiering` | No            |

## Server Internals (#358)

| Feature                              | Description                                  | System Tested |
|--------------------------------------|----------------------------------------------|---------------|
| Storage consumption reporting        | Per-thread storage size in KB                | No            |
| CPU occupancy reporting              | Ratio of working time to wall-clock time     | No            |
| Access count reporting               | Total accesses per epoch                     | No            |
| Per-key access frequency             | Tracked over 60-second window                | No            |
| Per-key size for primary replicas    | Size of data for keys this thread owns       | No            |
| Per-event-type occupancy logging     | Performance profiling of event handlers      | No            |

## Summary

| Category                     | Total | System Tested | Coverage | Issue  |
|------------------------------|-------|---------------|----------|--------|
| Client Operations            | 15    | 15            | 100%     | —      |
| Lattice Types                | 6     | 6             | 100%     | —      |
| Single-Node Features         | 9     | 9             | 100%     | —      |
| Error Handling               | 5     | 5             | 100%     | #355   |
| Multi-Tiered Storage         | 4     | 0             | 0%       | #356   |
| Multi-Node Features          | 31    | 15            | 48%      | #352   |
| Monitoring & Policy Engine   | 8     | 0             | 0%       | #357   |
| Server Internals             | 5     | 0             | 0%       | #358   |
| **Total**                    | **83**| **49**        | **59%**  |        |
