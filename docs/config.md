# Configuration Reference

Anna is configured via YAML files in `server/conf/`. Three files are provided:

| File | Purpose |
|------|---------|
| `anna-config.yml` | Default local development configuration |
| `anna-local.yml` | Identical to `anna-config.yml` (convenience copy) |
| `anna-base.yml` | Production-oriented configuration with higher capacities |

All parameters below are optional unless noted otherwise. When omitted,
the compiled-in default is used, preserving backward-compatible behavior.

## Network Configuration

### Monitoring (`monitoring:`)

| Config Key | Required | Default | Meaning |
|------------|----------|---------|---------|
| `monitoring.ip` | Yes | — | IP address of the monitoring node |
| `monitoring.scaling_alert_ip` | Yes | — | IP address of the scaling alert endpoint |

### Routing (`routing:`) — Optional

> **Note:** The `routing:` section is only required when running the legacy
> `anna-route` server. Clients using client-side routing (the recommended
> approach) do not need a routing server.

| Config Key | Required | Default | Meaning |
|------------|----------|---------|---------|
| `routing.ip` | Yes (if using anna-route) | — | IP address of this routing node |
| `routing.monitoring` | Yes (if using anna-route) | — | List of monitoring node IPs |

### Server (`server:`)

| Config Key | Required | Default | Meaning |
|------------|----------|---------|---------|
| `server.public_ip` | Yes | — | Public IP of this KVS node |
| `server.private_ip` | Yes | — | Private IP of this KVS node |
| `server.seed_ip` | Yes | — | IP of the seed node to join |
| `server.scaling_alert_ip` | Yes | — | Scaling alert endpoint IP (or "NULL" for standalone) |
| `server.routing` | No | — | List of routing node IPs (only needed when using the legacy `anna-route` server) |
| `server.monitoring` | Yes | — | List of monitoring node IPs |

### Ports (`ports:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `ports.base_offset` | `kBaseOffset` | `0` | Offset added to all port numbers |
| `ports.scaling_alert` | `kScalingAlertPort` | `6955` | Port on which the external system receives scaling alerts |

## Storage Configuration

### Disk Root (`disk:`)

| Config Key | Required | Default | Meaning |
|------------|----------|---------|---------|
| `disk` | Yes | — | Root directory for disk-tier storage |

### Capacities (`capacities:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `capacities.memory-cap` | `kMemoryNodeCapacity` | — | Memory tier capacity per node (GB, multiplied by 1M for KB) |
| `capacities.memory-cap-kb` | `kMemoryNodeCapacity` | — | Memory tier capacity per node (KB, used directly) |
| `capacities.disk-cap` | `kDiskNodeCapacity` | — | Disk tier capacity per node (GB) |
| `capacities.disk-cap-kb` | `kDiskNodeCapacity` | — | Disk tier capacity per node (KB) |

If both `-cap` and `-cap-kb` variants are present, the `-kb` variant takes precedence.

## Thread Configuration (`threads:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `threads.memory` | `kMemoryThreadCount` | — | Threads per memory-tier node |
| `threads.disk` | `kDiskThreadCount` | — | Threads per disk-tier node |
| `threads.routing` | `kRoutingThreadCount` | — | Threads per routing node (only relevant when using `anna-route`) |
| `threads.benchmark` | — | — | Benchmark threads (not used by server) |

## Hashing Configuration (`hashing:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `hashing.virtual_nodes_per_thread` | `kVirtualThreadNum` | `3000` | Virtual nodes per thread in consistent hash ring (must be > 0) |

## Replication Configuration (`replication:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `replication.memory` | `kDefaultGlobalMemoryReplication` | — | Default global replication factor for memory tier |
| `replication.disk` | `kDefaultGlobalDiskReplication` | — | Default global replication factor for disk tier |
| `replication.local` | `kDefaultLocalReplication` | — | Default local (intra-node) replication factor |
| `replication.minimum` | `kMinimumReplicaNumber` | — | Minimum total replica count |
| `replication.metadata` | `kMetadataReplicationFactor` | `1` | Global replication factor for metadata keys |
| `replication.metadata_local` | `kMetadataLocalReplicationFactor` | `1` | Local replication factor for metadata keys |

## Timing Configuration (`timings:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `timings.server_report_period` | `kServerReportThreshold` | `15` | Server stats report period (seconds) |
| `timings.key_monitoring_period` | `kKeyMonitoringThreshold` | `60` | Key access monitoring period (seconds) |
| `timings.monitoring_timeout` | `kMonitoringThreshold` | `30` | Monitoring response timeout (seconds) |
| `timings.monitoring_response_timeout_ms` | — | `10000` | Monitoring ZMQ poll timeout (milliseconds) |
| `timings.gossip_epoch` | `kGossipPeriod` | `10` | Gossip period (seconds, stored internally as microseconds) |
| `timings.data_redistribute_batch` | `kDataRedistributeThreshold` | `50` | Max keys redistributed per gossip batch |
| `timings.tombstone_gc_multiplier` | `kTombstoneGcMultiplier` | `30` | Tombstone GC: multiplier of gossip period |
| `timings.grace_period` | `kGracePeriod` | `120` | Grace period after scaling (seconds) |
| `timings.garbage_collect_period_us` | `kGarbageCollectThreshold` | `10000000` | Memory GC trigger interval (microseconds) |

