"""Unit tests for the anna.bench module."""

import pytest
from anna.bench import bench_key, run_bench


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
