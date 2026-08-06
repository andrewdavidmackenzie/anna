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

All 26 port groups are packed contiguously: 19 per-thread groups at 50-port
spacing (6000-6900), followed by 7 singleton ports (6950-6956).

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
| 6750 | `kCacheIpResponsePort` | Receive cache IP lookup responses |
| 6800 | `kManagementNodeResponsePort` | Receive management node responses (function node lists) |
| 6900 | `kCacheRegistrationPort` | Receive cache registration messages from ValueChangeSubscriber clients |

### Routing Server (5 per-thread ports) — Optional

These ports are only used when running the legacy `anna-route` server.
Clients using client-side routing do not need these ports.

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6350 | `kSeedPort` | Cluster membership requests (REQ/REP, bootstrap) |
| 6400 | `kRoutingNotifyPort` | Join/depart membership change notifications |
| 6450 | `kKeyAddressPort` | Key-address lookup requests from clients |
| 6500 | `kRoutingReplicationResponsePort` | Replication factor query responses |
| 6550 | `kRoutingReplicationChangePort` | Replication factor change announcements from monitor |

### Client (2 per-thread ports)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6600 | `kUserResponsePort` | Receive GET/PUT responses from KVS |
| 6650 | `kUserKeyAddressPort` | Receive key-address responses from routing (only used with `anna-route`) |

### Other per-thread ports

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6700 | `kBenchmarkCommandPort` | Receive benchmark trigger commands |
| 6850 | `kCacheUpdatePort` | Receive pushed key-value updates (ValueChangeSubscriber) |

### Singleton ports (no tid, packed at end)

| Base Port | Constant | Purpose |
|-----------|----------|---------|
| 6950 | `kMonitoringNotifyPort` | Cluster membership change notifications |
| 6951 | `kMonitoringResponsePort` | KVS metadata/stats responses |
| 6952 | `kDepartDonePort` | Depart-done confirmations from departing nodes |
| 6953 | `kFeedbackReportPort` | Client latency/throughput feedback (LatencyReporter) |
| 6954 | `kManagementRestartCountPort` | KVS queries restart count on startup |
| 6955 | `kScalingAlertPort` | Monitor sends scaling alerts (add/remove nodes) |
| 6956 | `kManagementFuncNodesPort` | KVS queries function/cache node lists |

Management ports (6954-6956) are on the external management system, not on
Anna nodes. The management system is not part of this project. See
[Autoscaling](autoscaling.md) for details.

## Port Range Summary

With `base_offset=0` and 1 thread per component:

- **Minimum port**: 6000 (`kNodeJoinPort`)
- **Maximum port**: 6956 (`kManagementFuncNodesPort`)
- **Total span**: 957 ports (but only 26 actually bound)

The 50-port spacing between consecutive per-thread port groups allows up to
**50 threads** per component before port ranges collide. This limit is
implicit -- there is no constant defining it and no runtime validation.
Configuring more than 50 threads will cause silent port collisions.

## base_offset for Multiple Clusters

The `ports.base_offset` config key shifts all ports by a fixed amount:

```yaml
ports:
  base_offset: 0      # default: ports 6000-6956
  # base_offset: 2000 # shifts to ports 8000-8956
```

This is used in tests to run multiple independent clusters on the same
machine. Each cluster needs a `base_offset` at least 957 apart (with 1
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
(6460 and 6760) compared to the other clients (6600 and 6650). This is a
historical divergence from the upstream project.
