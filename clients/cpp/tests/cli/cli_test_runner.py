import subprocess
import time
import os
import sys
import shutil
import socket
import signal
import re

def normalize_set_line(line):
    """Sort tokens inside { ... } so set iteration order doesn't matter."""
    m = re.match(r'^(\{ )(.+)( \})$', line)
    if m:
        tokens = m.group(2).split()
        return m.group(1) + ' '.join(sorted(tokens)) + m.group(3)
    return line

def run_cli_smoke_test():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    repo_root = os.path.normpath(os.path.join(script_dir, "..", "..", "..", ".."))

    cli_binary = os.environ.get("ANNA_CLI_PATH")
    if not cli_binary:
        cli_binary = os.path.join(repo_root, "clients", "cpp", "build", "cli", "anna-cli")

    server_dir = os.environ.get("ANNA_SERVER_PATH")
    if not server_dir:
        server_dir = os.path.join(repo_root, "server", "cpp", "build", "target", "kvs")

    if not os.path.exists(cli_binary):
        print(f"Error: CLI binary {cli_binary} not found.")
        print("Build the client first or set ANNA_CLI_PATH.")
        sys.exit(1)

    if not os.path.exists(server_dir):
        print(f"Error: Server directory {server_dir} not found.")
        print("Build the server first or set ANNA_SERVER_PATH.")
        sys.exit(1)

    shared_dir = os.path.join(repo_root, "tests", "shared", "cli")
    input_file = os.path.join(shared_dir, "input")
    expected_file = os.path.join(shared_dir, "expected")
    test_config = "test_config.yml"
    test_data = "test_data"
    output_file = "test.output"
    err_file = "test.err"

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

    env = os.environ.copy()
    env["PATH"] = server_dir + ":" + env.get("PATH", "")

    procs = []
    try:
        for name in ["anna-monitor", "anna-route", "anna-kvs"]:
            bin_path = os.path.join(server_dir, name)
            proc = subprocess.Popen(
                [bin_path, "--config", test_config],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            procs.append(proc)
            time.sleep(1)

        timeout_secs = 30
        start_time = time.time()
        while time.time() - start_time < timeout_secs:
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(1.0)
                    if s.connect_ex(("127.0.0.1", 6450)) == 0:
                        break
            except Exception:
                pass
            time.sleep(1)
        else:
            print("Error: Routing tier did not start within 30 seconds")
            sys.exit(1)

        time.sleep(3)

        print(f"Running CLI smoke test: {cli_binary} --config {test_config} cli {input_file}")
        with open(output_file, "w") as out_f, open(err_file, "w") as err_f:
            result = subprocess.run(
                [cli_binary, "--config", test_config, "cli", input_file],
                stdout=out_f,
                stderr=err_f,
                env=env,
                timeout=60
            )

        if os.path.exists(err_file) and os.path.getsize(err_file) > 0:
            with open(err_file, "r") as f:
                stderr_content = f.read()
            if stderr_content.strip():
                print("--- CLI stderr ---")
                print(stderr_content)

        with open(output_file, "r") as f:
            actual_lines = [normalize_set_line(line.rstrip()) for line in f if line.strip()]

        with open(expected_file, "r") as f:
            expected_lines = [normalize_set_line(line.rstrip()) for line in f if line.strip()]

        if actual_lines == expected_lines:
            print("CLI smoke test PASSED!")
        else:
            print("CLI smoke test FAILED!")
            print(f"\nExpected ({len(expected_lines)} lines):")
            for line in expected_lines:
                print(f"  {line}")
            print(f"\nActual ({len(actual_lines)} lines):")
            for line in actual_lines:
                print(f"  {line}")

            print("\nDifferences:")
            max_lines = max(len(expected_lines), len(actual_lines))
            for i in range(max_lines):
                exp = expected_lines[i] if i < len(expected_lines) else "<missing>"
                act = actual_lines[i] if i < len(actual_lines) else "<missing>"
                if exp != act:
                    print(f"  Line {i+1}: expected '{exp}' got '{act}'")

            sys.exit(1)

    finally:
        print("Cleaning up...")
        for proc in procs:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            except Exception:
                pass

        for f in [test_config, output_file, err_file, "client_log.txt"]:
            if os.path.exists(f):
                os.remove(f)
        if os.path.exists(test_data):
            shutil.rmtree(test_data)

if __name__ == "__main__":
    run_cli_smoke_test()
