import os
import signal
import socket
import subprocess
import sys
import shutil
import time
import tempfile

import pytest

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(SCRIPT_DIR, "..", "..", ".."))


def find_server_dir():
    server_dir = os.environ.get("ANNA_SERVER_PATH")
    if not server_dir:
        server_dir = os.path.join(REPO_ROOT, "server", "cpp", "build", "target", "kvs")
    return server_dir


def server_binaries_exist():
    server_dir = find_server_dir()
    return all(
        os.path.exists(os.path.join(server_dir, b))
        for b in ["anna-monitor", "anna-kvs"]
    )


TEST_CONFIG_YAML = """
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
policy:
  elasticity: false
  selective-rep: false
  tiering: false
"""


@pytest.fixture(scope="module")
def live_server(tmp_path_factory):
    if not server_binaries_exist():
        pytest.skip("Server binaries not found")

    server_dir = find_server_dir()
    work_dir = tmp_path_factory.mktemp("anna_system_test")
    disk_dir = str(work_dir / "test_data")
    os.makedirs(disk_dir, exist_ok=True)

    config_path = str(work_dir / "test_config.yml")
    with open(config_path, "w") as f:
        f.write(TEST_CONFIG_YAML.format(disk_dir=disk_dir))

    log_path = str(work_dir / "server.log")
    procs = []

    for name in ["anna-monitor", "anna-kvs"]:
        bin_path = os.path.join(server_dir, name)
        proc = subprocess.Popen(
            [bin_path, "--config", config_path],
            stdout=open(log_path, "a"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        procs.append(proc)
        time.sleep(1)

    # Wait for routing tier
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

    for proc in procs:
        try:
            proc.wait(timeout=5)
        except Exception:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except Exception:
                pass


@pytest.fixture
def client(live_server):
    from anna.client import AnnaTcpClient
    c = AnnaTcpClient("127.0.0.1", "127.0.0.1", local=True, offset=0)
    return c


class TestSystemPutGet:
    def test_put_and_get_lww(self, client):
        from anna.lattices import LWWPairLattice
        val = LWWPairLattice(int(time.time()), b"hello")
        result = client.put("sys_test_key", val)
        assert result["sys_test_key"] is True

        got = client.get("sys_test_key")
        assert got["sys_test_key"] is not None
        assert got["sys_test_key"].reveal() == b"hello"

    def test_put_and_get_set(self, client):
        from anna.lattices import SetLattice
        val = SetLattice({b"a", b"b", b"c"})
        result = client.put("sys_test_set", val)
        assert result["sys_test_set"] is True

        got = client.get("sys_test_set")
        assert got["sys_test_set"] is not None
        assert got["sys_test_set"].reveal() == {b"a", b"b", b"c"}

    def test_put_overwrites_lww(self, client):
        from anna.lattices import LWWPairLattice
        ts = int(time.time())
        client.put("sys_overwrite", LWWPairLattice(ts, b"first"))
        client.put("sys_overwrite", LWWPairLattice(ts + 1, b"second"))

        got = client.get("sys_overwrite")
        assert got["sys_overwrite"].reveal() == b"second"

    def test_get_nonexistent_key(self, client):
        got = client.get("sys_nonexistent_key_xyz")
        assert got["sys_nonexistent_key_xyz"] is None

    def test_put_and_get_ordered_set(self, client):
        from anna.lattices import ListBasedOrderedSet, OrderedSetLattice
        result = client.put_ordered_set("sys_oset", ["alpha", "beta", "gamma"])
        assert result["sys_oset"] is True

        got = client.get_ordered_set("sys_oset")
        assert got is not None
        revealed = got.reveal()
        assert len(revealed) == 3
        assert b"alpha" in revealed
        assert b"beta" in revealed
        assert b"gamma" in revealed

    def test_put_and_get_single_causal(self, client):
        from anna.lattices import SingleKeyCausalLattice
        result = client.put_single_causal("sys_sc", "sc_hello")
        assert result["sys_sc"] is True

        got = client.get_single_causal("sys_sc")
        assert got is not None
        assert isinstance(got, SingleKeyCausalLattice)

    def test_put_and_get_causal(self, client):
        from anna.lattices import MultiKeyCausalLattice
        result = client.put_causal("sys_mc", "mc_hello")
        assert result["sys_mc"] is True

        got = client.get_causal("sys_mc")
        assert got is not None
        assert isinstance(got, MultiKeyCausalLattice)

    def test_put_and_get_priority(self, client):
        from anna.lattices import PriorityLattice
        result = client.put_priority("sys_pri", 1.5, "important")
        assert result["sys_pri"] is True

        got = client.get_priority("sys_pri")
        assert got is not None
        assert isinstance(got, PriorityLattice)

    def test_delete(self, client):
        from anna.lattices import LWWPairLattice
        client.put("sys_del", LWWPairLattice(time.time_ns(), b"to_delete"))

        got = client.get("sys_del")
        assert got["sys_del"] is not None
        assert got["sys_del"].reveal() == b"to_delete"

        client.delete("sys_del")
