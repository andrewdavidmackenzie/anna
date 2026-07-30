import argparse
import sys

from .process_mgmt import start, stop, status, PROCESS_LIST


_TYPE_NAMES = {"lww", "set", "ordered_set", "lww_set", "priority", "causal", "single_causal"}


def cli_usage():
    return ("Valid commands are:\n"
            "  GET {key}                       - get the value of any key (auto-detects type)\n"
            "  PUT {key} {value}               - store a value (LWW, default)\n"
            "  PUT set {key} {vals...}         - store a set (union merge)\n"
            "  PUT ordered_set {key} {vals...} - store an ordered set\n"
            "  PUT lww_set {key} {vals...}     - store a set (LWW, replaces on write)\n"
            "  PUT priority {key} {pri} {val}  - store with priority (lowest wins)\n"
            "  PUT causal {key} {value}        - store with multi-key causal consistency\n"
            "  PUT single_causal {key} {value} - store with single-key causal consistency\n"
            "  DELETE {key}                    - delete a key\n"
            "  BENCH [keys] [value_size] [duration] [workload] - run a benchmark\n"
            "  START, STOP, STATUS, HELP, EXIT")


def execute_command(client, config_path, line):
    from .lattices import (LWWPairLattice, SetLattice, OrderedSetLattice,
                            ListBasedOrderedSet, SingleKeyCausalLattice,
                            PriorityLattice, VectorClock)

    parts = line.strip().split()
    if not parts:
        return True

    cmd = parts[0].upper()

    if cmd in ("GET", "GET_SET", "GET_ORDERED_SET", "GET_CAUSAL",
               "GET_SINGLE_CAUSAL", "GET_PRIORITY"):
        if len(parts) < 2:
            print("Usage: GET <key>")
            return True
        key = parts[1]
        # For legacy GET_* commands, use the type-specific method.
        if cmd == "GET_CAUSAL":
            val = client.get_causal(key)
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
        elif cmd == "GET_SINGLE_CAUSAL":
            val = client.get_single_causal(key)
            if val is not None:
                for k, v in sorted(val.vector_clock.reveal().items()):
                    print("{" + f"{k} : {v.reveal()}" + "}")
                values = val.value.reveal()
                for v in values:
                    print(v.decode("utf-8") if isinstance(v, bytes) else str(v))
            else:
                print("Key not found")
        elif cmd == "GET_PRIORITY":
            val = client.get_priority(key)
            if val is not None:
                print(f"priority: {val.priority}")
                value = val.value
                print(value.decode("utf-8") if isinstance(value, bytes) else str(value))
            else:
                print("Key not found")
        elif cmd == "GET_ORDERED_SET":
            val = client.get_ordered_set(key)
            if val is not None:
                items = [v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                         for v in val.reveal()]
                print("[ " + " ".join(items) + " ]")
            else:
                print("Key not found")
        elif cmd == "GET_SET":
            result = client.get(key)
            val = result.get(key)
            if val is not None:
                items = sorted(v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                               for v in val.reveal())
                print("{ " + " ".join(items) + " }")
            else:
                print("Key not found")
        else:
            # Unified GET: uses client.get() which returns LWW or Set
            # lattice objects. For other types (causal, priority, etc.),
            # use the legacy GET_CAUSAL, GET_PRIORITY commands.
            result = client.get(key)
            val = result.get(key)
            if val is not None:
                revealed = val.reveal()
                if isinstance(revealed, bytes):
                    print(revealed.decode("utf-8", errors="replace"))
                elif isinstance(revealed, (set, frozenset)):
                    items = sorted(v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                                   for v in revealed)
                    print("{ " + " ".join(items) + " }")
                elif isinstance(revealed, list):
                    items = [v.decode("utf-8", errors="replace") if isinstance(v, bytes) else str(v)
                             for v in revealed]
                    print("[ " + " ".join(items) + " ]")
                else:
                    print(revealed)
            else:
                print("Key not found")
    elif cmd == "PUT":
        if len(parts) < 3:
            print("Usage: PUT [type] <key> <value(s)>")
            return True
        # Exactly 3 tokens: always LWW (preserves keys named "set" etc.)
        # 4+ tokens with a type name: typed PUT.
        type_name = parts[1].lower() if len(parts) > 1 else ""
        if len(parts) >= 4 and type_name in _TYPE_NAMES:
            key_idx = 2
            if type_name == "lww":
                import time
                ts = time.time_ns()
                val = LWWPairLattice(ts, parts[3].encode("utf-8"))
                result = client.put(parts[key_idx], val)
                if not result.get(parts[key_idx], False):
                    print("Failure!")
            elif type_name == "set":
                values = set(v.encode("utf-8") for v in parts[3:])
                val = SetLattice(values)
                result = client.put(parts[key_idx], val)
                if not result.get(parts[key_idx], False):
                    print("Failure!")
            elif type_name == "ordered_set":
                values = [v.encode("utf-8") for v in parts[3:]]
                result = client.put_ordered_set(parts[key_idx], values)
                if not result.get(parts[key_idx], False):
                    print("Failure!")
            elif type_name == "lww_set":
                # LWW_SET: wrap a SetValue inside an LWWValue, then send
                # with lattice_type = LWW_SET. We can't use client.put()
                # because LWWPairLattice.serialize() returns LWW, not LWW_SET.
                from .kvs_pb2 import SetValue as SetValuePb, LWW_SET as LWW_SET_TYPE
                from .kvs_pb2 import LWWValue as LWWValuePb
                sv = SetValuePb()
                for v in parts[3:]:
                    sv.values.append(v.encode("utf-8"))
                import time
                ts = time.time_ns()
                lww = LWWValuePb()
                lww.timestamp = ts
                lww.value = sv.SerializeToString()
                # Use put_all with a custom lattice that returns LWW_SET.
                # Create a thin wrapper that serializes correctly.
                class _LwwSetLattice(LWWPairLattice):
                    def serialize(self):
                        res = LWWValuePb()
                        res.timestamp = self.ts
                        res.value = self.val
                        return res, LWW_SET_TYPE
                val = _LwwSetLattice(ts, sv.SerializeToString())
                result = client.put(parts[key_idx], val)
                if not result.get(parts[key_idx], False):
                    print("Failure!")
            elif type_name == "priority":
                if len(parts) < 5:
                    print("Usage: PUT priority {key} {priority} {value}")
                else:
                    priority = float(parts[3])
                    result = client.put_priority(parts[key_idx], priority, parts[4])
                    if not result.get(parts[key_idx], False):
                        print("Failure!")
            elif type_name == "causal":
                result = client.put_causal(parts[key_idx], parts[3])
                if not result.get(parts[key_idx], False):
                    print("Failure!")
            elif type_name == "single_causal":
                result = client.put_single_causal(parts[key_idx], parts[3])
                if not result.get(parts[key_idx], False):
                    print("Failure!")
        else:
            # Default: LWW
            import time
            ts = time.time_ns()
            val = LWWPairLattice(ts, parts[2].encode("utf-8"))
            result = client.put(parts[1], val)
            if not result.get(parts[1], False):
                print("Failure!")
    elif cmd == "DELETE":
        if len(parts) < 2:
            print("Usage: DELETE <key>")
            return True
        result = client.delete(parts[1])
        if not result.get(parts[1], False):
            print("Failure!")
    elif cmd in ("PUT_SET", "PUT_ORDERED_SET", "PUT_CAUSAL",
                 "PUT_SINGLE_CAUSAL", "PUT_PRIORITY"):
        # Legacy PUT_* aliases: remap to the unified PUT handler.
        type_map = {
            "PUT_SET": "set",
            "PUT_ORDERED_SET": "ordered_set",
            "PUT_CAUSAL": "causal",
            "PUT_SINGLE_CAUSAL": "single_causal",
            "PUT_PRIORITY": "priority",
        }
        remapped = ["PUT", type_map[cmd]] + parts[1:]
        return execute_command(client, config_path,
                               " ".join(remapped))
    elif cmd == "BENCH":
        from .bench import run_bench
        try:
            num_keys = int(parts[1]) if len(parts) > 1 else 1000
            value_size = int(parts[2]) if len(parts) > 2 else 256
            duration = int(parts[3]) if len(parts) > 3 else 10
            wl_arg = parts[4].upper() if len(parts) > 4 else "ALL"
            workloads = ["GET", "PUT", "MIXED"] if wl_arg == "ALL" else [wl_arg]
            run_bench(client, num_keys=num_keys, value_size=value_size,
                      duration=duration, workloads=workloads)
        except (ValueError, TypeError) as e:
            print(f"Error: {e}")
            print("Usage: BENCH [keys] [value_size] [duration] [workload]")
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
    parser.add_argument("--routing", help="Routing node IP address")
    parser.add_argument("--client-ip", help="Client IP address")
    parser.add_argument("--server-config", help="Path to server config file (for start command)")
    parser.add_argument("--keys", type=int, default=1000, help="Bench: key space size")
    parser.add_argument("--value-size", type=int, default=256, help="Bench: value size in bytes")
    parser.add_argument("--duration", type=int, default=10, help="Bench: duration in seconds")
    parser.add_argument("--report", type=int, default=2, help="Bench: report period in seconds")
    parser.add_argument("--workload", choices=["GET", "PUT", "MIXED", "ALL"],
                        type=str.upper, default="ALL",
                        help="Bench: GET, PUT, MIXED, or ALL")
    parser.add_argument("command", choices=["start", "stop", "status", "cli", "bench", "help"],
                        help="Command to run")
    parser.add_argument("input_file", nargs="?", help="Input file for cli command")

    args = parser.parse_args()

    if args.command == "help":
        parser.print_help()
        return

    if args.command == "start":
        if not args.server_config:
            parser.error("--server-config is required for the start command")
        count = start(args.server_config)
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

    if args.command == "bench":
        if not args.routing:
            parser.error("--routing is required for the bench command")
        if not args.client_ip:
            parser.error("--client-ip is required for the bench command")
        if args.keys <= 0:
            parser.error("--keys must be > 0")
        if args.value_size < 0:
            parser.error("--value-size must be >= 0")
        if args.duration <= 0:
            parser.error("--duration must be > 0")
        if args.report <= 0:
            parser.error("--report must be > 0")
        from .client import AnnaTcpClient
        from .bench import run_bench
        client = AnnaTcpClient(args.routing, args.client_ip, local=True)
        wl_arg = (args.workload or "ALL").upper()
        workloads = ["GET", "PUT", "MIXED"] if wl_arg == "ALL" else [wl_arg]
        run_bench(client, num_keys=args.keys, value_size=args.value_size,
                  duration=args.duration, report_period=args.report,
                  workloads=workloads)
        return

    if args.command == "cli":
        if not args.routing:
            parser.error("--routing is required for the cli command")
        if not args.client_ip:
            parser.error("--client-ip is required for the cli command")
        from .client import AnnaTcpClient
        client = AnnaTcpClient(args.routing, args.client_ip, local=True)

        if args.input_file:
            cli_file(client, args.server_config, args.input_file)
        else:
            cli_interactive(client, args.server_config)


if __name__ == "__main__":
    main()
