# Autoscaling and Policy Engine

Anna includes autoscaling capabilities to adapt to changing workloads,
automatically adjusting its deployment to balance cost, latency, and
fault tolerance.

## Workload Challenges

Real-world workloads vary along three dimensions:

| Dimension          | Challenge                             | Anna's Response       |
|--------------------|---------------------------------------|-----------------------|
| **Volume**         | Overall load increases/decreases      | Horizontal elasticity |
| **Skewness**       | Some keys are much hotter than others | Selective replication |
| **Hotspot shifts** | Hottest keys change over time         | Replication + tiering |

## Three Key Mechanisms

### 1. Horizontal Elasticity

Anna scales each storage tier independently by adding or removing nodes:

- **Scaling out**: When storage or compute capacity is insufficient, the policy
  engine instructs the cluster manager to add nodes. Data is automatically
  repartitioned via consistent hashing.
- **Scaling in**: When resources are underutilized, nodes are removed. The
  policy engine first checks that no key's replication factor would exceed
  the remaining node count, adjusting replication factors if needed.
- **Grace period**: After any resource change, a grace period prevents
  over-correction while data redistributes and access patterns stabilize.

### 2. Multi-Master Selective Replication

When `policy.selective-rep` is enabled, Anna selectively replicates hot
keys rather than replicating all keys uniformly:

- The policy engine classifies keys as "hot" if their access frequency
  exceeds H standard deviations above the mean
- Hot keys are first replicated across more threads within a node
  (intra-node replication), then across nodes (cross-node replication)
- Cold keys maintain the default replication factor
- Cross-node replication provides 4x throughput improvement per replica
  (due to network bandwidth scaling), vs 2x for intra-node

### 3. Vertical Tiering

Anna supports multiple storage tiers with different cost-performance profiles:

- **Memory tier**: Fast RAM-based storage (e.g., cloud VM instances)
- **Disk tier**: Cheaper flash-based storage (e.g., disk volumes)
- Hot data is **promoted** from disk to memory when access frequency
  exceeds a threshold
- Cold data is **demoted** from memory to disk when access frequency drops
- Promotion and demotion are implemented by adjusting per-key replication
  vectors — gossip handles the actual data movement

## Service Level Objectives (SLOs)

Anna supports three types of SLOs:

| SLO                     | Description                            | Default        |
|-------------------------|----------------------------------------|----------------|
| **Latency** (L_obj)     | Average request latency target         | 2.5ms          |
| **Budget** (B)          | Maximum cost per hour                  | User-specified |
| **Fault Tolerance** (k) | Number of replica failures to tolerate | 2              |

The policy engine balances these potentially conflicting objectives. For example,
lower latency requires more memory-tier replicas (higher cost), while lower cost
means fewer replicas (higher latency, lower fault tolerance).

## Policy Engine Algorithm

The monitoring system periodically collects statistics and triggers the policy
engine, which evaluates actions in this order:

1. **Storage capacity check**: If storage consumption exceeds thresholds,
   add or remove nodes
2. **Cross-tier data movement**: Promote hot data to memory, demote cold
   data to disk based on access frequency
3. **Latency check**: If latency exceeds the SLO, add memory nodes or
   replicate hot keys
4. **Cost optimization**: If latency is well below the SLO, check if nodes
   can be removed to save cost

### Policy Knobs

| Parameter        | Meaning                                        | Default                        |
|------------------|------------------------------------------------|--------------------------------|
| T                | Monitoring report period                       | 15 seconds                     |
| H                | Key hotness threshold (std devs above mean)    | 3                              |
| L                | Key coldness threshold (mean access frequency) | Mean                           |
| P                | Key promotion threshold                        | 2 accesses in 60s              |
| S_lower, S_upper | Storage consumption thresholds                 | Memory: 0.3-0.6, Disk: 0.5-0.75 |
| f_lower, f_upper | Latency thresholds (fraction of SLO)           | 0.5, 0.75                      |
| C_lower, C_upper | Compute occupancy thresholds                   | 0.05, 0.20                     |
| c                | Upper bound for latency ratio                  | 1.5                            |

Most policy knobs (storage thresholds, tier promotion/demotion, SLO
targets, and node scaling limits) are configurable via the YAML config
file. See the [Configuration Reference](config.md) for the full list of
config keys, defaults, and valid ranges.

## Port Configuration for Multi-Node Deployments

Anna uses a range of ports (6000–7150) for inter-node communication.
The YAML config supports a `ports.base_offset` setting that shifts all
ports by a fixed amount, enabling multiple independent clusters on the
same machine (e.g., for testing or CI).

```yaml
ports:
  base_offset: 0    # default — ports 6000-7150
  # base_offset: 2000  # shifts to ports 8000-9150
```

For multi-node testing on a single machine, each node uses a different
IP on the loopback range (127.0.0.1, 127.0.0.2, …) while sharing the
same port numbers. ZMQ sockets bind to the node's specific IP rather
than `0.0.0.0`, so multiple nodes can coexist without port conflicts.

