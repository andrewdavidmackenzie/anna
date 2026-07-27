import argparse
import sys

from .process_mgmt import start, stop, status, PROCESS_LIST


def cli_usage():
    return ("Valid commands are GET, GET_SET, GET_ORDERED_SET, GET_CAUSAL, "
            "GET_SINGLE_CAUSAL, GET_PRIORITY, PUT, PUT_SET, PUT_ORDERED_SET, "
            "PUT_CAUSAL, PUT_SINGLE_CAUSAL, PUT_PRIORITY, DELETE, "
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
    elif cmd == "DELETE":
        result = client.delete(parts[1])
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


def run_bench(client, args):
    import time as time_mod
    from .lattices import LWWPairLattice

    num_keys = args.keys
    value_size = args.value_size
    duration = args.duration
    report_period = args.report
    workload_arg = (args.workload or "ALL").upper()

    workloads = ["GET", "PUT", "MIXED"] if workload_arg == "ALL" else [workload_arg]
    value = "a" * value_size

    def bench_key(n):
        return str(n).zfill(8)

    results = []

    for wl in workloads:
        # Warmup
        print(f"Warming up {num_keys} keys ({value_size} bytes each)...")
        warmup_start = time_mod.monotonic()
        for i in range(1, num_keys + 1):
            ts = time_mod.time_ns()
            client.put(bench_key(i), LWWPairLattice(ts, value.encode()))
        warmup_ms = (time_mod.monotonic() - warmup_start) * 1000
        print(f"Warmup complete in {warmup_ms:.0f} ms")

        print(f"Running {wl} benchmark for {duration}s "
              f"({num_keys} keys, {value_size} B values)...")

        total_ops = 0
        epoch_ops = 0
        throughput_sum = 0.0
        epochs = 0
        seed = int(time_mod.monotonic() * 1e9) & 0xFFFFFFFF

        bench_start = time_mod.monotonic()
        epoch_start = bench_start

        while True:
            seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF
            k = (seed % num_keys) + 1
            key = bench_key(k)

            if wl == "GET":
                client.get(key)
                total_ops += 1
                epoch_ops += 1
            elif wl == "PUT":
                ts = time_mod.time_ns()
                client.put(key, LWWPairLattice(ts, value.encode()))
                total_ops += 1
                epoch_ops += 1
            else:  # MIXED
                ts = time_mod.time_ns()
                client.put(key, LWWPairLattice(ts, value.encode()))
                client.get(key)
                total_ops += 2
                epoch_ops += 2

            now = time_mod.monotonic()
            if now - epoch_start >= report_period:
                epochs += 1
                secs = now - epoch_start
                throughput = epoch_ops / secs
                throughput_sum += throughput
                print(f"[Epoch {epochs}] Throughput: {int(throughput)} ops/sec")
                epoch_ops = 0
                epoch_start = now

            if now - bench_start >= duration:
                break

        elapsed = time_mod.monotonic() - bench_start
        avg_tp = throughput_sum / epochs if epochs > 0 else total_ops / elapsed
        avg_lat = 1_000_000.0 / avg_tp if avg_tp > 0 else 0

        print(f"\n=== {wl} Results ===")
        print(f"Total ops:      {total_ops}")
        print(f"Elapsed:        {elapsed:.2f} s")
        print(f"Avg throughput: {int(avg_tp)} ops/sec")
        print(f"Avg latency:    {avg_lat:.1f} us/op")
        print()
        results.append((wl, avg_tp, avg_lat, total_ops, elapsed))

    print("\n=== Benchmark Summary (Python) ===")
    print(f"{'Workload':<10} {'Ops/sec':>12} {'Latency(us)':>14} {'Total ops':>12} {'Time(s)':>10}")
    print("-" * 58)
    for wl, tp, lat, ops, secs in results:
        print(f"{wl:<10} {int(tp):>12} {lat:>14.1f} {ops:>12} {secs:>10.2f}")


def main():
    parser = argparse.ArgumentParser(prog="anna-py", description="Anna KVS Python client")
    parser.add_argument("--routing", help="Routing node IP address")
    parser.add_argument("--client-ip", help="Client IP address")
    parser.add_argument("--server-config", help="Path to server config file (for start command)")
    parser.add_argument("--keys", type=int, default=1000, help="Bench: key space size")
    parser.add_argument("--value-size", type=int, default=256, help="Bench: value size in bytes")
    parser.add_argument("--duration", type=int, default=10, help="Bench: duration in seconds")
    parser.add_argument("--report", type=int, default=2, help="Bench: report period in seconds")
    parser.add_argument("--workload", help="Bench: GET, PUT, MIXED, or ALL")
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
        from .client import AnnaTcpClient
        client = AnnaTcpClient(args.routing, args.client_ip, local=True)
        run_bench(client, args)
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
