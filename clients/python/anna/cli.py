import argparse
import sys

import yaml

from .process_mgmt import start, stop, status, PROCESS_LIST


def load_config(config_path):
    with open(config_path) as f:
        conf = yaml.safe_load(f)

    routing_thread_count = conf["threads"]["routing"]

    user = conf["user"]
    ip = user["ip"]

    if "routing-elb" in conf:
        elb_addr = conf["routing-elb"]
    elif "routing-elb" in user:
        elb_addr = user["routing-elb"]
    else:
        elb_addr = user["routing"][0]

    return elb_addr, ip, routing_thread_count


def cli_usage():
    return ("Valid commands are GET, GET_SET, GET_ORDERED_SET, GET_CAUSAL, "
            "GET_SINGLE_CAUSAL, GET_PRIORITY, PUT, PUT_SET, PUT_ORDERED_SET, "
            "PUT_CAUSAL, PUT_SINGLE_CAUSAL, PUT_PRIORITY, "
            "START, STOP, STATUS, HELP and EXIT")


def execute_command(client, config_path, line):
    from .lattices import (LWWPairLattice, SetLattice, OrderedSetLattice,
                            ListBasedOrderedSet, SingleKeyCausalLattice,
                            PriorityLattice, VectorClock)

    parts = line.strip().split()
    if not parts:
        return True

    cmd = parts[0].upper()

    if cmd == "GET":
        result = client.get(parts[1])
        val = result.get(parts[1])
        if val is not None:
            revealed = val.reveal()
            if isinstance(revealed, bytes):
                print(revealed.decode("utf-8", errors="replace"))
            else:
                print(revealed)
        else:
            print("Key not found")
    elif cmd == "PUT":
        import time
        ts = time.time_ns()
        val = LWWPairLattice(ts, parts[2].encode("utf-8"))
        result = client.put(parts[1], val)
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "GET_SET":
        result = client.get(parts[1])
        val = result.get(parts[1])
        if val is not None:
            items = sorted(v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                           for v in val.reveal())
            print("{ " + " ".join(items) + " }")
        else:
            print("Key not found")
    elif cmd == "PUT_SET":
        values = set(v.encode("utf-8") for v in parts[2:])
        val = SetLattice(values)
        result = client.put(parts[1], val)
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "GET_CAUSAL":
        val = client.get_causal(parts[1])
        if val is not None:
            for k, v in sorted(val.vector_clock.reveal().items()):
                print("{" + f"{k} : {v.reveal()}" + "}")
            for dep_key, vc in sorted(val.dependencies.reveal().items()):
                vc_parts = " ".join(
                    "{" + f"{k} : {v.reveal()}" + "}"
                    for k, v in sorted(vc.reveal().items())
                )
                print(f"{dep_key} : {vc_parts}")
            values = val.value.reveal()
            for v in values:
                print(v.decode("utf-8") if isinstance(v, bytes) else str(v))
        else:
            print("Key not found")
    elif cmd == "PUT_CAUSAL":
        result = client.put_causal(parts[1], parts[2])
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "GET_ORDERED_SET":
        val = client.get_ordered_set(parts[1])
        if val is not None:
            items = [v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                     for v in val.reveal()]
            print("[ " + " ".join(items) + " ]")
        else:
            print("Key not found")
    elif cmd == "PUT_ORDERED_SET":
        values = [v.encode("utf-8") for v in parts[2:]]
        result = client.put_ordered_set(parts[1], values)
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "GET_SINGLE_CAUSAL":
        val = client.get_single_causal(parts[1])
        if val is not None:
            for k, v in sorted(val.vector_clock.reveal().items()):
                print("{" + f"{k} : {v.reveal()}" + "}")
            values = val.value.reveal()
            for v in values:
                print(v.decode("utf-8") if isinstance(v, bytes) else str(v))
        else:
            print("Key not found")
    elif cmd == "PUT_SINGLE_CAUSAL":
        result = client.put_single_causal(parts[1], parts[2])
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "GET_PRIORITY":
        val = client.get_priority(parts[1])
        if val is not None:
            print(f"priority: {val.priority}")
            value = val.value
            print(value.decode("utf-8") if isinstance(value, bytes) else str(value))
        else:
            print("Key not found")
    elif cmd == "PUT_PRIORITY":
        priority = float(parts[2])
        result = client.put_priority(parts[1], priority, parts[3])
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd == "START":
        count = start(config_path)
        print(f"{count} anna processes were started")
    elif cmd == "STOP":
        count = stop()
        print(f"{count} anna processes were stopped")
    elif cmd == "STATUS":
        running = status()
        for name in running:
            print(f"{name} process is running")
    elif cmd == "HELP":
        print(cli_usage())
    elif cmd == "EXIT":
        return False
    else:
        print(f"Unrecognized command: {cmd}")
        print(cli_usage())

    return True


def cli_interactive(client, config_path):
    while True:
        try:
            line = input("anna> ")
        except EOFError:
            break
        if not execute_command(client, config_path, line):
            break


def cli_file(client, config_path, filename):
    with open(filename) as f:
        for line in f:
            if not execute_command(client, config_path, line):
                break


def main():
    parser = argparse.ArgumentParser(prog="anna-py", description="Anna KVS Python client")
    parser.add_argument("--config", "-c", required=True, help="Path to config file")
    parser.add_argument("command", choices=["start", "stop", "status", "cli", "help"],
                        help="Command to run")
    parser.add_argument("input_file", nargs="?", help="Input file for cli command")

    args = parser.parse_args()

    if args.command == "help":
        parser.print_help()
        return

    if args.command == "start":
        count = start(args.config)
        print(f"{count} anna processes were started")
        return

    if args.command == "stop":
        count = stop()
        print(f"{count} anna processes were stopped")
        return

    if args.command == "status":
        running = status()
        if not running:
            for name in PROCESS_LIST:
                print(f"Process '{name}' is not running")
        else:
            for name in PROCESS_LIST:
                if name in running:
                    print(f"{name} process is running")
                else:
                    print(f"Process '{name}' is not running")
        return

    if args.command == "cli":
        from .client import AnnaTcpClient
        elb_addr, ip, _ = load_config(args.config)
        client = AnnaTcpClient(elb_addr, ip, local=True)

        if args.input_file:
            cli_file(client, args.config, args.input_file)
        else:
            cli_interactive(client, args.config)


if __name__ == "__main__":
    main()
