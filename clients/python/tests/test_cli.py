import subprocess
import sys
import os
import tempfile

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

    def test_pids_from_name_nonexistent(self):
        from anna.process_mgmt import _pids_from_name
        assert _pids_from_name("nonexistent_process_xyz") == []

    def test_find_binary_on_path(self):
        from anna.process_mgmt import _find_binary
        assert _find_binary("anna-monitor") == "anna-monitor"

    def test_find_binary_with_env(self):
        from anna.process_mgmt import _find_binary
        old = os.environ.get("ANNA_SERVER_PATH")
        os.environ["ANNA_SERVER_PATH"] = "/nonexistent/path"
        result = _find_binary("anna-monitor")
        assert result == "anna-monitor"
        if old:
            os.environ["ANNA_SERVER_PATH"] = old
        else:
            del os.environ["ANNA_SERVER_PATH"]

    def test_start_with_missing_binary(self):
        from anna.process_mgmt import start
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yml', delete=False) as f:
            f.write("threads:\n  routing: 1\n")
            config_path = f.name
        try:
            old = os.environ.get("ANNA_SERVER_PATH")
            os.environ["ANNA_SERVER_PATH"] = "/nonexistent/path"
            count = start(config_path)
            assert count == 0
            if old:
                os.environ["ANNA_SERVER_PATH"] = old
            else:
                del os.environ["ANNA_SERVER_PATH"]
        finally:
            os.unlink(config_path)


class TestLoadConfig:
    def test_load_config_with_routing_list(self):
        from anna.cli import load_config
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yml', delete=False) as f:
            f.write("threads:\n  routing: 1\nuser:\n  ip: 127.0.0.1\n  routing:\n    - 10.0.0.1\n")
            path = f.name
        try:
            elb, ip, count = load_config(path)
            assert ip == "127.0.0.1"
            assert elb == "10.0.0.1"
            assert count == 1
        finally:
            os.unlink(path)

    def test_load_config_with_elb(self):
        from anna.cli import load_config
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yml', delete=False) as f:
            f.write("threads:\n  routing: 2\nrouting-elb: elb.example.com\nuser:\n  ip: 10.0.0.5\n  routing:\n    - unused\n")
            path = f.name
        try:
            elb, ip, count = load_config(path)
            assert elb == "elb.example.com"
            assert ip == "10.0.0.5"
            assert count == 2
        finally:
            os.unlink(path)


class TestCliUsage:
    def test_cli_usage_string(self):
        from anna.cli import cli_usage
        usage = cli_usage()
        assert "GET" in usage
        assert "PUT" in usage
        assert "EXIT" in usage


class TestExecuteCommand:
    def test_empty_line_returns_true(self):
        from anna.cli import execute_command
        assert execute_command(None, None, "") is True
        assert execute_command(None, None, "   ") is True

    def test_exit_returns_false(self):
        from anna.cli import execute_command
        assert execute_command(None, None, "EXIT") is False
        assert execute_command(None, None, "exit") is False

    def test_help_prints_usage(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "HELP")
        assert result is True
        assert "GET" in capsys.readouterr().out

    def test_stop_prints_count(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "STOP")
        assert result is True
        assert "anna processes were stopped" in capsys.readouterr().out

    def test_start_prints_count(self, capsys, tmp_path):
        from anna.cli import execute_command
        config = tmp_path / "test.yml"
        config.write_text("threads:\n  routing: 1\n")
        result = execute_command(None, str(config), "START")
        assert result is True
        assert "anna processes were started" in capsys.readouterr().out

    def test_status_with_nothing_running(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "STATUS")
        assert result is True
        assert capsys.readouterr().out == ""

    def test_unrecognized_command(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "FOOBAR")
        assert result is True
        out = capsys.readouterr().out
        assert "Unrecognized command: FOOBAR" in out
        assert "GET" in out

    def test_get_with_mock_client(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        from anna.lattices import LWWPairLattice

        client = MagicMock()
        client.get.return_value = {"mykey": LWWPairLattice(1, b"hello")}

        result = execute_command(client, None, "GET mykey")
        assert result is True
        assert "hello" in capsys.readouterr().out

    def test_get_key_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get.return_value = {"mykey": None}

        execute_command(client, None, "GET mykey")
        assert "Key not found" in capsys.readouterr().out

    def test_put_with_mock_client(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put.return_value = {"mykey": True}

        result = execute_command(client, None, "PUT mykey myvalue")
        assert result is True
        assert capsys.readouterr().out == ""

    def test_put_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put.return_value = {"mykey": False}

        execute_command(client, None, "PUT mykey myvalue")
        assert "Failure!" in capsys.readouterr().out

    def test_get_set_with_mock_client(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        from anna.lattices import SetLattice

        client = MagicMock()
        client.get.return_value = {"myset": SetLattice({b"x", b"y"})}

        execute_command(client, None, "GET_SET myset")
        out = capsys.readouterr().out.strip()
        assert out.startswith("{") and out.endswith("}")

    def test_put_set_with_mock_client(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put.return_value = {"myset": True}

        result = execute_command(client, None, "PUT_SET myset a b c")
        assert result is True
        assert capsys.readouterr().out == ""
