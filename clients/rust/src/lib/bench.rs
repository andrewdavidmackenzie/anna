//! Benchmark infrastructure for measuring KVS throughput and latency.

use crate::errors::Result;
use std::time::{Duration, Instant};

/// Trait abstracting the KVS operations needed for benchmarking.
/// Implemented by `KVSClient` for production use, and by test mocks.
pub trait BenchClient {
    /// Put a key-value pair.
    fn put_val(
        &mut self,
        key: &str,
        value: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    /// Get a key's value.
    fn get_val(&mut self, key: &str) -> impl std::future::Future<Output = Result<String>> + Send;
}

impl BenchClient for crate::kvs_client::KVSClient {
    async fn put_val(&mut self, key: &str, value: &str) -> Result<()> {
        self.put(key, value).await
    }
    async fn get_val(&mut self, key: &str) -> Result<String> {
        self.get(key).await
    }
}

/// Configuration for a benchmark run.
pub struct BenchConfig {
    /// Number of keys in the key space.
    pub num_keys: u64,
    /// Size of each value in bytes.
    pub value_size: usize,
    /// Total duration of each workload.
    pub duration: Duration,
    /// Seconds between throughput reports.
    pub report_period: Duration,
    /// Workload names to run (GET, PUT, MIXED).
    pub workloads: Vec<String>,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            num_keys: 1000,
            value_size: 256,
            duration: Duration::from_secs(10),
            report_period: Duration::from_secs(2),
            workloads: vec!["GET".into(), "PUT".into(), "MIXED".into()],
        }
    }
}

/// Result of a single workload run.
pub struct WorkloadResult {
    /// Workload name (GET, PUT, or MIXED).
    pub name: String,
    /// Total number of operations completed.
    pub total_ops: u64,
    /// Wall-clock time elapsed.
    pub elapsed: Duration,
}

impl WorkloadResult {
    /// Average operations per second.
    pub fn ops_per_sec(&self) -> f64 {
        self.total_ops as f64 / self.elapsed.as_secs_f64()
    }

    /// Average microseconds per operation.
    pub fn us_per_op(&self) -> f64 {
        if self.total_ops == 0 {
            return 0.0;
        }
        self.elapsed.as_micros() as f64 / self.total_ops as f64
    }
}

/// Format a key index as a zero-padded 8-character string.
pub fn bench_key(index: u64) -> String {
    format!("{:08}", index)
}

