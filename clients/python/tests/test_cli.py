import subprocess
import sys
import os
import tempfile

import pytest

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
        r = run_cli("stop")
        assert r.returncode == 0
        assert "0 anna processes were stopped" in r.stdout

    def test_status_with_nothing_running(self):
        r = run_cli("status")
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

    def test_pids_from_name_returns_pids(self):
        from unittest.mock import patch, MagicMock
        from anna.process_mgmt import _pids_from_name

        mock_result = MagicMock()
        mock_result.stdout = "1234\n5678\n"

        with patch("anna.process_mgmt.subprocess.run", return_value=mock_result):
            pids = _pids_from_name("anna-kvs")
        assert pids == [1234, 5678]

    def test_pids_from_name_exception(self):
        from unittest.mock import patch
        from anna.process_mgmt import _pids_from_name

        with patch("anna.process_mgmt.subprocess.run", side_effect=Exception("fail")):
            pids = _pids_from_name("anna-kvs")
        assert pids == []

    def test_find_binary_with_valid_env(self):
        from unittest.mock import patch
        from anna.process_mgmt import _find_binary

        with patch.dict(os.environ, {"ANNA_SERVER_PATH": "/tmp"}), \
             patch("os.path.isfile", return_value=True), \
             patch("os.access", return_value=True):
            result = _find_binary("anna-kvs")
        assert result == "/tmp/anna-kvs"

    def test_start_skips_already_running(self):
        from unittest.mock import patch
        from anna.process_mgmt import start

        with patch("anna.process_mgmt._pids_from_name", return_value=[1234]):
            count = start("/fake/config.yml")
        assert count == 0

    def test_start_popen_success(self):
        from unittest.mock import patch, MagicMock
        from anna.process_mgmt import start

        with patch("anna.process_mgmt._pids_from_name", return_value=[]), \
             patch("anna.process_mgmt._find_binary", return_value="/usr/bin/fake"), \
             patch("anna.process_mgmt.subprocess.Popen") as mock_popen:
            mock_popen.return_value = MagicMock()
            count = start("/fake/config.yml")
        assert count == 3  # 3 processes in PROCESS_LIST

    def test_status_with_running_processes(self):
        from unittest.mock import patch
        from anna.process_mgmt import status

        def mock_pids(name):
            return [1234] if name == "anna-kvs" else []

        with patch("anna.process_mgmt._pids_from_name", side_effect=mock_pids):
            result = status()
        assert result == ["anna-kvs"]

    def test_stop_kills_processes(self):
        from unittest.mock import patch, call
        from anna.process_mgmt import stop

        def mock_pids(name):
            if name == "anna-kvs":
                return [1111, 2222]
            return []

        with patch("anna.process_mgmt._pids_from_name", side_effect=mock_pids), \
             patch("os.kill") as mock_kill:
            count = stop()
        assert count == 2
        import signal
        mock_kill.assert_any_call(1111, signal.SIGTERM)
        mock_kill.assert_any_call(2222, signal.SIGTERM)

    def test_stop_handles_process_lookup_error(self):
        from unittest.mock import patch
        from anna.process_mgmt import stop

        with patch("anna.process_mgmt._pids_from_name", return_value=[9999]), \
             patch("os.kill", side_effect=ProcessLookupError):
            count = stop()
        assert count == 0


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


