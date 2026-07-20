# Latency Feedback API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add LatencyReporter API to all 4 client libraries, migrate clients from server Config to minimal ClientConfig, add multi-node SLO enforcement test.

**Architecture:** Replace server-side `Config` with a minimal `ClientConfig` (routing addresses + client IP) in all client APIs. Add `LatencyReporter` that discovers monitoring IPs via `ANNA_METADATA|monitoring_ips` metadata key. Monitor writes its IP on startup. Multi-node system test verifies SLO policy changes replication.

**Tech Stack:** Rust (zeromq, prost, tokio), Python (pyzmq, protobuf), Go (go-zeromq/zmq4, protobuf), C++ (cppzmq, protobuf, yaml-cpp removal)

## Global Constraints

- Test ports must stay below 32768 (Linux ephemeral range)
- `#![deny(missing_docs)]` on Rust lib — all public items need doc comments
- Rust edition 2018
- Go 1.23
- Pre-commit: `make clippy`, `make fmt`, `make test`
- Generated protobuf files: Rust auto-generated via build.rs, Python via Makefile protoc, Go checked in

---

### Task 1: Rust — ClientConfig + migrate KVSClient and ValueChangeSubscriber

**Files:**
- Create: `clients/rust/src/lib/client_config.rs`
- Modify: `clients/rust/src/lib/lib.rs` — add module, remove config module
- Modify: `clients/rust/src/lib/kvs_client.rs` — change constructor
- Modify: `clients/rust/src/lib/value_change_subscriber.rs` — change constructor
- Modify: `clients/rust/src/lib/errors.rs` — remove ConfigFile variant
- Modify: `clients/rust/Cargo.toml` — remove serde_yaml, serde_derive, serde
- Delete: `clients/rust/src/lib/config.rs`
- Delete: `clients/rust/src/lib/test_config.yml`
- Delete: `clients/rust/default-config.yml`
- Modify: `clients/rust/tests/system.rs` — use ClientConfig
- Modify: `clients/rust/tests/common/mod.rs` — generate configs, add ClientConfig helper
- Modify: `clients/rust/tests/monitor.rs` — use ClientConfig
- Modify: `clients/rust/tests/multi_node.rs` — use ClientConfig
- Modify: `clients/rust/tests/invocation.rs` — generate temp config

**ClientConfig struct:**
```rust
pub struct ClientConfig {
    pub routing_addresses: Vec<String>,
    pub client_ip: String,
}

impl ClientConfig {
    pub fn base_offset(&self) -> usize {
        // parse port from first routing address, subtract 6450
    }
}
```

**KVSClient::new changes:**
- Replace `config: &Config` with `config: &ClientConfig`
- `base_offset` from `config.base_offset()`
- `routing_thread_count` defaults to 1
- `routing_ips` parsed from `config.routing_addresses`
- `user_ip` from `config.client_ip`

**ValueChangeSubscriber::new changes:**
- Replace `config: &Config` with `config: &ClientConfig`
- `base_offset` from `config.base_offset()`
- `cache_ip` from `config.client_ip`
- `server_ip` from first routing address IP (parsed)
- `memory_threads` defaults to 1

**Test infrastructure changes:**
- `common/mod.rs`: Generate config YAML in temp dir (like MonitorTestCluster pattern), add `client_config(base_offset)` helper that returns ClientConfig
- `system.rs`: Use generated config + ClientConfig
- `monitor.rs`: Add `client_config()` method to MonitorTestCluster
- `multi_node.rs`: Add `client_config()` method to MultiNodeCluster
- `invocation.rs`: Generate temp config for CLI tests

---

### Task 2: Rust — Update CLI binary

**Files:**
- Modify: `clients/rust/src/main.rs` — replace --config with --routing/--client-ip, keep --server-config for start/stop

**Changes:**
- Remove `use annalib::config::Config`
- Add `use annalib::client_config::ClientConfig`
- Replace `--config` arg with `--routing` (repeatable) and `--client-ip` (default "127.0.0.1")
- Keep `--server-config` for `start`/`stop` subcommands (operator commands need server YAML)
- `start()` and `stop()` functions take server config path
- `cli` subcommand constructs ClientConfig from --routing and --client-ip

---

### Task 3: Rust — Add LatencyReporter

**Files:**
- Create: `clients/rust/src/lib/latency_reporter.rs`
- Modify: `clients/rust/src/lib/lib.rs` — add module

**API:**
```rust
pub struct LatencyReporter {
    uid: String,
    base_offset: usize,
    warmup: bool,
    socket_cache: HashMap<Address, PushSocket>,
    monitoring_ips: Vec<Address>,
}

impl LatencyReporter {
    pub async fn new(client: &mut KVSClient, tid: Option<usize>) -> Result<Self>
    // Queries ANNA_METADATA|monitoring_ips, connects PUSH sockets

    pub async fn report(&mut self, latency_us: f64, throughput: f64, key_latencies: &[(String, f64)]) -> Result<()>
    // Builds UserFeedback, sends to all monitors

    pub fn set_warmup(&mut self, warmup: bool)

    pub async fn finish(&mut self) -> Result<()>
    // Sends UserFeedback with finish=true
}
```

**Unit test:** Use ZMQ PULL socket to receive and verify UserFeedback protobuf.

