import os
import signal
import subprocess
import time

PROCESS_LIST = ["anna-monitor", "anna-kvs"]


def _pids_from_name(name):
    uid = str(os.getuid())
    try:
        result = subprocess.run(
            ["pgrep", "-x", "-u", uid, name],
            capture_output=True, text=True
        )
        return [int(pid) for pid in result.stdout.strip().split("\n") if pid.strip()]
    except Exception:
        return []


def _find_binary(name):
    server_path = os.environ.get("ANNA_SERVER_PATH")
    if server_path:
        full = os.path.join(server_path, name)
        if os.path.isfile(full) and os.access(full, os.X_OK):
            return full
    return name


def start(config_file_path):
    started = 0
    for name in PROCESS_LIST:
        if _pids_from_name(name):
            continue

        binary = _find_binary(name)
        try:
            subprocess.Popen(
                [binary, "--config", config_file_path],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            started += 1
        except FileNotFoundError:
            pass

    return started


def status():
    result = []
    for name in PROCESS_LIST:
        pids = _pids_from_name(name)
        if pids:
            result.append(name)
    return result


def stop():
    killed = 0
    for name in PROCESS_LIST:
        for pid in _pids_from_name(name):
            try:
                os.kill(pid, signal.SIGTERM)
                killed += 1
            except ProcessLookupError:
                pass

    return killed
