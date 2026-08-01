# Port Layout

Anna uses ZeroMQ TCP sockets for all inter-component communication. Every port
in the system follows the formula:

```
actual_port = base_port + tid + base_offset
```

- `base_port`: a constant defined in `kvs_threads.hpp` or `threads.hpp`
- `tid`: the thread index (0 to N-1 for per-thread ports; omitted for singletons)
- `base_offset`: a global offset from the YAML config (`ports.base_offset`),
  allowing multiple independent clusters on the same machine

## Port Map

### KVS Server (10 per-thread ports)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6000 | `kNodeJoinPort` | Receive new node join announcements |
| 6050 | `kNodeDepartPort` | Receive node departure notifications |
| 6100 | `kSelfDepartPort` | Receive self-depart commands from monitor |
| 6150 | `kServerReplicationResponsePort` | Receive replication factor query responses |
| 6200 | `kKeyRequestPort` | Receive client GET/PUT/DELETE requests |
| 6250 | `kGossipPort` | Receive gossip from peer KVS nodes |
| 6300 | `kServerReplicationChangePort` | Receive replication factor changes from monitor |
| 7050 | `kCacheIpResponsePort` | Receive cache IP lookup responses |
| 7100 | `kManagementNodeResponsePort` | Receive management node responses (function node lists) |
| 7200 | `kCacheRegistrationPort` | Receive cache registration messages from ValueChangeSubscriber clients |

### Routing Server (5 per-thread ports)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6350 | `kSeedPort` | Cluster membership requests (REQ/REP, bootstrap) |
| 6400 | `kRoutingNotifyPort` | Join/depart membership change notifications |
| 6450 | `kKeyAddressPort` | Key-address lookup requests from clients |
| 6500 | `kRoutingReplicationResponsePort` | Replication factor query responses |
| 6550 | `kRoutingReplicationChangePort` | Replication factor change announcements from monitor |

### Monitor (4 singleton ports, no tid)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6600 | `kMonitoringNotifyPort` | Cluster membership change notifications |
| 6650 | `kMonitoringResponsePort` | KVS metadata/stats responses |
| 6700 | `kDepartDonePort` | Depart-done confirmations from departing nodes |
| 6750 | `kFeedbackReportPort` | Client latency/throughput feedback (LatencyReporter) |

### Client (3 per-thread ports)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6800 | `kUserResponsePort` | Receive GET/PUT responses from KVS |
| 6850 | `kUserKeyAddressPort` | Receive key-address responses from routing |
| 7150 | `kCacheUpdatePort` | Receive pushed key-value updates (ValueChangeSubscriber) |

### Benchmark (1 per-thread port)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6900 | `kBenchmarkCommandPort` | Receive benchmark trigger commands |

### Management / External System (3 singleton ports, no tid)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 7000 | `kManagementRestartCountPort` | KVS queries restart count on startup |
| 7001 | `kScalingAlertPort` | Monitor sends scaling alerts (add/remove nodes) |
| 7002 | `kManagementFuncNodesPort` | KVS queries function/cache node lists |

These ports are on the external management system, not on Anna nodes. The
management system is not part of this project. See
[Autoscaling](autoscaling.md) for details.

## Port Range Summary

With `base_offset=0` and 1 thread per component:

- **Minimum port**: 6000 (`kNodeJoinPort`)
- **Maximum port**: 7200 (`kCacheRegistrationPort`)
- **Total span**: 1201 ports (but only 26 actually bound)

The 50-port spacing between consecutive port groups (6000, 6050, 6100, ...)
allows up to **50 threads** per component before per-thread port ranges
collide. This limit is implicit -- there is no constant defining it and
no runtime validation. Configuring more than 50 threads will cause silent
port collisions.

### Why the range is 1201 and not smaller

The 26 port groups are not packed contiguously. The layout has gaps:

- Ports 6000-6900: 19 groups, mostly contiguous at 50-port spacing (950 span)
- Ports 7000-7002: 3 management singletons (3 span)
- Ports 7050-7200: 4 more groups at 50-port spacing (150 span)

The 100-port gap between 6900 (benchmark) and 7000 (management) and the
48-port gap between 7002 and 7050 waste 148 ports. Renumbering these groups
to be contiguous would reduce the range from 1201 to approximately **1053**
(21 per-thread groups x 50 spacing + 5 singletons).

Further reduction would require reducing the inter-group spacing from 50,
which limits the maximum thread count. With spacing of 10 (max 10 threads),
the range shrinks to approximately **260**. With spacing of 4 (max 4
threads), approximately **104**.

## base_offset for Multiple Clusters

The `ports.base_offset` config key shifts all ports by a fixed amount:

```yaml
ports:
  base_offset: 0      # default: ports 6000-7200
  # base_offset: 2000 # shifts to ports 8000-9200
```

This is used in tests to run multiple independent clusters on the same
machine. Each cluster needs a `base_offset` at least 1201 apart (with 1
thread) to avoid port conflicts.

## Multi-Node Deployments

In production, multiple KVS nodes on different machines use the **same**
`base_offset` (typically 0). Port conflicts are avoided because ZMQ sockets
bind to the node's specific IP address, not `0.0.0.0`. Multiple nodes can
coexist on the same LAN without port conflicts as long as each has a distinct
IP.

For local development with multiple nodes on `localhost`, the loopback range
`127.0.0.0/8` provides 16 million distinct IPs. Linux supports this by
default. macOS requires adding aliases:

```bash
sudo ifconfig lo0 alias 127.0.0.2
```

## Source Locations

Port constants are defined in:

- `server/cpp/src/kvs/kvs_threads.hpp` -- KVS, routing, monitoring, benchmark, management ports
- `server/cpp/src/threads.hpp` -- client ports, key-address port, scaling alert port
- `clients/rust/src/lib/threads.rs` -- Rust client port constants
- `clients/go/annalib/threads.go` -- Go client port constants
- `clients/cpp/src/anna_client.hpp` -- C++ client port constants
- `clients/python/anna/common.py` -- Python client port constants

Note: the Python client uses different port bases for its own sockets
(6460 and 6760) compared to the other clients (6800 and 6850). This is a
historical divergence from the upstream project.