/// Simple pseudo-random number generator (LCG).
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Create a new RNG with the given seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns a pseudo-random u64.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }

    /// Returns a value in `[0, bound)`.
    pub fn next_bounded(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// Run the full benchmark: warmup, then each workload, then summary.
pub async fn run_bench(
    client: &mut (impl BenchClient + Send),
    config: &BenchConfig,
) -> Result<Vec<WorkloadResult>> {
    if config.num_keys == 0 {
        return Err(crate::errors::Error::Process(
            "num_keys must be > 0".to_string(),
        ));
    }
    let value: String = "a".repeat(config.value_size);

    println!(
        "Benchmark (Rust): keys={}, value_size={}, duration={}s, report={}s",
        config.num_keys,
        config.value_size,
        config.duration.as_secs(),
        config.report_period.as_secs(),
    );

    // Warmup
    println!("Warming up {} keys...", config.num_keys);
    let warmup_start = Instant::now();
    for i in 0..config.num_keys {
        client.put_val(&bench_key(i), &value).await?;
    }
    println!(
        "Warmup complete: {} keys in {:.2}s",
        config.num_keys,
        warmup_start.elapsed().as_secs_f64()
    );

    let mut results = Vec::new();
    for wl in &config.workloads {
        let result = run_workload(
            client,
            wl,
            config.num_keys,
            &value,
            config.duration,
            config.report_period,
        )
        .await?;
        results.push(result);
    }

    // Summary
    println!();
    println!("=== Benchmark Summary (Rust) ===");
    println!(
        "{:<10} {:>12} {:>12} {:>12} {:>10}",
        "Workload", "ops/sec", "us/op", "total_ops", "elapsed"
    );
    println!("{}", "-".repeat(60));
    for r in &results {
        println!(
            "{:<10} {:>12.1} {:>12.1} {:>12} {:>9.2}s",
            r.name,
            r.ops_per_sec(),
            r.us_per_op(),
            r.total_ops,
            r.elapsed.as_secs_f64()
        );
    }

    Ok(results)
}

async fn run_workload(
    client: &mut (impl BenchClient + Send),
    workload: &str,
    num_keys: u64,
    value: &str,
    duration: Duration,
    report_period: Duration,
) -> Result<WorkloadResult> {
    println!();
    println!("--- {} workload ---", workload);

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let mut rng = SimpleRng::new(seed);

    let start = Instant::now();
    let mut total_ops: u64 = 0;
    let mut epoch_ops: u64 = 0;
    let mut last_report = start;

    while start.elapsed() < duration {
        let key = bench_key(rng.next_bounded(num_keys));

        match workload {
            "GET" => {
                client.get_val(&key).await?;
                total_ops += 1;
                epoch_ops += 1;
            }
            "PUT" => {
                client.put_val(&key, value).await?;
                total_ops += 1;
                epoch_ops += 1;
            }
            "MIXED" => {
                client.put_val(&key, value).await?;
                client.get_val(&key).await?;
                total_ops += 2;
                epoch_ops += 2;
            }
            _ => {
                return Err(crate::errors::Error::Process(format!(
                    "Invalid workload: {}",
                    workload
                )))
            }
        }

        let now = Instant::now();
        if now.duration_since(last_report) >= report_period {
            let secs = now.duration_since(last_report).as_secs_f64();
            println!(
                "  [{:>6.1}s] {:>10.1} ops/sec  ({} ops in {:.2}s)",
                now.duration_since(start).as_secs_f64(),
                epoch_ops as f64 / secs,
                epoch_ops,
                secs,
            );
            epoch_ops = 0;
            last_report = now;
        }
    }

    let elapsed = start.elapsed();
    println!(
        "{} complete: {} ops in {:.2}s ({:.1} ops/sec)",
        workload,
        total_ops,
        elapsed.as_secs_f64(),
        total_ops as f64 / elapsed.as_secs_f64()
    );

    Ok(WorkloadResult {
        name: workload.to_string(),
        total_ops,
        elapsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_key_pads_small_numbers() {
        assert_eq!(bench_key(1), "00000001");
        assert_eq!(bench_key(42), "00000042");
        assert_eq!(bench_key(1000000), "01000000");
    }

    #[test]
    fn bench_key_large_numbers() {
        assert_eq!(bench_key(99999999), "99999999");
        assert_eq!(bench_key(100000000), "100000000");
    }

    #[test]
    fn simple_rng_bounded() {
        let mut rng = SimpleRng::new(12345);
        for _ in 0..100 {
            assert!(rng.next_bounded(1000) < 1000);
        }
    }

    #[test]
    fn simple_rng_different_seeds() {
        let mut rng1 = SimpleRng::new(1);
        let mut rng2 = SimpleRng::new(2);
        let mut differ = false;
        for _ in 0..10 {
            if rng1.next_bounded(1000) != rng2.next_bounded(1000) {
                differ = true;
                break;
            }
        }
        assert!(differ);
    }

    #[test]
    fn workload_result_ops_per_sec() {
        let r = WorkloadResult {
            name: "GET".into(),
            total_ops: 1000,
            elapsed: Duration::from_secs(2),
        };
        assert!((r.ops_per_sec() - 500.0).abs() < 0.1);
        assert!((r.us_per_op() - 2000.0).abs() < 0.1);
    }

    #[test]
    fn workload_result_zero_ops() {
        let r = WorkloadResult {
            name: "PUT".into(),
            total_ops: 0,
            elapsed: Duration::from_secs(1),
        };
        assert_eq!(r.us_per_op(), 0.0);
    }

    #[test]
    fn default_bench_config() {
        let cfg = BenchConfig::default();
        assert_eq!(cfg.num_keys, 1000);
        assert_eq!(cfg.value_size, 256);
        assert_eq!(cfg.duration, Duration::from_secs(10));
        assert_eq!(cfg.workloads, vec!["GET", "PUT", "MIXED"]);
    }

    /// Mock client for testing bench functions without a real server.
    struct MockBenchClient;

    impl BenchClient for MockBenchClient {
        async fn put_val(&mut self, _key: &str, _value: &str) -> Result<()> {
            Ok(())
        }
        async fn get_val(&mut self, _key: &str) -> Result<String> {
            Ok("mock_value".to_string())
        }
    }

    #[tokio::test]
    async fn run_bench_get_workload() {
        let mut client = MockBenchClient;
        let config = BenchConfig {
            num_keys: 5,
            value_size: 16,
            duration: Duration::from_secs(1),
            report_period: Duration::from_secs(1),
            workloads: vec!["GET".into()],
        };
        let results = run_bench(&mut client, &config)
            .await
            .expect("bench should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "GET");
        assert!(results[0].total_ops > 0);
    }

    #[tokio::test]
    async fn run_bench_put_workload() {
        let mut client = MockBenchClient;
        let config = BenchConfig {
            num_keys: 5,
            value_size: 16,
            duration: Duration::from_secs(1),
            report_period: Duration::from_secs(1),
            workloads: vec!["PUT".into()],
        };
        let results = run_bench(&mut client, &config)
            .await
            .expect("bench should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "PUT");
        assert!(results[0].total_ops > 0);
    }

    #[tokio::test]
    async fn run_bench_mixed_workload() {
        let mut client = MockBenchClient;
        let config = BenchConfig {
            num_keys: 5,
            value_size: 16,
            duration: Duration::from_secs(1),
            report_period: Duration::from_secs(1),
            workloads: vec!["MIXED".into()],
        };
        let results = run_bench(&mut client, &config)
            .await
            .expect("bench should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "MIXED");
        assert!(results[0].total_ops > 0);
    }

    #[tokio::test]
    async fn run_bench_all_workloads() {
        let mut client = MockBenchClient;
        let config = BenchConfig {
            num_keys: 5,
            value_size: 16,
            duration: Duration::from_secs(1),
            report_period: Duration::from_secs(1),
            workloads: vec!["GET".into(), "PUT".into(), "MIXED".into()],
        };
        let results = run_bench(&mut client, &config)
            .await
            .expect("bench should succeed");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "GET");
        assert_eq!(results[1].name, "PUT");
        assert_eq!(results[2].name, "MIXED");
    }

    #[tokio::test]
    async fn run_bench_zero_keys_fails() {
        let mut client = MockBenchClient;
        let config = BenchConfig {
            num_keys: 0,
            value_size: 16,
            duration: Duration::from_secs(1),
            report_period: Duration::from_secs(1),
            workloads: vec!["GET".into()],
        };
        assert!(run_bench(&mut client, &config).await.is_err());
    }
}
