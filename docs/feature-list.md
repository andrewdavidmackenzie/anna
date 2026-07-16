# Anna Feature List

A comprehensive list of features implemented in the Anna KVS, derived from
the research papers, documentation, and server source code. This list serves
as the basis for black-box system testing.

## Client Operations

| Feature                    | Description                                                  | Tested |
|----------------------------|--------------------------------------------------------------|--------|
| GET                        | Retrieve a value by key                                      | Yes    |
| PUT                        | Store a value by key (lattice merge)                         | Yes    |
| DELETE                     | Remove a key (PUT with empty value and dominating timestamp) | Yes    |
| GET_SET                    | Retrieve a set-valued key                                    | Yes    |
| PUT_SET                    | Add values to a set (union semantics)                        | Yes    |
| GET_CAUSAL                 | Retrieve with causal metadata (vector clock, dependencies)   | Yes    |
| PUT_CAUSAL                 | Store with causal metadata                                   | Yes    |
| GET_ORDERED_SET            | Retrieve an ordered set-valued key                           | Yes    |
| PUT_ORDERED_SET            | Add values to an ordered set                                 | Yes    |
| GET_SINGLE_CAUSAL          | Retrieve with single-key causal metadata                     | Yes    |
| PUT_SINGLE_CAUSAL          | Store with single-key causal metadata                        | Yes    |
| GET_PRIORITY               | Retrieve a priority-valued key                               | Yes    |
| PUT_PRIORITY               | Store a priority-value pair (lowest priority wins)           | Yes    |
| Address cache invalidation | Server signals client to refresh address cache               | No     |
| Multi-key GET              | Retrieve multiple keys in one request                        | No     |

## Lattice Types

| Lattice                | Description                                         | Tested |
|------------------------|-----------------------------------------------------|--------|
| LWW (Last-Writer-Wins) | Timestamp-based conflict resolution                 | Yes    |
| SET                    | Unordered set with union merge                      | Yes    |
| ORDERED_SET            | Ordered set with union merge                        | Yes    |
| SINGLE_CAUSAL          | Single-key causal with vector clock                 | Yes    |
| MULTI_CAUSAL           | Multi-key causal with vector clock and dependencies | Yes    |
| PRIORITY               | Priority-value pair, lowest priority wins           | Yes    |

## Storage Tiers

| Feature                       | Description                                | Tested                 |
|-------------------------------|--------------------------------------------|------------------------|
| Memory tier                   | In-memory storage using hash tables        | Yes (via system tests) |
| Disk (EBS) tier               | File-based storage on mounted volumes      | No                     |
| Tier selection via env var    | `SERVER_TYPE=memory` or `SERVER_TYPE=ebs`  | No                     |
| Identical kernel across tiers | Same storage kernel, different serde layer | No                     |

## Replication

| Feature                         | Description                                                  | Tested |
|---------------------------------|--------------------------------------------------------------|--------|
| Per-key replication factors     | Independent replication per key per tier                      | No     |
| Default replication from config | `replication.memory`, `replication.ebs`, `replication.local`  | No     |
| Replication factor request      | Server fetches unknown factors from metadata                  | No     |
| Replication factor change       | Monitor can dynamically adjust replication                    | No     |
| Metadata stored as KVS data     | Replication info under `METADATA\|replication\|<key>`         | No     |
| Gossip after replication change | Data redistributed to new responsible threads                 | No     |

## Gossip / Multicast

| Feature                     | Description                                      | Tested |
|-----------------------------|--------------------------------------------------|--------|
| Periodic gossip (10s epoch) | Changesets multicast to all responsible replicas  | No     |
| Merge-at-sender             | Batched updates merged before sending             | No     |
| Gossip to caches            | Changed keys also sent to function executor nodes | No     |
| Join gossip                 | Redistribute data to newly joined nodes           | No     |
| Cross-tier gossip           | Updates propagated between memory and disk tiers  | No     |

## Cluster Management

| Feature          | Description                                          | Tested |
|------------------|------------------------------------------------------|--------|
| Node join        | New node joins cluster, receives data via gossip     | No     |
| Node depart      | Node leaves, data redistributed                      | No     |
| Self-depart      | Node gracefully removes itself, gossips all data out | No     |
| Rejoin detection | Join counter distinguishes fresh joins from rejoins  | No     |
| Seed node        | First routing node serves cluster membership         | No     |

## Routing Tier

| Feature                   | Description                                              | Tested         |
|---------------------------|----------------------------------------------------------|----------------|
| Key address lookup        | Client queries routing for server addresses              | Yes (implicit) |
| Hash ring caching         | Routing caches storage tier hash rings                   | Yes (implicit) |
| Memory-tier preference    | Returns memory addresses when available                  | No             |
| Replication-aware routing | Uses replication vectors to find all responsible threads | No             |
| Pending request queue     | Queues requests while replication factor is unknown      | No             |
| Multi-threaded routing    | Configurable number of routing threads                   | No             |

## Monitoring and Policy Engine

