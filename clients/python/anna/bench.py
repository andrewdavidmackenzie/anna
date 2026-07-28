"""Benchmark infrastructure for measuring KVS throughput and latency."""

import time as time_mod
from .lattices import LWWPairLattice


def bench_key(n):
    """Format a key index as a zero-padded 8-character string."""
    return str(n).zfill(8)


def run_bench(client, num_keys=1000, value_size=256, duration=10,
              report_period=2, workloads=None):
    """Run benchmarks against the KVS.

    Args:
        client: An AnnaTcpClient instance.
        num_keys: Number of keys in the key space.
        value_size: Size of values in bytes.
        duration: Duration of each workload in seconds.
        report_period: Seconds between throughput reports.
        workloads: List of workload names (GET, PUT, MIXED).

    Returns:
        List of (workload, avg_throughput, avg_latency_us, total_ops, elapsed) tuples.
    """
    if num_keys <= 0:
        raise ValueError("num_keys must be > 0")
    if duration <= 0:
        raise ValueError("duration must be > 0")
    if report_period <= 0:
        raise ValueError("report_period must be > 0")

    if workloads is None:
        workloads = ["GET", "PUT", "MIXED"]

    value = "a" * value_size
    results = []

    # Validate all workloads upfront.
    for wl in workloads:
        if wl not in ("GET", "PUT", "MIXED"):
            raise ValueError(f"Invalid workload: {wl}. Must be GET, PUT, or MIXED.")

    # Warmup once, shared across all workloads.
    print(f"Warming up {num_keys} keys ({value_size} bytes each)...")
    warmup_start = time_mod.monotonic()
    for i in range(1, num_keys + 1):
        ts = time_mod.time_ns()
        client.put(bench_key(i), LWWPairLattice(ts, value.encode()))
    warmup_ms = (time_mod.monotonic() - warmup_start) * 1000
    print(f"Warmup complete in {warmup_ms:.0f} ms")

    for wl in workloads:
        print(f"Running {wl} benchmark for {duration}s "
              f"({num_keys} keys, {value_size} B values)...")

        total_ops = 0
        epoch_ops = 0
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
                secs = now - epoch_start
                throughput = epoch_ops / secs
                print(f"[Epoch] Throughput: {int(throughput)} ops/sec")
                epoch_ops = 0
                epoch_start = now

            if now - bench_start >= duration:
                break

        elapsed = time_mod.monotonic() - bench_start
        avg_tp = total_ops / elapsed if elapsed > 0 else 0
        avg_lat = 1_000_000.0 / avg_tp if avg_tp > 0 else 0

        print(f"\n=== {wl} Results ===")
        print(f"Total ops:      {total_ops}")
        print(f"Elapsed:        {elapsed:.2f} s")
        print(f"Avg throughput: {int(avg_tp)} ops/sec")
        print(f"Avg latency:    {avg_lat:.1f} us/op")
        print()
        results.append((wl, avg_tp, avg_lat, total_ops, elapsed))

    print("\n=== Benchmark Summary (Python) ===")
    print(f"{'Workload':<10} {'Ops/sec':>12} {'Latency(us)':>14} "
          f"{'Total ops':>12} {'Time(s)':>10}")
    print("-" * 58)
    for wl, tp, lat, ops, secs in results:
        print(f"{wl:<10} {int(tp):>12} {lat:>14.1f} {ops:>12} {secs:>10.2f}")

    return results
