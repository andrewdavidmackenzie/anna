import os
import re
import signal
import socket
import subprocess
import sys
import time

import pytest

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(SCRIPT_DIR, "..", "..", ".."))
SHARED_DIR = os.path.join(REPO_ROOT, "tests", "shared", "cli")
INPUT_FILE = os.path.join(SHARED_DIR, "input")
EXPECTED_FILE = os.path.join(SHARED_DIR, "expected")


def find_server_dir():
    server_dir = os.environ.get("ANNA_SERVER_PATH")
    if not server_dir:
        server_dir = os.path.join(REPO_ROOT, "server", "cpp", "build", "target", "kvs")
    return server_dir


def server_binaries_exist():
    server_dir = find_server_dir()
    return all(
        os.path.exists(os.path.join(server_dir, b))
        for b in ["anna-monitor", "anna-route", "anna-kvs"]
    )


def normalize_set_line(line):
    m = re.match(r'^(\{ )(.+)( \})$', line)
    if m:
        tokens = m.group(2).split()
        return m.group(1) + ' '.join(sorted(tokens)) + m.group(3)
    return line


TEST_CONFIG_YAML = """
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
ebs: {ebs_dir}
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
"""


@pytest.fixture(scope="module")
def cli_server(tmp_path_factory):
    if not server_binaries_exist():
        pytest.skip("Server binaries not found")

    server_dir = find_server_dir()
    work_dir = tmp_path_factory.mktemp("anna_cli_smoke")
    ebs_dir = str(work_dir / "test_data")
    os.makedirs(ebs_dir, exist_ok=True)

    config_path = str(work_dir / "test_config.yml")
    with open(config_path, "w") as f:
        f.write(TEST_CONFIG_YAML.format(ebs_dir=ebs_dir))

    log_path = str(work_dir / "server.log")
    procs = []

    for name in ["anna-monitor", "anna-route", "anna-kvs"]:
        bin_path = os.path.join(server_dir, name)
        proc = subprocess.Popen(
            [bin_path, "--config", config_path],
            stdout=open(log_path, "a"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        procs.append(proc)
        time.sleep(1)

    timeout = 30
    start = time.time()
    ready = False
    while time.time() - start < timeout:
        for proc in procs:
            if proc.poll() is not None:
                pytest.fail(f"Server exited with {proc.returncode}")
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(1.0)
                if s.connect_ex(("127.0.0.1", 6450)) == 0:
                    ready = True
                    break
        except Exception:
            pass
        time.sleep(1)

    if not ready:
        for proc in procs:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            except Exception:
                pass
        pytest.fail("Server failed to start")

    time.sleep(3)

    yield config_path

    for proc in procs:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        except Exception:
            pass


class TestCliSmoke:
    def test_golden_file_output(self, cli_server):
        result = subprocess.run(
            [sys.executable, "-m", "anna", "--config", cli_server, "cli", INPUT_FILE],
            capture_output=True, text=True, timeout=60,
            cwd=os.path.join(REPO_ROOT, "clients", "python"),
        )

        actual = [normalize_set_line(line.rstrip()) for line in result.stdout.splitlines() if line.strip()]

        with open(EXPECTED_FILE) as f:
            expected = [normalize_set_line(line.rstrip()) for line in f if line.strip()]

        assert actual == expected, (
            f"Output mismatch:\n"
            f"Expected: {expected}\n"
            f"Actual:   {actual}\n"
            f"Stderr:   {result.stderr}"
        )