| Feature                     | Description                                                   | Tested |
|-----------------------------|---------------------------------------------------------------|--------|
| Statistics collection       | Per-key access frequency, per-node storage, CPU occupancy     | No     |
| Elasticity policy           | Add/remove nodes based on storage/compute capacity            | No     |
| Hot-key replication         | Selectively replicate hot keys across more threads/nodes      | No     |
| Cross-tier data movement    | Promote hot data to memory, demote cold to disk               | No     |
| SLO enforcement             | Latency-based scaling (target: 3ms)                           | No     |
| Underutilization scale-down | Remove nodes when occupancy is low                            | No     |
| Grace period                | Prevent over-correction during data redistribution            | No     |
| Policy toggles              | `policy.elasticity`, `policy.selective-rep`, `policy.tiering` | No     |

## Consistent Hashing

| Feature                         | Description                                  | Tested |
|---------------------------------|----------------------------------------------|--------|
| Two-level hash ring             | Global (nodes) + Local (threads within node) | No     |
| Virtual nodes (3000 per thread) | Even distribution across physical threads    | No     |
| CRC32 hashing                   | Hash function for key-to-ring mapping        | No     |
| Thread responsibility lookup    | Determines which threads handle a key        | No     |

## Fault Tolerance

| Feature                                 | Description                                     | Tested |
|-----------------------------------------|-------------------------------------------------|--------|
| k-fault tolerance                       | k+1 replicas ensure k failures tolerable        | No     |
| Failure detection via timeout           | Nodes detect peer failures and update hash ring  | No     |
| Automatic repartitioning                | Data redistributed after node failure            | No     |
| Stateless routing/monitoring            | Recovers by querying peers/storage               | No     |
| Key migration interleaved with requests | No downtime during reconfiguration               | No     |

## Periodic Self-Reporting (every 15s)

| Feature                              | Description                                  | Tested |
|--------------------------------------|----------------------------------------------|--------|
| Storage consumption reporting        | Per-thread storage size in KB                | No     |
| CPU occupancy reporting              | Ratio of working time to wall-clock time     | No     |
| Access count reporting               | Total accesses per epoch                     | No     |
| Per-key access frequency             | Tracked over 60-second window                | No     |
| Per-key size for primary replicas    | Size of data for keys this thread owns       | No     |
| Per-event-type occupancy logging     | Performance profiling of event handlers      | No     |

## Configuration

| Feature                      | Description                                                   | Tested  |
|------------------------------|---------------------------------------------------------------|---------|
| YAML config file             | All settings in a single YAML file                            | Yes     |
| Thread counts                | `threads.memory`, `threads.ebs`, `threads.routing`            | Partial |
| Node capacities              | `capacities.memory-cap`, `capacities.ebs-cap`                 | No      |
| Server identity              | `server.public_ip`, `server.private_ip`                       | No      |
| Cluster topology             | `server.seed_ip`, `server.mgmt_ip`, monitoring/routing IPs    | Partial |
| Management node integration  | Kubernetes support for node provisioning                      | No      |
| Standalone mode              | `mgmt_ip: "NULL"` for local/non-k8s deployment               | Yes     |

## Communication

| Feature          | Description                                  | Tested         |
|------------------|----------------------------------------------|----------------|
| ZeroMQ PUSH/PULL | Async messaging between all components       | Yes            |
| Protocol Buffers | Structured message serialization             | Yes            |
| Socket cache     | Lazy-created, cached ZMQ push sockets        | Yes (implicit) |
| Graceful shutdown | SIGTERM handler for clean exit               | Yes            |

## Error Handling

| Error Code   | Description                                      | Tested |
|--------------|--------------------------------------------------|--------|
| NO_ERROR     | Operation succeeded                              | Yes    |
| KEY_DNE      | Key does not exist                                | No     |
| WRONG_THREAD | This thread is not responsible for the key        | No     |
| TIMEOUT      | Operation timed out                               | No     |
| LATTICE      | Lattice type mismatch                             | No     |
| NO_SERVERS   | No servers available (routing tier)               | No     |

## Summary

| Category                 | Total Features | Tested | Coverage |
|--------------------------|----------------|--------|----------|
| Client Operations        | 15             | 13     | 87%      |
| Lattice Types            | 6              | 6      | 100%     |
| Storage Tiers            | 4              | 1      | 25%      |
| Replication              | 6              | 0      | 0%       |
| Gossip / Multicast       | 5              | 0      | 0%       |
| Cluster Management       | 5              | 0      | 0%       |
| Routing Tier             | 6              | 2      | 33%      |
| Monitoring / Policy      | 8              | 0      | 0%       |
| Consistent Hashing       | 4              | 0      | 0%       |
| Fault Tolerance          | 5              | 0      | 0%       |
| Periodic Self-Reporting  | 6              | 0      | 0%       |
| Configuration            | 7              | 4      | 57%      |
| Communication            | 4              | 4      | 100%     |
| Error Handling           | 6              | 1      | 17%      |
| **Total**                | **87**         | **31** | **36%**  |

The next step (#336) is to prioritize untested features and add system tests
to improve both feature and code coverage of the server.
