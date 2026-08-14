#!/usr/bin/env python3
"""Basic example of using the anna Python client library.

This example starts the anna server processes (monitor, route, kvs),
connects a client, performs basic key-value operations (put, get, delete),
and then shuts the server down.

Prerequisites:
    The anna server binaries (anna-monitor, anna-route, anna-kvs) must
    be in your PATH or pointed to by the ANNA_SERVER_PATH environment
    variable. Build them first with `make server-cpp` or `make server-rust`.

Running:
    python clients/python/examples/basic.py
"""

import os
import socket
import sys
import tempfile
import time

# Add the client library to the path
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
CLIENT_DIR = os.path.normpath(os.path.join(SCRIPT_DIR, ".."))
if CLIENT_DIR not in sys.path:
    sys.path.insert(0, CLIENT_DIR)

from anna.client import AnnaTcpClient
from anna.lattices import LWWPairLattice
from anna import process_mgmt

CONFIG_TEMPLATE = """\
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
policy:
  elasticity: false
  selective-rep: false
  tiering: false
disk: {disk_dir}
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
ports:
  base_offset: 0
"""


def wait_for_routing(host="127.0.0.1", port=6450, timeout=30):
    """Wait for the routing tier to accept TCP connections."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(1.0)
                if s.connect_ex((host, port)) == 0:
                    time.sleep(1)
                    return
        except Exception:
            pass
        time.sleep(0.5)
    raise TimeoutError("Routing tier did not start within {} seconds".format(timeout))


def main():
    # Create a temporary config
    work_dir = tempfile.mkdtemp(prefix="anna_example_")
    disk_dir = os.path.join(work_dir, "disk")
    os.makedirs(disk_dir, exist_ok=True)
    config_path = os.path.join(work_dir, "config.yml")
    with open(config_path, "w") as f:
        f.write(CONFIG_TEMPLATE.format(disk_dir=disk_dir))

    # Start the anna server
    print("Starting anna server...")
    count = process_mgmt.start(config_path)
    print(f"  Started {count} processes")

    try:
        wait_for_routing()

        # Connect a client
        client = AnnaTcpClient("127.0.0.1", "127.0.0.1", local=True, offset=0)

        # PUT a value
        ts = time.time_ns()
        print("\nPUT greeting = hello")
        result = client.put("greeting", LWWPairLattice(ts, b"hello"))
        if result.get("greeting") is not True:
            raise RuntimeError("PUT failed")

        # GET it back
        got = client.get("greeting")
        print(f"GET greeting = {got['greeting'].reveal().decode()}")

        # Overwrite the value
        ts = time.time_ns()
        print("\nPUT greeting = hello world")
        client.put("greeting", LWWPairLattice(ts, b"hello world"))

        got = client.get("greeting")
        print(f"GET greeting = {got['greeting'].reveal().decode()}")

        # PUT a second key
        ts = time.time_ns()
        print("\nPUT count = 42")
        client.put("count", LWWPairLattice(ts, b"42"))

        # DELETE the first key
        print("\nDELETE greeting")
        client.delete("greeting")

        # Verify deletion
        got = client.get("greeting")
        if got["greeting"] is None:
            print("GET greeting = (deleted)")
        else:
            print(f"GET greeting = {got['greeting'].reveal().decode()} (unexpected)")

        # GET the remaining key
        got = client.get("count")
        print(f"GET count = {got['count'].reveal().decode()}")

    finally:
        # Stop the server
        print("\nStopping anna server...")
        killed = process_mgmt.stop()
        print(f"  Stopped {killed} processes")

    print("\nDone!")


if __name__ == "__main__":
    main()
