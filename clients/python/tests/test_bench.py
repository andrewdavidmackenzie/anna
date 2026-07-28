"""Unit tests for the anna.bench module."""

import pytest
from anna.bench import bench_key, run_bench
from anna.lattices import LWWPairLattice


class MockBenchClient:
    """Mock client that accepts put/get calls without a server."""

    def put(self, key, value):
        return {key: True}

    def get(self, key):
        return {key: LWWPairLattice(1, b"bench_value")}


class TestBenchKey:
    def test_pads_small_numbers(self):
        assert bench_key(1) == "00000001"
        assert bench_key(42) == "00000042"

    def test_large_numbers(self):
        assert bench_key(99999999) == "99999999"
        assert bench_key(100000000) == "100000000"


class TestRunBenchValidation:
    def test_zero_keys_raises(self):
        with pytest.raises(ValueError, match="num_keys must be > 0"):
            run_bench(None, num_keys=0)

    def test_zero_duration_raises(self):
        with pytest.raises(ValueError, match="duration must be > 0"):
            run_bench(None, duration=0)

    def test_zero_report_period_raises(self):
        with pytest.raises(ValueError, match="report_period must be > 0"):
            run_bench(None, report_period=0)

    def test_invalid_workload_raises(self):
        with pytest.raises(ValueError, match="Invalid workload"):
            run_bench(None, workloads=["INVALID"])


class TestRunBenchWithMock:
    def test_get_workload(self):
        client = MockBenchClient()
        results = run_bench(client, num_keys=5, value_size=16,
                            duration=1, report_period=1, workloads=["GET"])
        assert len(results) == 1
        assert results[0][0] == "GET"
        assert results[0][3] > 0  # total_ops > 0

    def test_put_workload(self):
        client = MockBenchClient()
        results = run_bench(client, num_keys=5, value_size=16,
                            duration=1, report_period=1, workloads=["PUT"])
        assert len(results) == 1
        assert results[0][0] == "PUT"
        assert results[0][3] > 0

    def test_mixed_workload(self):
        client = MockBenchClient()
        results = run_bench(client, num_keys=5, value_size=16,
                            duration=1, report_period=1, workloads=["MIXED"])
        assert len(results) == 1
        assert results[0][0] == "MIXED"
        assert results[0][3] > 0

    def test_all_workloads(self):
        client = MockBenchClient()
        results = run_bench(client, num_keys=5, value_size=16,
                            duration=1, report_period=1)
        assert len(results) == 3
        assert [r[0] for r in results] == ["GET", "PUT", "MIXED"]
