#!/bin/bash
#
# Entrypoint for the Anna Docker container.
#
# Starts server components (monitor, kvs) using the config file at
# /etc/anna/anna-config.yml. Override the config by mounting a custom file:
#
#   docker run -v ./my-config.yml:/etc/anna/anna-config.yml anna
#
# Or run a single component:
#
#   docker run anna anna-kvs --config /etc/anna/anna-config.yml
#
set -e

CONFIG="${ANNA_CONFIG:-/etc/anna/anna-config.yml}"

# If the user passes a command (e.g., "anna-kvs --config ..."), run it directly.
if [ "$1" = "anna-kvs" ] || [ "$1" = "anna-monitor" ]; then
    exec "$@"
fi

# Graceful shutdown: forward SIGTERM/SIGINT to child processes.
shutdown() {
    echo "Received shutdown signal, stopping Anna cluster..."
    kill "$KVS_PID" "$MONITOR_PID" 2>/dev/null || true
    wait "$KVS_PID" "$MONITOR_PID" 2>/dev/null || true
    exit 0
}
trap shutdown SIGTERM SIGINT

echo "Starting Anna cluster..."
echo "Config: $CONFIG"

# Start monitor first (other components notify it on join)
anna-monitor --config "$CONFIG" &
MONITOR_PID=$!
sleep 1

# Start KVS (self-seeding: first node starts with empty membership)
anna-kvs --config "$CONFIG" &
KVS_PID=$!

echo "Anna cluster started (monitor=$MONITOR_PID, kvs=$KVS_PID)"

# Wait for any process to exit, then shut down the others
wait -n "$MONITOR_PID" "$KVS_PID" 2>/dev/null || true

echo "A server process exited, shutting down..."
kill "$KVS_PID" "$MONITOR_PID" 2>/dev/null || true
wait