## Policy Configuration (`policy:`)

These parameters control the autoscaling policy engine. See
[Autoscaling and Policy Engine](autoscaling.md) for how they are used.

### Top-level Policy Keys

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `policy.elasticity` | `kEnableElasticity` | `false` | Enable horizontal elasticity |
| `policy.selective-rep` | `kEnableSelectiveRep` | `false` | Enable selective hot-key replication |
| `policy.tiering` | `kEnableTiering` | `false` | Enable vertical tiering |
| `policy.node_addition_batch_size` | `kNodeAdditionBatchSize` | `2` | Nodes added concurrently during scaling |
| `policy.assumed_value_size_kb` | `kValueSize` | `256` | Assumed value size (KB) for capacity calculations |
| `policy.min_memory_nodes` | `kMinMemoryTierSize` | `1` | Minimum number of memory-tier nodes |
| `policy.min_disk_nodes` | `kMinDiskTierSize` | `0` | Minimum number of disk-tier nodes |
| `policy.warmup_key_count` | `kWarmupKeyCount` | `1000000` | Keys to pre-populate in replication map (max 99,999,999) |

### Storage Thresholds (`policy.storage:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `policy.storage.memory_upper` | `kMaxMemoryNodeConsumption` | `0.6` | Upper storage threshold for memory tier (0.0-1.0) |
| `policy.storage.memory_lower` | `kMinMemoryNodeConsumption` | `0.3` | Lower storage threshold for memory tier (0.0-1.0) |
| `policy.storage.disk_upper` | `kMaxDiskNodeConsumption` | `0.75` | Upper storage threshold for disk tier (0.0-1.0) |
| `policy.storage.disk_lower` | `kMinDiskNodeConsumption` | `0.5` | Lower storage threshold for disk tier (0.0-1.0) |

### Tiering Thresholds (`policy.tiering_thresholds:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `policy.tiering_thresholds.promotion_threshold` | `kKeyPromotionThreshold` | `0` | Access count to trigger key promotion to memory |
| `policy.tiering_thresholds.demotion_threshold` | `kKeyDemotionThreshold` | `1` | Access count below which key is demoted to disk |

### SLO Parameters (`policy.slo:`)

| Config Key | C++ Variable | Default | Meaning |
|------------|-------------|---------|---------|
| `policy.slo.latency_target_us` | `kSloWorst` | `3000` | SLO worst-case latency target (microseconds, must be > 0) |
| `policy.slo.occupancy_upper` | `kSloOccupancyUpper` | `0.15` | Min memory occupancy to trigger node addition (0.0-1.0) |
| `policy.slo.occupancy_lower` | `kSloOccupancyLower` | `0.05` | Max memory occupancy to trigger node removal (0.0-1.0) |

## Example Configuration

```yaml
monitoring:
  scaling_alert_ip: 127.0.0.1
  ip: 127.0.0.1
routing:
  monitoring:
    - 127.0.0.1
  ip: 127.0.0.1
server:
  monitoring:
    - 127.0.0.1
  routing:
    - 127.0.0.1
  seed_ip: 127.0.0.1
  public_ip: 127.0.0.1
  private_ip: 127.0.0.1
  scaling_alert_ip: "NULL"
policy:
  elasticity: false
  selective-rep: false
  tiering: false
  node_addition_batch_size: 2
  assumed_value_size_kb: 256
  min_memory_nodes: 1
  min_disk_nodes: 0
  warmup_key_count: 1000000
  storage:
    memory_upper: 0.6
    memory_lower: 0.3
    disk_upper: 0.75
    disk_lower: 0.5
  tiering_thresholds:
    promotion_threshold: 0
    demotion_threshold: 1
  slo:
    latency_target_us: 3000
    occupancy_upper: 0.15
    occupancy_lower: 0.05
disk: ./
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
hashing:
  virtual_nodes_per_thread: 3000
ports:
  base_offset: 0
  scaling_alert: 6955
timings:
  server_report_period: 15
  key_monitoring_period: 60
  monitoring_timeout: 30
  gossip_epoch: 10
  data_redistribute_batch: 50
  tombstone_gc_multiplier: 30
  grace_period: 120
  garbage_collect_period_us: 10000000
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
  metadata: 1
  metadata_local: 1
```