class TestOrderedSetFormatting:
    def test_get_ordered_set(self):
        from anna.lattices import OrderedSetLattice, ListBasedOrderedSet
        from unittest.mock import MagicMock
        from io import StringIO
        import sys

        oset = ListBasedOrderedSet([b"apple", b"banana", b"cherry"])
        lattice = OrderedSetLattice(oset)

        client = MagicMock()
        client.get_ordered_set.return_value = lattice

        captured = StringIO()
        old_stdout = sys.stdout
        sys.stdout = captured

        from anna.cli import execute_command
        execute_command(client, "/dev/null", "GET_ORDERED_SET mykey")

        sys.stdout = old_stdout
        output = captured.getvalue().strip()

        assert output.startswith("[")
        assert output.endswith("]")
        assert "apple" in output
        assert "banana" in output
        assert "cherry" in output

    def test_get_ordered_set_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get_ordered_set.return_value = None

        execute_command(client, "/dev/null", "GET_ORDERED_SET mykey")
        assert "Key not found" in capsys.readouterr().out

    def test_put_ordered_set(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_ordered_set.return_value = {"mykey": True}

        result = execute_command(client, "/dev/null", "PUT_ORDERED_SET mykey a b c")
        assert result is True
        client.put_ordered_set.assert_called_once()

    def test_put_ordered_set_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_ordered_set.return_value = {"mykey": False}

        execute_command(client, "/dev/null", "PUT_ORDERED_SET mykey a b")
        assert "Failure!" in capsys.readouterr().out


class TestSingleCausalFormatting:
    def test_get_single_causal(self):
        from anna.lattices import SingleKeyCausalLattice, SetLattice, VectorClock
        from unittest.mock import MagicMock
        from io import StringIO
        import sys

        vc = VectorClock({"node1": 2}, True)
        val = SetLattice({b"world"})
        lattice = SingleKeyCausalLattice(vc, val)

        client = MagicMock()
        client.get_single_causal.return_value = lattice

        captured = StringIO()
        old_stdout = sys.stdout
        sys.stdout = captured

        from anna.cli import execute_command
        execute_command(client, "/dev/null", "GET_SINGLE_CAUSAL mykey")

        sys.stdout = old_stdout
        output = captured.getvalue()

        assert "{node1 : 2}" in output
        assert "world" in output

    def test_get_single_causal_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get_single_causal.return_value = None

        execute_command(client, "/dev/null", "GET_SINGLE_CAUSAL mykey")
        assert "Key not found" in capsys.readouterr().out

    def test_put_single_causal(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_single_causal.return_value = {"mykey": True}

        result = execute_command(client, "/dev/null", "PUT_SINGLE_CAUSAL mykey hello")
        assert result is True
        client.put_single_causal.assert_called_once_with("mykey", "hello")

    def test_put_single_causal_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_single_causal.return_value = {"mykey": False}

        execute_command(client, "/dev/null", "PUT_SINGLE_CAUSAL mykey val")
        assert "Failure!" in capsys.readouterr().out


class TestPriorityFormatting:
    def test_get_priority(self):
        from anna.lattices import PriorityLattice
        from unittest.mock import MagicMock
        from io import StringIO
        import sys

        lattice = PriorityLattice(3.5, b"important")

        client = MagicMock()
        client.get_priority.return_value = lattice

        captured = StringIO()
        old_stdout = sys.stdout
        sys.stdout = captured

        from anna.cli import execute_command
        execute_command(client, "/dev/null", "GET_PRIORITY mykey")

        sys.stdout = old_stdout
        output = captured.getvalue()

        assert "priority: 3.5" in output
        assert "important" in output

    def test_get_priority_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get_priority.return_value = None

        execute_command(client, "/dev/null", "GET_PRIORITY mykey")
        assert "Key not found" in capsys.readouterr().out

    def test_put_priority(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_priority.return_value = {"mykey": True}

        result = execute_command(client, "/dev/null", "PUT_PRIORITY mykey 2.5 hello")
        assert result is True
        client.put_priority.assert_called_once_with("mykey", 2.5, "hello")

    def test_put_priority_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_priority.return_value = {"mykey": False}

        execute_command(client, "/dev/null", "PUT_PRIORITY mykey 1.0 val")
        assert "Failure!" in capsys.readouterr().out


class TestCausalFormatting:
    def test_format_causal_output(self):
        """Test the causal output formatting logic from cli.py execute_command."""
        from anna.lattices import (
            MultiKeyCausalLattice, SetLattice, MapLattice, VectorClock,
        )
        from unittest.mock import MagicMock
        from io import StringIO
        import sys

        # Build a causal lattice as if returned by get_causal
        vc = VectorClock({"test": 1}, True)
        dep_vc = VectorClock({"test1": 1}, True)
        deps = MapLattice({"dep1": dep_vc})
        val = SetLattice({b"hello"})
        lattice = MultiKeyCausalLattice(vc, deps, val)

        # Mock client
        client = MagicMock()
        client.get_causal.return_value = lattice

        # Capture stdout
        captured = StringIO()
        old_stdout = sys.stdout
        sys.stdout = captured

        from anna.cli import execute_command
        execute_command(client, "/dev/null", "GET_CAUSAL mykey")

        sys.stdout = old_stdout
        output = captured.getvalue()

        assert "{test : 1}" in output
        assert "dep1 : {test1 : 1}" in output
        assert "hello" in output

    def test_put_causal_command(self):
        """Test PUT_CAUSAL CLI dispatch."""
        from unittest.mock import MagicMock

        client = MagicMock()
        client.put_causal.return_value = {"k": True}

        from anna.cli import execute_command
        result = execute_command(client, "/dev/null", "PUT_CAUSAL k hello")

        assert result is True
        client.put_causal.assert_called_once_with("k", "hello")

    def test_get_causal_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get_causal.return_value = None

        execute_command(client, "/dev/null", "GET_CAUSAL mykey")
        assert "Key not found" in capsys.readouterr().out

    def test_put_causal_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put_causal.return_value = {"k": False}

        execute_command(client, "/dev/null", "PUT_CAUSAL k val")
        assert "Failure!" in capsys.readouterr().out


class TestDeleteCommand:
    def test_delete_success(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.delete.return_value = {"mykey": True}

        result = execute_command(client, None, "DELETE mykey")
        assert result is True
        client.delete.assert_called_once_with("mykey")

    def test_delete_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.delete.return_value = {"mykey": False}

        execute_command(client, None, "DELETE mykey")
        assert "Failure!" in capsys.readouterr().out


class TestGetSetNotFound:
    def test_get_set_key_not_found(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.get.return_value = {"myset": None}

        execute_command(client, None, "GET_SET myset")
        assert "Key not found" in capsys.readouterr().out


class TestPutSetFailure:
    def test_put_set_failure(self, capsys):
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        client = MagicMock()
        client.put.return_value = {"myset": False}

        execute_command(client, None, "PUT_SET myset a b c")
        assert "Failure!" in capsys.readouterr().out


class TestStatusWithRunning:
    def test_status_shows_running_processes(self, capsys):
        from unittest.mock import MagicMock, patch
        from anna.cli import execute_command

        with patch("anna.cli.status", return_value=["anna-kvs"]):
            execute_command(None, None, "STATUS")
        out = capsys.readouterr().out
        assert "anna-kvs process is running" in out


class TestGetNonBytesReveal:
    def test_get_non_bytes_reveal(self, capsys):
        """Test GET when reveal() returns a non-bytes value (e.g., int)."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command

        lattice = MagicMock()
        lattice.reveal.return_value = 42

        client = MagicMock()
        client.get.return_value = {"mykey": lattice}

        execute_command(client, None, "GET mykey")
        assert "42" in capsys.readouterr().out


class TestCliInteractive:
    def test_cli_interactive_exit(self):
        from unittest.mock import MagicMock, patch
        from anna.cli import cli_interactive

        with patch("builtins.input", side_effect=["HELP", "EXIT"]):
            cli_interactive(None, None)

    def test_cli_interactive_eof(self):
        from unittest.mock import MagicMock, patch
        from anna.cli import cli_interactive

        with patch("builtins.input", side_effect=EOFError):
            cli_interactive(None, None)


class TestCliFile:
    def test_cli_file_reads_commands(self, tmp_path):
        from unittest.mock import MagicMock
        from anna.cli import cli_file

        f = tmp_path / "commands.txt"
        f.write_text("HELP\nEXIT\n")

        cli_file(None, None, str(f))

    def test_cli_file_stops_on_exit(self, tmp_path):
        from unittest.mock import MagicMock
        from anna.cli import cli_file

        f = tmp_path / "commands.txt"
        f.write_text("EXIT\nHELP\n")

        cli_file(None, None, str(f))


class TestMainFunction:
    def test_main_help(self):
        from unittest.mock import patch
        from anna.cli import main

        with patch("sys.argv", ["anna-py", "help"]):
            main()

    def test_main_start_requires_config(self):
        from unittest.mock import patch
        from anna.cli import main
        import pytest

        with patch("sys.argv", ["anna-py", "start"]):
            with pytest.raises(SystemExit):
                main()

    def test_main_start_with_config(self, tmp_path):
        from unittest.mock import patch
        from anna.cli import main

        config = tmp_path / "test.yml"
        config.write_text("threads:\n  routing: 1\n")

        with patch("sys.argv", ["anna-py", "--server-config", str(config), "start"]):
            main()

    def test_main_stop(self):
        from unittest.mock import patch
        from anna.cli import main

        with patch("sys.argv", ["anna-py", "stop"]):
            main()

    def test_main_status_nothing_running(self, capsys):
        from unittest.mock import patch
        from anna.cli import main

        with patch("sys.argv", ["anna-py", "status"]):
            main()
        out = capsys.readouterr().out
        assert "not running" in out

    def test_main_status_with_running(self, capsys):
        from unittest.mock import patch
        from anna.cli import main

        with patch("sys.argv", ["anna-py", "status"]), \
             patch("anna.cli.status", return_value=["anna-kvs"]):
            main()
        out = capsys.readouterr().out
        assert "anna-kvs process is running" in out

    def test_main_cli_requires_routing(self):
        from unittest.mock import patch
        from anna.cli import main
        import pytest

        with patch("sys.argv", ["anna-py", "cli"]):
            with pytest.raises(SystemExit):
                main()

    def test_main_cli_requires_client_ip(self):
        from unittest.mock import patch
        from anna.cli import main
        import pytest

        with patch("sys.argv", ["anna-py", "--routing", "127.0.0.1", "cli"]):
            with pytest.raises(SystemExit):
                main()

    def test_main_cli_interactive(self):
        from unittest.mock import patch, MagicMock
        from anna.cli import main

        with patch("sys.argv", ["anna-py", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1", "cli"]), \
             patch("anna.client.AnnaTcpClient") as mock_cls, \
             patch("anna.cli.cli_interactive") as mock_interactive:
            mock_cls.return_value = MagicMock()
            main()
            mock_interactive.assert_called_once()

    def test_main_cli_with_file(self, tmp_path):
        from unittest.mock import patch, MagicMock
        from anna.cli import main

        f = tmp_path / "input.txt"
        f.write_text("HELP\nEXIT\n")

        with patch("sys.argv", ["anna-py", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1", "cli", str(f)]), \
             patch("anna.client.AnnaTcpClient") as mock_cls, \
             patch("anna.cli.cli_file") as mock_file:
            mock_cls.return_value = MagicMock()
            main()
            mock_file.assert_called_once()


class TestBenchValidation:
    def test_bench_rejects_zero_keys(self):
        r = run_cli("--routing", "127.0.0.1", "--client-ip", "127.0.0.1",
                     "--keys", "0", "bench")
        assert r.returncode != 0
        assert "--keys must be > 0" in r.stderr

    def test_bench_rejects_zero_duration(self):
        r = run_cli("--routing", "127.0.0.1", "--client-ip", "127.0.0.1",
                     "--duration", "0", "bench")
        assert r.returncode != 0
        assert "--duration must be > 0" in r.stderr

    def test_bench_rejects_invalid_workload(self):
        r = run_cli("--routing", "127.0.0.1", "--client-ip", "127.0.0.1",
                     "--workload", "INVALID", "bench")
        assert r.returncode != 0
        assert "invalid choice" in r.stderr.lower()

    def test_bench_valid_workload_choices(self):
        import argparse
        for wl in ["GET", "PUT", "MIXED", "ALL", "get", "put", "mixed", "all"]:
            parser = argparse.ArgumentParser()
            parser.add_argument("--workload", choices=["GET", "PUT", "MIXED", "ALL"],
                                type=str.upper)
            args = parser.parse_args(["--workload", wl])
            assert args.workload in ("GET", "PUT", "MIXED", "ALL")

    def test_bench_main_validation_zero_keys(self):
        """Test main() bench validation directly for coverage."""
        from unittest.mock import patch
        from anna.cli import main
        with patch("sys.argv", ["anna", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1",
                                "--keys", "0", "bench"]):
            with pytest.raises(SystemExit):
                main()

    def test_bench_main_validation_zero_duration(self):
        from unittest.mock import patch
        from anna.cli import main
        with patch("sys.argv", ["anna", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1",
                                "--duration", "0", "bench"]):
            with pytest.raises(SystemExit):
                main()

    def test_bench_main_validation_zero_report(self):
        from unittest.mock import patch
        from anna.cli import main
        with patch("sys.argv", ["anna", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1",
                                "--report", "0", "bench"]):
            with pytest.raises(SystemExit):
                main()

    def test_bench_main_validation_negative_value_size(self):
        from unittest.mock import patch
        from anna.cli import main
        with patch("sys.argv", ["anna", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1",
                                "--value-size", "-1", "bench"]):
            with pytest.raises(SystemExit):
                main()

    def test_bench_main_runs_with_mock(self):
        """Test the full main() bench path with a mocked client."""
        from unittest.mock import patch, MagicMock
        from anna.cli import main
        from anna.lattices import LWWPairLattice
        mock_client = MagicMock()
        mock_client.put.return_value = {"k": True}
        mock_client.get.return_value = {"k": LWWPairLattice(1, b"v")}
        with patch("sys.argv", ["anna", "--routing", "127.0.0.1",
                                "--client-ip", "127.0.0.1",
                                "--keys", "5", "--value-size", "16",
                                "--duration", "1", "--workload", "GET",
                                "bench"]), \
             patch("anna.client.AnnaTcpClient", return_value=mock_client):
            main()


class TestUnifiedPutSyntax:
    """Tests for the new 'PUT <type> <key> <values...>' syntax."""

    def test_put_set_unified(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"myset": True}
        execute_command(client, None, "PUT set myset a b c")
        client.put.assert_called_once()

    def test_put_ordered_set_unified(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put_ordered_set.return_value = {"myoset": True}
        execute_command(client, None, "PUT ordered_set myoset x y z")
        client.put_ordered_set.assert_called_once()

    def test_put_priority_unified(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put_priority.return_value = {"mykey": True}
        execute_command(client, None, "PUT priority mykey 1.5 hello")
        client.put_priority.assert_called_once()

    def test_put_causal_unified(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put_causal.return_value = {"mykey": True}
        execute_command(client, None, "PUT causal mykey hello")
        client.put_causal.assert_called_once()

    def test_put_single_causal_unified(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put_single_causal.return_value = {"mykey": True}
        execute_command(client, None, "PUT single_causal mykey hello")
        client.put_single_causal.assert_called_once()

    def test_put_lww_explicit(self):
        """PUT lww key value should work like PUT key value."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"mykey": True}
        execute_command(client, None, "PUT lww mykey hello")
        client.put.assert_called_once()

    def test_put_key_named_set_is_lww(self, capsys):
        """PUT set value (3 tokens) should treat 'set' as the key, not type."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"set": True}
        execute_command(client, None, "PUT set value")
        client.put.assert_called_once()


class TestArityValidation:
    """Tests for argument count validation."""

    def test_get_no_key(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "GET")
        assert result is True
        assert "Usage" in capsys.readouterr().out

    def test_put_no_args(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "PUT")
        assert result is True
        assert "Usage" in capsys.readouterr().out

    def test_put_one_arg(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "PUT key")
        assert result is True
        assert "Usage" in capsys.readouterr().out

    def test_delete_no_key(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "DELETE")
        assert result is True
        assert "Usage" in capsys.readouterr().out

    def test_put_priority_missing_value(self, capsys):
        from anna.cli import execute_command
        result = execute_command(None, None, "PUT priority mykey 1.5")
        assert result is True
        assert "Usage" in capsys.readouterr().out


class TestUnifiedGetFormatting:
    """Tests for unified GET auto-detect formatting."""

    def test_get_list_value_formats_as_ordered_set(self, capsys):
        """GET on an ordered-set-like value (list) should format as [ ... ]."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        lattice = MagicMock()
        lattice.reveal.return_value = [b"alpha", b"beta"]
        client = MagicMock()
        client.get.return_value = {"mykey": lattice}
        execute_command(client, None, "GET mykey")
        out = capsys.readouterr().out
        assert "[ alpha beta ]" in out


class TestLegacyPutAliases:
    """Legacy PUT_* commands should still work via the unified handler."""

    def test_put_set_legacy(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"myset": True}
        execute_command(client, None, "PUT_SET myset a b c")
        client.put.assert_called_once()

    def test_put_priority_legacy(self):
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put_priority.return_value = {"mykey": True}
        execute_command(client, None, "PUT_PRIORITY mykey 1.5 hello")
        client.put_priority.assert_called_once()


class TestUnionScalarType:
    """Tests for the UNION_SCALAR lattice type support."""

    def test_put_union_unified(self):
        """PUT union should call client.put with a UnionScalarLattice."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"mykey": True}
        execute_command(client, None, "PUT union mykey hello")
        client.put.assert_called_once()
        # Check lattice type tag
        args = client.put.call_args
        lattice = args[0][1]
        _, lt = lattice.serialize()
        from anna.kvs_pb2 import UNION_SCALAR
        assert lt == UNION_SCALAR

    def test_union_scalar_deserialize(self):
        """_deserialize should decode UNION_SCALAR as UnionScalarLattice."""
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import SetValue, KeyTuple, UNION_SCALAR
        from anna.lattices import UnionScalarLattice

        sv = SetValue()
        sv.values.append(b"beta")
        sv.values.append(b"alpha")

        tup = KeyTuple()
        tup.lattice_type = UNION_SCALAR
        tup.payload = sv.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, UnionScalarLattice)
        revealed = result.reveal()
        assert b"alpha" in revealed
        assert b"beta" in revealed
        # Should be sorted
        assert revealed == b"alpha\nbeta"

    def test_get_union_scalar_formats_as_text(self, capsys):
        """Unified GET on a UNION_SCALAR key should format as text."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        from anna.lattices import UnionScalarLattice
        client = MagicMock()
        client.get.return_value = {"mykey": UnionScalarLattice({b"line1", b"line2"})}
        execute_command(client, None, "GET mykey")
        out = capsys.readouterr().out
        assert "line1" in out
        assert "line2" in out


class TestLwwSetType:
    """Tests for the LWW_SET lattice type support."""

    def test_put_lww_set_unified(self):
        """PUT lww_set should call client.put with an LwwSetLattice."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        client = MagicMock()
        client.put.return_value = {"mykey": True}
        execute_command(client, None, "PUT lww_set mykey a b c")
        client.put.assert_called_once()
        # Check that the lattice has the correct type tag
        args = client.put.call_args
        lattice = args[0][1]
        _, lt = lattice.serialize()
        from anna.kvs_pb2 import LWW_SET
        assert lt == LWW_SET

    def test_lww_set_deserialize(self):
        """_deserialize should decode LWW_SET responses as SetLattice."""
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import LWWValue, SetValue, KeyTuple, LWW_SET
        from anna.lattices import SetLattice

        # Build a mock tuple with LWW_SET lattice type
        sv = SetValue()
        sv.values.append(b"x")
        sv.values.append(b"y")
        lww = LWWValue()
        lww.timestamp = 12345
        lww.value = sv.SerializeToString()

        tup = KeyTuple()
        tup.lattice_type = LWW_SET
        tup.payload = lww.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, SetLattice)
        revealed = result.reveal()
        assert b"x" in revealed
        assert b"y" in revealed

    def test_get_lww_set_formats_as_set(self, capsys):
        """Unified GET on an LWW_SET key should format as { ... }."""
        from unittest.mock import MagicMock
        from anna.cli import execute_command
        from anna.lattices import SetLattice
        client = MagicMock()
        client.get.return_value = {"mykey": SetLattice({b"p", b"q", b"r"})}
        execute_command(client, None, "GET mykey")
        out = capsys.readouterr().out
        assert "{ " in out
        assert "p" in out
        assert "q" in out
        assert "r" in out
