#!/usr/bin/env python3
"""Shared CLI smoke test runner for all anna clients.

Usage:
    python3 run_smoke_test.py <cli_command> [args...]

Example:
    python3 run_smoke_test.py ./target/anna-go --config conf/anna-config.yml cli
    python3 run_smoke_test.py python3 -m anna --routing 127.0.0.1 --client-ip 127.0.0.1 cli
    python3 run_smoke_test.py ./target/debug/anna --config conf/anna-config.yml cli
    python3 run_smoke_test.py ./clients/cpp/build/cli/anna-cli --config test_config.yml cli

The runner:
1. Starts anna server processes (anna-monitor, anna-kvs)
2. Waits for the KVS seed port (port 6450) to become reachable
3. Runs the CLI with the shared input file
4. Compares stdout against the shared expected output
5. Stops servers and cleans up
"""

import os
import re
import signal
import socket
import subprocess
import sys
import time
import shutil

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
INPUT_FILE = os.path.join(SCRIPT_DIR, "input")
EXPECTED_FILE = os.path.join(SCRIPT_DIR, "expected")


def find_repo_root():
    d = SCRIPT_DIR
    while d != "/":
        if os.path.exists(os.path.join(d, "Makefile")) and os.path.exists(os.path.join(d, "server")):
            return d
        d = os.path.dirname(d)
    return None


def find_server_dir():
    server_dir = os.environ.get("ANNA_SERVER_PATH")
    if not server_dir:
        repo_root = find_repo_root()
        if repo_root:
            server_dir = os.path.join(repo_root, "server", "cpp", "build", "target", "kvs")
    return server_dir


def normalize_set_line(line):
    m = re.match(r'^(\{ )(.+)( \})$', line)
    if m:
        tokens = m.group(2).split()
        return m.group(1) + ' '.join(sorted(tokens)) + m.group(3)
    return line


def write_test_config(path):
    with open(path, "w") as f:
        f.write("""\
monitoring:
  scaling_alert_ip: 127.0.0.1
  ip: 127.0.0.1
routing:
  monitoring:
      - 127.0.0.1
  ip: 127.0.0.1
user:
  monitoring:
      - 127.0.0.1
  routing:
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
disk: test_data
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
ports:
  base_offset: 0
timings:
  server_report_period: 15
  key_monitoring_period: 60
  monitoring_timeout: 30
  gossip_epoch: 10
  data_redistribute_batch: 50
  tombstone_gc_multiplier: 30
  grace_period: 120
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
policy:
  elasticity: false
  selective-rep: false
  tiering: false
""")


def start_servers(server_dir, config_path):
    procs = []
    for name in ["anna-monitor", "anna-kvs"]:
        bin_path = os.path.join(server_dir, name)
        if not os.path.exists(bin_path):
            print(f"SKIP: Server binary {bin_path} not found")
            sys.exit(0)
        proc = subprocess.Popen(
            [bin_path, "--config", config_path],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        procs.append(proc)
        time.sleep(1)

    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(1.0)
                if s.connect_ex(("127.0.0.1", 6200)) == 0:
                    break
        except Exception:
            pass
        time.sleep(1)
    else:
        stop_servers(procs)
        print("FAIL: KVS seed port did not start within 30 seconds")
        sys.exit(1)

    time.sleep(3)
    return procs


def stop_servers(procs):
    for proc in procs:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        except Exception:
            pass

    for proc in procs:
        try:
            proc.wait(timeout=5)
        except Exception:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except Exception:
                pass


def run_smoke_test(cli_cmd):
    server_dir = find_server_dir()
    if not server_dir:
        print("SKIP: Server directory not found")
        sys.exit(0)

    os.makedirs("test_data", exist_ok=True)
    config_path = os.path.abspath("test_config.yml")
    write_test_config(config_path)

    procs = start_servers(server_dir, config_path)

    try:
        resolved_cmd = [config_path if arg == "{CONFIG}" else arg for arg in cli_cmd]
        full_cmd = resolved_cmd + [INPUT_FILE]
        env = os.environ.copy()
        env["PATH"] = server_dir + ":" + env.get("PATH", "")

        result = subprocess.run(
            full_cmd, capture_output=True, text=True, timeout=60, env=env
        )

        actual = [normalize_set_line(line.rstrip()) for line in result.stdout.splitlines() if line.strip()]

        with open(EXPECTED_FILE) as f:
            expected = [normalize_set_line(line.rstrip()) for line in f if line.strip()]

        if actual == expected:
            print("CLI smoke test PASSED!")
        else:
            print("CLI smoke test FAILED!")
            print(f"\nExpected ({len(expected)} lines):")
            for line in expected:
                print(f"  {line}")
            print(f"\nActual ({len(actual)} lines):")
            for line in actual:
                print(f"  {line}")
            if result.stderr.strip():
                print(f"\nStderr:\n{result.stderr}")
            sys.exit(1)

    finally:
        stop_servers(procs)
        for f in ["test_config.yml"]:
            if os.path.exists(f):
                os.remove(f)
        if os.path.exists("test_data"):
            shutil.rmtree("test_data")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <cli_command> [args...]")
        print("The shared input file will be appended as the last argument.")
        sys.exit(1)
    run_smoke_test(sys.argv[1:])
