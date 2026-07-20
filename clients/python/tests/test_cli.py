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
