import subprocess
import sys

def run_cli(*args):
    result = subprocess.run(
        [sys.executable, "-m", "anna"] + list(args),
        capture_output=True, text=True, timeout=10
    )
    return result


class TestCliInvocation:
    def test_help_shows_usage(self):
        r = run_cli("--help")
        assert r.returncode == 0
        assert "anna-py" in r.stdout
        assert "start" in r.stdout
        assert "stop" in r.stdout

    def test_stop_with_nothing_running(self):
        r = run_cli("--config", "/dev/null", "stop")
        assert r.returncode == 0
        assert "0 anna processes were stopped" in r.stdout

    def test_status_with_nothing_running(self):
        r = run_cli("--config", "/dev/null", "status")
        assert r.returncode == 0
        assert "not running" in r.stdout


class TestProcessMgmt:
    def test_stop_returns_zero(self):
        from anna.process_mgmt import stop
        assert stop() == 0

    def test_status_returns_empty(self):
        from anna.process_mgmt import status
        assert status() == []
