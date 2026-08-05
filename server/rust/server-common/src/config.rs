//! YAML configuration parsing.
//!
//! Parses the same `anna-config.yml` format used by the C++ servers.

use serde::Deserialize;
use std::path::Path;

/// Top-level configuration structure.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub user: UserConfig,
    #[serde(default)]
    pub ports: PortsConfig,
    #[serde(default)]
    pub threads: ThreadsConfig,
    #[serde(default)]
    pub capacities: CapacitiesConfig,
    #[serde(default)]
    pub replication: ReplicationConfig,
    #[serde(default)]
    pub timings: TimingsConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub disk: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MonitoringConfig {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub scaling_alert_ip: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct RoutingConfig {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub monitoring: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerConfig {
    #[serde(default)]
    pub public_ip: String,
    #[serde(default)]
    pub private_ip: String,
    #[serde(default)]
    pub seed_ip: String,
    #[serde(default)]
    pub scaling_alert_ip: String,
    #[serde(default)]
    pub routing: Vec<String>,
    #[serde(default)]
    pub monitoring: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UserConfig {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub routing: Vec<String>,
    #[serde(default)]
    pub monitoring: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PortsConfig {
    #[serde(default)]
    pub base_offset: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct ThreadsConfig {
    #[serde(default)]
    pub memory: u32,
    #[serde(default)]
    pub disk: u32,
    #[serde(default)]
    pub routing: u32,
    #[serde(default)]
    pub benchmark: u32,
}

impl ThreadsConfig {
    /// Resolve zero thread counts to auto-detected core count.
    /// A value of 0 means "use all available cores" (matching C++ behavior).
    pub fn resolve_auto(&mut self) {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);
        if self.memory == 0 {
            self.memory = cores;
        }
        if self.disk == 0 {
            self.disk = cores;
        }
        if self.routing == 0 {
            self.routing = cores;
        }
        if self.benchmark == 0 {
            self.benchmark = 1;
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct CapacitiesConfig {
    #[serde(default, rename = "memory-cap")]
    pub memory_cap_gb: u64,
    #[serde(default, rename = "memory-cap-kb")]
    pub memory_cap_kb: Option<u64>,
    #[serde(default, rename = "disk-cap")]
    pub disk_cap_gb: u64,
    #[serde(default, rename = "disk-cap-kb")]
    pub disk_cap_kb: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ReplicationConfig {
    #[serde(default)]
    pub memory: u32,
    #[serde(default)]
    pub disk: u32,
    #[serde(default)]
    pub local: u32,
    #[serde(default)]
    pub minimum: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct TimingsConfig {
    #[serde(default)]
    pub server_report_period: u32,
    #[serde(default)]
    pub key_monitoring_period: u32,
    #[serde(default)]
    pub monitoring_timeout: u32,
    #[serde(default)]
    pub gossip_epoch: u32,
    #[serde(default)]
    pub data_redistribute_batch: u32,
    #[serde(default)]
    pub tombstone_gc_multiplier: u32,
    #[serde(default)]
    pub grace_period: u32,
    #[serde(default)]
    pub monitoring_response_timeout_ms: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub elasticity: bool,
    #[serde(default, rename = "selective-rep")]
    pub selective_rep: bool,
    #[serde(default)]
    pub tiering: bool,
}

impl Config {
    /// Load configuration from a YAML file.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    /// Get the effective memory capacity in bytes.
    pub fn memory_capacity_bytes(&self) -> u64 {
        if let Some(kb) = self.capacities.memory_cap_kb {
            kb * 1024
        } else {
            self.capacities.memory_cap_gb * 1024 * 1024 * 1024
        }
    }

    /// Get the effective disk capacity in bytes.
    pub fn disk_capacity_bytes(&self) -> u64 {
        if let Some(kb) = self.capacities.disk_cap_kb {
            kb * 1024
        } else {
            self.capacities.disk_cap_gb * 1024 * 1024 * 1024
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let yaml = "
monitoring:
  ip: 127.0.0.1
ports:
  base_offset: 100
threads:
  memory: 2
  disk: 1
  routing: 1
";
        let config: Config = serde_yaml::from_str(yaml).expect("parse failed");
        assert_eq!(config.monitoring.ip, "127.0.0.1");
        assert_eq!(config.ports.base_offset, 100);
        assert_eq!(config.threads.memory, 2);
        assert_eq!(config.threads.disk, 1);
    }

    #[test]
    fn parse_full_config() {
        let yaml = "
monitoring:
  scaling_alert_ip: 10.0.0.1
  ip: 10.0.0.1
routing:
  monitoring:
    - 10.0.0.1
  ip: 10.0.0.1
server:
  monitoring:
    - 10.0.0.1
  routing:
    - 10.0.0.1
  seed_ip: 10.0.0.1
  public_ip: 10.0.0.2
  private_ip: 10.0.0.2
  scaling_alert_ip: \"NULL\"
ports:
  base_offset: 0
threads:
  memory: 4
  disk: 2
  routing: 2
  benchmark: 1
capacities:
  memory-cap: 1
  disk-cap: 256
replication:
  memory: 1
  disk: 1
  local: 1
  minimum: 1
timings:
  server_report_period: 15
  monitoring_timeout: 30
  gossip_epoch: 10
policy:
  elasticity: true
  selective-rep: false
  tiering: true
";
        let config: Config = serde_yaml::from_str(yaml).expect("parse failed");
        assert_eq!(config.server.public_ip, "10.0.0.2");
        assert_eq!(config.threads.memory, 4);
        assert!(config.policy.elasticity);
        assert!(config.policy.tiering);
        assert!(!config.policy.selective_rep);
        assert_eq!(config.replication.memory, 1);
        assert_eq!(config.timings.monitoring_timeout, 30);
    }

    #[test]
    fn memory_capacity_kb_overrides_gb() {
        let yaml = "
capacities:
  memory-cap: 1
  memory-cap-kb: 512
";
        let config: Config = serde_yaml::from_str(yaml).expect("parse failed");
        assert_eq!(config.memory_capacity_bytes(), 512 * 1024);
    }

    #[test]
    fn memory_capacity_defaults_to_gb() {
        let yaml = "
capacities:
  memory-cap: 2
";
        let config: Config = serde_yaml::from_str(yaml).expect("parse failed");
        assert_eq!(config.memory_capacity_bytes(), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn defaults_for_missing_fields() {
        let config: Config = serde_yaml::from_str("{}").expect("parse failed");
        assert_eq!(config.ports.base_offset, 0);
        assert_eq!(config.threads.memory, 0);
        assert!(!config.policy.elasticity);
    }

    #[test]
    fn resolve_auto_thread_counts() {
        let mut threads = ThreadsConfig::default();
        assert_eq!(threads.memory, 0);
        assert_eq!(threads.disk, 0);
        assert_eq!(threads.routing, 0);
        assert_eq!(threads.benchmark, 0);

        threads.resolve_auto();

        // After resolve, all should be > 0.
        assert!(threads.memory > 0, "memory should be auto-detected");
        assert!(threads.disk > 0, "disk should be auto-detected");
        assert!(threads.routing > 0, "routing should be auto-detected");
        assert!(threads.benchmark > 0, "benchmark should default to 1");
    }

    #[test]
    fn resolve_auto_preserves_explicit() {
        let mut threads = ThreadsConfig {
            memory: 4,
            disk: 2,
            routing: 0, // auto
            benchmark: 0, // auto
        };

        threads.resolve_auto();

        assert_eq!(threads.memory, 4, "explicit value preserved");
        assert_eq!(threads.disk, 2, "explicit value preserved");
        assert!(threads.routing > 0, "zero resolved to auto");
        assert_eq!(threads.benchmark, 1, "benchmark defaults to 1");
    }
}
