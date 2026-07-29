# Latency Feedback API and Client Config Migration

**Issue:** [#409](https://github.com/andrewdavidmackenzie/anna/issues/409)
**Date:** 2026-07-21
**Status:** Design

## Overview

Add a `LatencyReporter` API to all four client libraries (Rust, Python, Go, C++)
enabling clients to report latency feedback to the anna monitor for SLO enforcement.
As part of this work, migrate all client APIs away from server-side `Config` files
to a minimal `ClientConfig` struct, establishing the principle that config files are
a server-side-only mechanism.

## 1. ClientConfig

Replace the full `Config` struct in all client-facing APIs with a minimal bootstrap
struct containing only what a client needs to connect:

```
ClientConfig {
    routing_addresses: [String]   // e.g. ["tcp://10.0.0.1:6450"]
    client_ip: String             // IP this client binds on
}
```

- **Base offset** is derived from the first routing address port: `port - 6450`.
- **Routing thread count** defaults to 1 (follow-up #413 addresses runtime discovery).
- **Memory thread count** defaults to 1 (follow-up #413).

## 2. LatencyReporter API

New client-side API for reporting latency feedback to the monitor, following the
`ValueChangeSubscriber` pattern.

### API shape (all languages)

```
LatencyReporter::new(client: &mut KVSClient, tid: Option<usize>) -> Result<Self>
    // Discovers monitoring IPs via ANNA_METADATA|monitoring_ips
    // Connects ZMQ PUSH sockets to each monitor's feedback port (6750 + offset)

report(latency_us: f64, throughput: f64, key_latencies: &[(String, f64)]) -> Result<()>
    // Builds UserFeedback protobuf, sends to all monitors
    // UID auto-generated from client_ip:tid

set_warmup(warmup: bool)
    // Controls the warmup flag on subsequent reports

finish() -> Result<()>
    // Sends UserFeedback with finish=true
```

### Protobuf

`UserFeedback` (from `benchmark.proto`) is already compiled into the Rust client at
`proto::metadata::UserFeedback`. Fields: `uid`, `latency`, `throughput`, `finish`,
`warmup`, `key_latency` (repeated `KeyLatency` with `key` + `latency`).

No protobuf changes needed.

### Socket pattern

- ZMQ PUSH sockets to each monitor's feedback address (`tcp://<ip>:6750+offset`)
- Lazy connection with retry (same `get_or_connect` pattern as `ValueChangeSubscriber`)
- Fan-out: every report goes to all monitoring threads

## 3. Monitoring IP Discovery

The monitor writes its IP to a well-known metadata key on startup:

- **Key:** `ANNA_METADATA|monitoring_ips`
- **Value:** Serialized `StringSet` protobuf (from `shared.proto`)
- **Writer:** `anna-monitor` on startup, after joining the cluster
- **Reader:** Any client needing monitoring addresses

### Server-side change

In `monitoring.cpp`, after the monitor completes startup, it PUTs its IP to
`ANNA_METADATA|monitoring_ips` via the KVS using the existing request mechanism.

### Client-side convenience

`KVSClient` gets a `get_monitoring_ips()` method that GETs this metadata key and
deserializes the `StringSet`.

## 4. Config Removal from Client Code

### Files to delete from client libraries

- `clients/rust/src/lib/config.rs` — full `Config` struct and YAML parsing
- `clients/rust/default-config.yml` — client default config file
- `clients/rust/src/lib/test_config.yml` — Config unit test fixture
- `clients/go/annalib/default-config.yml` — Go client default config
- Any YAML config loading in Python/C++ clients

### Dependencies to remove

- `serde_yaml` from Rust client `Cargo.toml`
- `serde` if no longer needed for other purposes
- Equivalent YAML parsing dependencies in other languages

### API migration

- `KVSClient::new(&Config, tid)` → `KVSClient::new(&ClientConfig, tid)`
- `ValueChangeSubscriber::new(&Config, tid)` → `ValueChangeSubscriber::new(&ClientConfig, tid)`
- `LatencyReporter::new(&mut KVSClient, tid)` — born with new pattern

### CLI binary

The `anna` CLI binary (`clients/rust/src/main.rs`) switches from `--config <path>`
to explicit flags:
- `--routing tcp://host:6450` (repeatable for multiple addresses)
- `--client-ip 127.0.0.1`

Operator commands (`start`, `stop`) that need server config retain a `--server-config`
flag for specifying the server-side YAML file.

## 5. Config File Relocation

- `conf/` → `server/conf/` — reference/example configs only
- No code outside `server/` references these files
- Docker entrypoint scripts (if any) generate config at runtime from env vars (no change needed)

## 6. Test Infrastructure Updates

### All system tests generate their own configs

No test references a static config file path. Tests that start server processes
generate YAML in temp directories, following the existing `MonitorTestCluster` pattern.

- `system.rs` / `common/mod.rs` — adopt MonitorTestCluster-style config generation
- `invocation.rs` — generate temp configs for CLI tests
- `clients/go/tests/system_test.go` — generate temp configs

### Client construction in tests

Tests construct `ClientConfig` directly from known parameters:

```rust
let client_config = ClientConfig {
    routing_addresses: vec![format!("tcp://127.0.0.1:{}", 6450 + base_offset)],
    client_ip: "127.0.0.1".to_string(),
};
let mut client = KVSClient::new(&client_config, Some(tid)).await;
```

Test cluster structs expose a helper:

```rust
impl MonitorTestCluster {
    fn client_config(&self) -> ClientConfig { ... }
}
```

## 7. Multi-Node SLO Enforcement Test

Using `MultiNodeCluster` infrastructure:

1. Start 2-node memory cluster with `selective_rep: true`
2. PUT several keys
3. GET one key many times to establish it as "hot"
4. Use `LatencyReporter` to send `UserFeedback` with:
   - `avg_latency > 3000us` (above `kSloWorst`)
   - Per-key latencies showing the hot key above the SLO
5. Wait for monitoring cycle (30s `kMonitoringThreshold`)
6. Query `ANNA_METADATA|replication|<hot_key>` and verify replication factor increased
7. Verify cold key replication factor unchanged

This is the first test that verifies SLO policy actually changes replication factors.

## 8. Codecov Fix

Already committed: fix ignore patterns to use `**/tests/**` (matches at any depth),
add `**/build/**` and `target/**` exclusions.

## Out of Scope

- **Benchmark infrastructure** (issue #409 part 4) — deferred to separate issue
- **Runtime topology discovery** (thread counts etc.) — tracked in #413
- **Server-side Config struct changes** — C++ server continues to parse its own YAML

## References

- `server/protobuf/benchmark.proto` — `UserFeedback` definition
- `server/cpp/src/monitor/feedback_handler.cpp` — server-side feedback processing
- `server/cpp/src/monitor/slo_policy.cpp` — SLO enforcement logic
- `server/cpp/src/monitor/monitoring_utils.hpp` — constants (`kSloWorst = 3000`)
- `clients/rust/src/lib/value_change_subscriber.rs` — reference pattern for new API
- `clients/rust/tests/monitor.rs` — existing `MonitorTestCluster` infrastructure
- `clients/rust/tests/multi_node.rs` — multi-node test infrastructure
