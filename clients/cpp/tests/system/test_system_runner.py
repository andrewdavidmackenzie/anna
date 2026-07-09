import subprocess
import time
import os
import sys
import shutil

def run_system_tests():
    server_bin = os.environ.get("ANNA_SERVER_PATH")
    if not server_bin:
        print("Error: ANNA_SERVER_PATH environment variable not set.")
        sys.exit(1)

    test_config = "test_config.yml"
    test_data = "test_data"
    log_file = "server.log"

    # Create a minimal config for the test
    with open(test_config, "w") as f:
        f.write("""
ebs: test_data
capacities:
  memory-cap: 1
  ebs-cap: 1
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

    print(f"Starting server: {server_bin} --config {test_config}")
    # Start the server
    server_proc = subprocess.Popen(
        [server_bin, "--config", test_config],
        stdout=open(log_file, "w"),
        stderr=subprocess.STDOUT,
        preexec_fn=os.setpgrp # Run in a new process group so we can kill it easily
    )

    try:
        # Wait for server to be ready (simple sleep)
        print("Waiting for server to start...")
        time.sleep(5)

        # Run the system tests
        print("Running system tests...")
        # We assume system_tests is in the current directory
        # and it's the compiled binary.
        result = subprocess.run(["./system_tests"], capture_output=True, text=True)
        
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
        # Kill the server process group
        print("Cleaning up...")
        try:
            os.killpg(os.getpgid(server_proc.pid), subprocess.signal.SIGTERM)
        except Exception as e:
            print(f"Error killing server: {e}")
        
        if os.path.exists(test_config):
            os.remove(test_config)
        if os.path.exists(log_file):
            os.remove(log_file)
        if os.path.exists(test_data):
            shutil.rmtree(test_data)

if __name__ == "__main__":
    run_system_tests()
