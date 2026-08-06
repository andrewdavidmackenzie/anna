import os
import subprocess
import sys

import pytest

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(SCRIPT_DIR, "..", "..", ".."))
SHARED_RUNNER = os.path.join(REPO_ROOT, "tests", "shared", "cli", "run_smoke_test.py")


def server_binaries_exist():
    server_dir = os.environ.get("ANNA_SERVER_PATH",
        os.path.join(REPO_ROOT, "server", "cpp", "build", "target", "kvs"))
    return all(
        os.path.exists(os.path.join(server_dir, b))
        for b in ["anna-monitor", "anna-kvs"]
    )


class TestCliSmoke:
    def test_golden_file_output(self):
        if not server_binaries_exist():
            pytest.skip("Server binaries not found")

        result = subprocess.run(
            [sys.executable, SHARED_RUNNER,
             sys.executable, "-m", "anna",
             "--routing", "127.0.0.1", "--client-ip", "127.0.0.1", "cli"],
            capture_output=True, text=True, timeout=120,
            cwd=os.path.join(REPO_ROOT, "clients", "python"),
        )

        if result.returncode != 0:
            pytest.fail(
                f"Shared smoke test failed (exit {result.returncode}):\n"
                f"stdout: {result.stdout}\n"
                f"stderr: {result.stderr}"
            )
