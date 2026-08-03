# Client Feature List

Features implemented per client. Each client wraps the Anna KVS protocol
(protobuf over ZeroMQ) with a language-native API.

## Value Change Subscription

All four clients provide a value change subscription API — a pub-sub
mechanism for receiving notifications when specific keys are updated
(including deletes). The subscriber registers interest in keys with the
KVS server threads via a dedicated registration port (6900). During each
gossip epoch, when a watched key changes, the KVS pushes the new value
to the subscriber on port 6850.

Applications can use this for caching, event-driven updates, replication
to external systems, or any pattern that needs to react to key changes.

### API (per language)

| Operation                  | Description                                     |
|----------------------------|-------------------------------------------------|
| Create subscriber          | Connect to the KVS and bind the update listener |
| `watch(keys)`              | Register interest in one or more keys           |
| `recv_update(timeout)`     | Block for next pushed update from gossip        |
| `get_cached(key)`          | Read the latest received value locally          |

This feature replaces the original Cloudburst management-node-based cache
registration with direct client-to-server registration, enabling
subscribers in standalone mode (`scaling_alert_ip: "NULL"`).

## Latency Feedback (LatencyReporter)

All four clients provide a `LatencyReporter` API for sending latency
feedback to the monitoring system, enabling SLO enforcement (selective
replication of hot keys when latency exceeds 3ms).

| Operation                  | Description                                     |
|----------------------------|-------------------------------------------------|
| Create reporter            | Connect to monitoring threads (explicit IPs or metadata discovery) |
| `report(latency, throughput, key_latencies)` | Send `UserFeedback` protobuf to all monitors |
| `set_warmup(bool)`         | Toggle warmup flag (monitor ignores policy during warmup) |
| `finish()`                 | Signal that this client is done reporting        |

## Monitoring and Autoscaling Helpers

All four clients provide helper methods for reading cluster metrics and
managing replication factors, enabling operator-driven autoscaling.

| Operation                  | Description                                     |
|----------------------------|-------------------------------------------------|
| `get_storage_stats(ip, ip, tid, tier)` | Read `ServerThreadStatistics` (consumption, occupancy, epoch) |
| `get_key_access_stats(ip, ip, tid, tier)` | Read `KeyAccessData` (per-key access counts) |
| `get_key_size_stats(ip, ip, tid, tier)` | Read `KeySizeData` (per-key sizes) |
| `put_replication_factor(key, mem_rep, local_rep)` | Set per-key replication factor |
| `get_cluster_topology()`   | Read `ClusterTopology` (thread counts) |
| `get_monitoring_ips()`     | Read monitoring node IP addresses |

See [autoscaling.md](autoscaling.md) for the full operator's guide.

## Rust Client (`clients/rust`)

| Feature                    | Tested |
|----------------------------|--------|
| Unified GET (auto-detect type) | Yes |
| Unified PUT (type prefix)  | Yes    |
| DELETE                     | Yes    |
| Value enum (get_value/put_value) | Yes |
| GET_BYTES (raw LWW value)  | Yes    |
| Multi-key GET (get_multi)  | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD retry         | Yes    |
| Timeout retry              | Yes    |
| Dead-address eviction      | Yes    |
| Configurable timeout       | Yes    |
| Port base_offset support   | Yes    |
| Process management (start/stop/status) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |
| Latency feedback (LatencyReporter) | Yes |
| Monitoring stats helpers (get_storage_stats etc.) | Yes |
| Replication factor management (put_replication_factor) | Yes |
| Cluster topology discovery (get_cluster_topology) | Yes |
| Monitoring IP discovery (get_monitoring_ips) | Yes |
| Per-key TTL (put_with_ttl) | Yes    |
| PN-Counter (increment/decrement/get_counter) | Yes |

## C++ Client (`clients/cpp`)

| Feature                    | Tested |
|----------------------------|--------|
| Unified GET (get_any, auto-detect type) | Yes |
| Unified PUT (type prefix)  | Yes    |
| DELETE                     | Yes    |
| GET_BYTES (raw LWW value)  | Yes    |
| Multi-key GET (get_multi)  | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD auto-retry    | Yes    |
| Configurable timeout       | Yes    |
| Process management (start/stop/status) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |
| Latency feedback (LatencyReporter) | Yes |
| Monitoring stats helpers (get_storage_stats etc.) | Yes |
| Replication factor management (put_replication_factor) | Yes |
| Cluster topology discovery (get_cluster_topology) | Yes |
| Monitoring IP discovery (get_monitoring_ips) | Yes |

## Go Client (`clients/go`)

| Feature                    | Tested |
|----------------------------|--------|
| Unified GET (legacy dispatch by type) | Yes |
| Unified PUT (type prefix)  | Yes    |
| DELETE                     | Yes    |
| GET_BYTES (raw LWW value)  | Yes    |
| Multi-key GET (GetMulti)   | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD auto-retry    | Yes    |
| Dead-address eviction      | Yes    |
| Configurable timeout       | Yes    |
| Timeout with retry         | Yes    |
| Process management (start/stop/status) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |
| Latency feedback (LatencyReporter) | Yes |
| Monitoring stats helpers (get_storage_stats etc.) | Yes |
| Replication factor management (put_replication_factor) | Yes |
| Cluster topology discovery (GetClusterTopology) | Yes |
| Monitoring IP discovery (GetMonitoringIPs) | Yes |

## Python Client (`clients/python`)

| Feature                    | Tested |
|----------------------------|--------|
| Unified GET (LWW/Set auto-detect, legacy dispatch for others) | Yes |
| Unified PUT (type prefix)  | Yes    |
| DELETE                     | Yes    |
| GET_BYTES (raw LWW value)  | Yes    |
| Multi-key GET (get_multi)  | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD auto-retry    | Yes    |
| Dead-address eviction      | Yes    |
| Configurable timeout       | Yes    |
| Port base_offset support   | Yes    |
| Process management (start/stop) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |
| Latency feedback (LatencyReporter) | Yes |
| Monitoring stats helpers (get_storage_stats etc.) | Yes |
| Replication factor management (put_replication_factor) | Yes |
| Cluster topology discovery (get_cluster_topology) | Yes |
| Monitoring IP discovery (get_monitoring_ips) | Yes |