On macOS, additional loopback addresses require explicit aliases:
```bash
sudo ifconfig lo0 alias 127.0.0.2
```
Linux supports the full 127.0.0.0/8 range by default.

## Operator's Guide to Autoscaling

Anna's autoscaling is a split-responsibility architecture:

- **Anna provides**: metrics collection, cluster primitives (join, depart,
  replication changes), and a built-in policy engine
- **The operator provides**: infrastructure lifecycle (provisioning and
  deprovisioning nodes) and optionally custom decision logic

### Reading Scaling Metrics via Client Libraries

All client libraries provide helper methods to read the metadata keys
that the KVS server writes every `server_report_period` seconds:

```python
# Python example
stats = client.get_storage_stats("10.0.0.1", "10.0.0.1", 0, "MEMORY")
print(f"Storage: {stats.storage_consumption} KB")
print(f"Occupancy: {stats.occupancy}")
print(f"Access count: {stats.access_count}")

access = client.get_key_access_stats("10.0.0.1", "10.0.0.1", 0, "MEMORY")
for key_count in access.keys:
    print(f"Key {key_count.key}: {key_count.access_count} accesses")

sizes = client.get_key_size_stats("10.0.0.1", "10.0.0.1", 0, "MEMORY")
for key_size in sizes.key_sizes:
    print(f"Key {key_size.key}: {key_size.size} bytes")
```

Available in all four client libraries (Rust, Python, Go, C++):

| Method | Returns | Metadata Key |
|--------|---------|-------------|
| `get_storage_stats(ip, ip, tid, tier)` | `ServerThreadStatistics` | `ANNA_METADATA\|stats\|...\|...\|tid\|tier` |
| `get_key_access_stats(ip, ip, tid, tier)` | `KeyAccessData` | `ANNA_METADATA\|access\|...\|...\|tid\|tier` |
| `get_key_size_stats(ip, ip, tid, tier)` | `KeySizeData` | `ANNA_METADATA\|size\|...\|...\|tid\|tier` |
| `put_replication_factor(key, mem_rep, local_rep)` | — | `ANNA_METADATA\|replication\|key` |
| `get_cluster_topology()` | `ClusterTopology` | `ANNA_METADATA\|cluster_topology` |
| `get_monitoring_ips()` | list of IPs | `ANNA_METADATA\|monitoring_ips` |

### Scaling Actions

| Action | How |
|--------|-----|
| **Scale out** | Start a new `anna-kvs` process with `seed_ip` pointing to an existing node. Data redistributes automatically via consistent hashing. |
| **Scale in** | Send `SIGUSR1` to the node to trigger graceful self-departure. The node transfers its data to remaining nodes before exiting. |
| **Hot-key replication** | Call `put_replication_factor(key, N, 1)` to increase the memory replication factor for a specific key. |

### Example: Simple Autoscaler

```python
import time

CAPACITY_GB = 1  # per-node capacity from config
SCALE_OUT_THRESHOLD = 0.6
SCALE_IN_THRESHOLD = 0.05
CHECK_INTERVAL = 30  # seconds

while True:
    stats = client.get_storage_stats(node_ip, node_ip, 0, "MEMORY")
    consumption_pct = stats.storage_consumption / (CAPACITY_GB * 1_000_000)

    if consumption_pct > SCALE_OUT_THRESHOLD:
        # Start a new anna-kvs pointing at the seed node
        start_new_node(seed_ip)

    if stats.occupancy < SCALE_IN_THRESHOLD and node_count > 1:
        # Signal the least-busy node to depart
        signal_depart(least_busy_node_pid)

    time.sleep(CHECK_INTERVAL)
```

### Scaling Alert Protocol

The built-in policy engine communicates scaling recommendations to an
external system via ZMQ PUSH on the `scaling_alert` port (default 7001).
Messages are serialized `ScalingAlert` protobuf messages (defined in
`server/protobuf/metadata.proto`):

- **Scale up**: `action=ADD`, `tier=MEMORY|DISK`, `count=N`
- **Scale down**: `action=REMOVE`, `tier=MEMORY|DISK`, `count=1`,
  `departed_node_ip=<ip>` (sent after a node has finished departing)

The external system is not part of Anna — it's the operator's responsibility
to implement infrastructure provisioning in response to these alerts.

## Performance Results

From the [VLDB 2019 paper](http://www.vikrams.io/papers/anna-vldb19.pdf) evaluation:

- Anna outperforms ElastiCache and Masstree by up to 10x in
  cost-effectiveness under various contention levels
- Anna outperforms DynamoDB by 36x at low cost and up to 355x at higher costs
- Anna meets latency SLOs 97% of the time during dynamic workload changes
- Node failure recovery maintains 80%+ throughput (vs Redis dropping to 0
  during leader election)
- Hotspot adaptation achieves 99.5% memory-tier hit rate within 25 seconds
