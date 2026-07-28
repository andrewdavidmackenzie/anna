#!/usr/bin/env bash
#
# Run benchmarks across all client languages against a local anna cluster.
# Usage: ./scripts/bench.sh [--keys N] [--value-size N] [--duration N]
#
# This script:
# 1. Starts a local anna cluster (monitor, route, kvs)
# 2. Runs the bench command for each client (C++, Rust, Python, Go)
# 3. Stops the cluster and cleans up
#
# All binaries are expected to already be built in release mode
# (see: make bench-build).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH_DATA="/tmp/anna_bench_data"
BENCH_CONFIG="/tmp/anna_bench_config.yml"
LOG_DIR="/tmp/anna_bench_logs"

# Default benchmark parameters
KEYS=1000
VALUE_SIZE=256
DURATION=10

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --keys)      KEYS="$2"; shift 2 ;;
    --value-size) VALUE_SIZE="$2"; shift 2 ;;
    --duration)  DURATION="$2"; shift 2 ;;
    *) echo "Unknown argument: $1"; exit 1 ;;
  esac
done

# Paths to release binaries
CPP_CLI="${REPO_ROOT}/clients/cpp/release-build/cli/anna-cli"
RUST_CLI="${REPO_ROOT}/target/release/anna"
GO_CLI="${REPO_ROOT}/target/anna-go"
SERVER_DIR="${REPO_ROOT}/server/cpp/release-build/target/kvs"

# Check binaries exist
for bin in "$CPP_CLI" "$RUST_CLI" "$GO_CLI" \
           "$SERVER_DIR/anna-monitor" "$SERVER_DIR/anna-route" "$SERVER_DIR/anna-kvs"; do
  if [[ ! -x "$bin" ]]; then
    echo "Error: $bin not found. Run 'make bench-build' first."
    exit 1
  fi
done

cleanup() {
  echo ""
  echo "Stopping cluster..."
  pkill -f "anna-monitor.*bench_config" 2>/dev/null || true
  pkill -f "anna-route.*bench_config" 2>/dev/null || true
  pkill -f "anna-kvs.*bench_config" 2>/dev/null || true
  sleep 1
  rm -rf "$BENCH_DATA" "$BENCH_CONFIG" "$LOG_DIR"
}

trap cleanup EXIT

# Kill any existing anna processes to avoid port conflicts
pkill -f "anna-monitor" 2>/dev/null || true
pkill -f "anna-route" 2>/dev/null || true
pkill -f "anna-kvs" 2>/dev/null || true
sleep 1

# Create config
mkdir -p "$BENCH_DATA" "$LOG_DIR"
cat > "$BENCH_CONFIG" << EOF
monitoring:
  mgmt_ip: 127.0.0.1
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
  mgmt_ip: "NULL"
disk: ${BENCH_DATA}
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
policy:
  elasticity: false
  selective-rep: false
  tiering: false
EOF

# Start cluster
echo "Starting anna cluster..."
"$SERVER_DIR/anna-monitor" --config "$BENCH_CONFIG" > "$LOG_DIR/monitor.log" 2>&1 &
sleep 1
"$SERVER_DIR/anna-route" --config "$BENCH_CONFIG" > "$LOG_DIR/route.log" 2>&1 &
sleep 1
"$SERVER_DIR/anna-kvs" --config "$BENCH_CONFIG" > "$LOG_DIR/kvs.log" 2>&1 &
sleep 3

# Verify cluster is running
for proc in anna-monitor anna-route anna-kvs; do
  if ! pgrep -f "${proc}.*bench_config" > /dev/null; then
    echo "Error: $proc failed to start. Check $LOG_DIR/"
    cat "$LOG_DIR"/*.log
    exit 1
  fi
done
echo "Cluster ready"
echo ""
echo "Benchmark parameters: keys=$KEYS, value_size=$VALUE_SIZE, duration=${DURATION}s"
echo ""

# Run benchmarks
echo "========================================"
echo "  C++ Client (Release)"
echo "========================================"
echo ""
"$CPP_CLI" --routing 127.0.0.1 --client-ip 127.0.0.1 \
  --keys "$KEYS" --value-size "$VALUE_SIZE" --duration "$DURATION" bench
echo ""

echo "========================================"
echo "  Rust Client (Release)"
echo "========================================"
echo ""
"$RUST_CLI" --routing tcp://127.0.0.1:6450 --client-ip 127.0.0.1 \
  bench --keys "$KEYS" --value-size "$VALUE_SIZE" --duration "$DURATION"
echo ""

echo "========================================"
echo "  Python Client (CPython)"
echo "========================================"
echo ""
PYTHONPATH="${REPO_ROOT}/clients/python" python3 -m anna \
  --routing 127.0.0.1 --client-ip 127.0.0.1 \
  --keys "$KEYS" --value-size "$VALUE_SIZE" --duration "$DURATION" bench
echo ""

echo "========================================"
echo "  Go Client"
echo "========================================"
echo ""
"$GO_CLI" --routing tcp://127.0.0.1:6450 --client-ip 127.0.0.1 \
  --keys "$KEYS" --value-size "$VALUE_SIZE" --duration "$DURATION" bench
echo ""

echo "========================================"
echo "  All benchmarks complete"
echo "========================================"