---

### Task 4: C++ server — Monitor writes monitoring IPs metadata

**Files:**
- Modify: `server/cpp/src/monitor/monitoring.cpp` — PUT monitoring IP to metadata key on startup

**Change:** After the monitor starts and joins the cluster, construct a `StringSet` containing the monitor's IP and PUT it to `ANNA_METADATA|monitoring_ips` using the existing request mechanism. Use the `kSelfRequestPort` or similar internal channel.

---

### Task 5: Config file relocation

**Files:**
- Move: `conf/anna-config.yml` → `server/conf/anna-config.yml`
- Move: `conf/anna-local.yml` → `server/conf/anna-local.yml`
- Move: `conf/anna-base.yml` → `server/conf/anna-base.yml`
- Delete: `conf/` directory
- Modify: `dockerfiles/start-anna.sh` — update path if needed

---

### Task 6: Python — Add benchmark.proto, LatencyReporter, remove pyyaml

**Files:**
- Create: `clients/python/anna/latency_reporter.py`
- Create: `clients/python/tests/test_latency_reporter.py`
- Modify: `clients/python/anna/cli.py` — remove yaml import, accept args directly
- Modify: `clients/python/setup.cfg` — remove pyyaml dependency
- Modify: `Makefile` — add `benchmark.proto` to Python protoc compilation

**LatencyReporter API:**
```python
class LatencyReporter:
    def __init__(self, client, tid=0):
        # Queries ANNA_METADATA|monitoring_ips via client
    def report(self, latency_us, throughput, key_latencies):
        # Sends UserFeedback to all monitors
    def set_warmup(self, warmup):
    def finish(self):
        # Sends finish=True
```

**CLI changes:** Replace `load_config()` YAML parsing with direct argument parsing. The CLI already uses argparse-style invocation; change from `--config path` to `--routing addr --client-ip ip`.

---

### Task 7: Go — ClientConfig migration, LatencyReporter, remove yaml

**Files:**
- Create: `clients/go/annalib/client_config.go`
- Create: `clients/go/annalib/latency_reporter.go`
- Create: `clients/go/annalib/latency_reporter_test.go`
- Create: `clients/go/annalib/proto/metadata/metadata.pb.go` — generate from metadata.proto + benchmark.proto
- Modify: `clients/go/annalib/client.go` — change NewKVSClient to take ClientConfig
- Modify: `clients/go/annalib/client_test.go` — update tests
- Delete: `clients/go/annalib/config.go`
- Delete: `clients/go/annalib/config_test.go`
- Delete: `clients/go/annalib/default-config.yml`
- Modify: `clients/go/annalib/go.mod` — remove yaml.v3
- Modify: `clients/go/cmd/anna-go/main.go` — use ClientConfig
- Modify: `clients/go/tests/system_test.go` — generate temp config, use ClientConfig

**Go ClientConfig:**
```go
type ClientConfig struct {
    RoutingAddresses []string
    ClientIP         string
}
func (c *ClientConfig) BaseOffset() int { /* port - 6450 */ }
```

---

### Task 8: C++ client — Remove yaml load_config, add LatencyReporter

**Files:**
- Create: `clients/cpp/src/latency_reporter.hpp`
- Create: `clients/cpp/tests/unit/test_latency_reporter.cpp`
- Modify: `clients/cpp/src/client_lib.hpp` — remove load_config, yaml includes
- Modify: `clients/cpp/src/client_lib.cpp` — remove load_config implementation
- Modify: `clients/cpp/src/cli.cpp` — use direct args instead of config file
- Modify: `clients/cpp/CMakeLists.txt` — remove yaml-cpp, add benchmark.proto to main lib
- Modify: `clients/cpp/tests/unit/test_client_lib.cpp` — remove LoadConfigParsesYaml test
- Modify: `clients/cpp/tests/system/test_system.cpp` — generate config, use ClientConfig directly

**C++ ClientConfig already exists** — just needs `load_config()` removed and `LatencyReporter` added.

---

### Task 9: Multi-node SLO enforcement test

**Files:**
- Modify: `clients/rust/tests/multi_node.rs` — add new test

**Test outline:**
1. Start 2-node memory cluster with `selective_rep: true`, `base_offset` unique
2. PUT 5 keys
3. GET one key 20+ times (make it "hot")
4. Create LatencyReporter, send UserFeedback with latency > 3000μs
5. Sleep for monitoring cycle (~35s to allow 30s threshold + processing)
6. Query `ANNA_METADATA|replication|<hot_key>` via `get_bytes()`
7. Assert hot key replication factor > 1
8. Assert cold key replication factor unchanged

---

## Task Dependencies

```
Task 1 (Rust ClientConfig) ──→ Task 2 (CLI) ──→ Task 3 (LatencyReporter)
                                                        │
Task 4 (C++ monitor metadata) ─────────────────────────→│
Task 5 (Config relocation) ─────────────────────────────→│
Task 6 (Python) ────────────────────────────────────────→│
Task 7 (Go) ────────────────────────────────────────────→│
Task 8 (C++ client) ───────────────────────────────────→│
                                                        ↓
                                              Task 9 (SLO test)
```

Tasks 4-8 can run in parallel after Task 1 is complete. Task 9 depends on Tasks 3 and 4.
