import subprocess
import signal
import time
import os
import sys
import shutil
import socket

def run_system_tests():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    repo_root = os.path.normpath(os.path.join(script_dir, "..", "..", "..", ".."))

    # Find the server binaries. Check ANNA_SERVER_PATH env var first, then
    # try to locate them relative to this script's location in the repo.
    server_dir = os.environ.get("ANNA_SERVER_PATH")
    if not server_dir:
        server_dir = os.path.join(repo_root, "server", "cpp", "build", "target", "kvs")

    if not os.path.exists(server_dir):
        print(f"Error: Server directory {server_dir} not found.")
        print("Build the server first or set ANNA_SERVER_PATH.")
        sys.exit(1)

    test_config = "test_config.yml"
    test_data = "test_data"
    log_file = "server.log"

    # Create a config with all sections required by anna-kvs, anna-monitor,
    # anna-route, and the C++ client library.
    with open(test_config, "w") as f:
        f.write("""
monitoring:
  mgmt_ip: 127.0.0.1
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
  mgmt_ip: "NULL"
ebs: test_data
capacities:
  memory-cap: 1
  ebs-cap: 0
threads:
  memory: 1
  ebs: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  ebs: 0
  minimum: 1
  local: 1
policy:
  elasticity: false
  selective-rep: false
  tiering: false
""")

    if not os.path.exists(test_data):
        os.makedirs(test_data)

    # Start in dependency order: monitor first, then route, then kvs.
    binaries = ["anna-monitor", "anna-route", "anna-kvs"]
    procs = []

    print(f"Starting servers in {server_dir}...")
    for bin_name in binaries:
        bin_path = os.path.join(server_dir, bin_name)
        if not os.path.exists(bin_path):
            print(f"Warning: {bin_path} not found. Skipping.")
            continue

        print(f"Starting {bin_name}...")
        proc = subprocess.Popen(
            [bin_path, "--config", test_config],
            stdout=open(log_file, "a"),
            stderr=subprocess.STDOUT,
            start_new_session=True
        )
        procs.append(proc)
        time.sleep(1)

    try:
        # Wait for the routing tier to be ready by probing its ZMQ TCP port.
        # kKeyAddressPort (6450) is the port the routing thread binds for
        # key-address lookups from clients.
        routing_port = 6450
        print(f"Waiting for routing tier on port {routing_port}...")
        timeout = 30
        start_time = time.time()
        ready = False
        while time.time() - start_time < timeout:
            for proc in procs:
                if proc.poll() is not None:
                    print(f"Error: Server process exited with code {proc.returncode}")
                    with open(log_file, "r") as lf:
                        print(lf.read())
                    sys.exit(1)
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(1.0)
                    if s.connect_ex(("127.0.0.1", routing_port)) == 0:
                        ready = True
                        break
            except Exception:
                pass
            time.sleep(1)

        if not ready:
            print("Error: Server failed to start within timeout.")
            print("--- Server Log ---")
            with open(log_file, "r") as lf:
                print(lf.read())
            sys.exit(1)

        # Allow time for the KVS server to register with the routing tier
        # and for the hash ring to stabilize.
        print("Waiting for cluster to stabilize...")
        time.sleep(3)

        # Run the system tests
        print("Running system tests...")
        # Search for the system_tests binary in several likely locations.
        candidates = [
            "./system_tests",
            os.path.join(script_dir, "system_tests"),
            os.path.join(repo_root, "clients", "cpp", "build", "tests", "system_tests"),
        ]
        system_tests_path = None
        for path in candidates:
            if os.path.exists(path):
                system_tests_path = path
                break

        if system_tests_path is None:
            print("Error: system_tests binary not found. Searched:")
            for path in candidates:
                print(f"  {path}")
            sys.exit(1)

        result = subprocess.run([system_tests_path], capture_output=True, text=True)
        
        print("--- Test Output ---")
        print(result.stdout)
        if result.stderr:
            print("--- Error Output ---")
            print(result.stderr)
        
        if result.returncode == 0:
            print("System tests PASSED!")
        else:
            print(f"System tests FAILED with return code {result.returncode}")
            sys.exit(result.returncode)

    finally:
        print("Cleaning up...")
        for proc in procs:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            except Exception as e:
                print(f"Error sending SIGTERM to {proc.pid}: {e}")

        for proc in procs:
            try:
                proc.wait(timeout=5)
            except Exception:
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                except Exception:
                    pass
        
        if os.path.exists(test_config):
            os.remove(test_config)
        if os.path.exists(log_file):
            os.remove(log_file)
        if os.path.exists(test_data):
            shutil.rmtree(test_data)

if __name__ == "__main__":
    run_system_tests()
