#![allow(dead_code)] // TODO remove eventually
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use serde_derive::Deserialize;

use super::errors::{Error, Result};
use crate::types::Address;

/// `Config` structure containing the configuration read from the yaml config file
#[derive(Deserialize)]
pub struct Config {
    monitoring: Monitoring,
    routing: Routing,
    user: User,
    #[serde(rename = "routing-elb")]
    routing_elb: Option<Vec<Address>>, // Need an example, this maybe a single IP not an array
    server: Server,
    policy: Policy,
    ebs: Ebs,
    capacities: Capacities,
    threads: Threads,
    replication: Replication,
}

/// Monitoring configuration section
#[derive(Deserialize)]
struct Monitoring {
    mgmt_ip: Address,
    ip: Address,
}

/// Routing configuration section
#[derive(Deserialize)]
struct Routing {
    monitoring: Vec<Address>,
    ip: Address,
}

/// User configuration section
#[derive(Deserialize)]
struct User {
    monitoring: Vec<Address>,
    routing: Vec<Address>,
    ip: Address,
}

/// Server configuration section
#[derive(Deserialize)]
struct Server {
    monitoring: Vec<Address>,
    routing: Vec<Address>,
    seed_ip: Address,
    public_ip: Address,
    private_ip: Address,
    mgmt_ip: Address,
}

/// Policy configuration section
#[derive(Deserialize)]
struct Policy {
    elasticity: bool,
    #[serde(rename = "selective-rep")]
    selective_rep: bool,
    tiering: bool,
}

/// EBS configuration section
#[derive(Deserialize)]
/// EBS configuration consists of a File Path String
struct Ebs(String);

/// Capacities configuration section
#[derive(Deserialize)]
struct Capacities {
    #[serde(rename = "memory-cap")]
    memory_cap: usize,
    #[serde(rename = "ebs-cap")]
    ebs_cap: usize,
}

/// Threads configuration section
#[derive(Deserialize)]
struct Threads {
    memory: usize,
    ebs: usize,
    routing: usize,
    benchmark: usize,
}

/// Replication configuration section
#[derive(Deserialize)]
struct Replication {
    memory: usize,
    ebs: usize,
    minimum: usize,
    local: usize,
}

impl Default for Config {
    fn default() -> Self {
        let localhost = "127.0.0.1".to_string();
        Config {
            monitoring: Monitoring {
                mgmt_ip: localhost.clone(),
                ip: localhost.clone(),
            },
            routing: Routing {
                monitoring: vec![localhost.clone()],
                ip: localhost.clone(),
            },
            user: User {
                monitoring: vec![localhost.clone()],
                routing: vec![localhost.clone()],
                ip: localhost.clone(),
            },
            routing_elb: None,
            server: Server {
                monitoring: vec![localhost.clone()],
                routing: vec![localhost.clone()],
                seed_ip: localhost.clone(),
                public_ip: localhost.clone(),
                private_ip: localhost.clone(),
                mgmt_ip: localhost.clone(),
            },
            policy: Policy {
                elasticity: false,
                selective_rep: false,
                tiering: false,
            },
            ebs: Ebs(String::new()),
            capacities: Capacities {
                memory_cap: 256,
                ebs_cap: 256,
            },
            threads: Threads {
                memory: 1,
                ebs: 1,
                routing: 1,
                benchmark: 1,
            },
            replication: Replication {
                memory: 1,
                ebs: 1,
                minimum: 1,
                local: 1,
            },
        }
    }
}

/// Anna configuration, deserialized from a YAML config file.
impl Config {
    /// Read configuration from a YAML file.
    ///
    /// ```rust
    /// let config = annalib::config::Config::default();
    /// assert_eq!(config.get_user_ip(), "127.0.0.1");
    /// assert_eq!(config.get_routing_thread_count(), 1);
    /// ```
    pub fn read(config_file_path: &PathBuf) -> Result<Config> {
        let path_str = config_file_path.display().to_string();
        let mut file = File::open(config_file_path).map_err(|e| Error::ConfigFile {
            path: path_str.clone(),
            detail: format!("Could not open: {}", e),
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| Error::ConfigFile {
            path: path_str.clone(),
            detail: format!("Could not read: {}", e),
        })?;
        serde_yaml::from_str(&content).map_err(|e| Error::ConfigFile {
            path: path_str,
            detail: format!("YAML parse error: {}", e),
        })
    }

    /// Return a vector of `Address` used for routing
    pub fn get_routing_ips(&self) -> &Vec<Address> {
        match &self.routing_elb {
            Some(elb_ip) => elb_ip,
            None => &self.user.routing,
        }
    }

    /// Return the `Address` for this `User`
    pub fn get_user_ip(&self) -> &Address {
        &self.user.ip
    }

    /// Return the number of threads used for routing
    pub fn get_routing_thread_count(&self) -> usize {
        self.threads.routing
    }
}

#[cfg(test)]
mod test {
    use super::Config;
    use std::path::PathBuf;

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.get_user_ip(), "127.0.0.1");
        assert_eq!(config.get_routing_thread_count(), 1);
        assert_eq!(config.get_routing_ips(), &vec!["127.0.0.1".to_string()]);
    }

    #[test]
    fn routing_ips_no_elb() {
        let config = Config::read(&PathBuf::from("src/lib/test_config.yml"))
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_routing_ips(), &vec!("127.0.0.1".to_string()));
    }

    #[test]
    fn user_ip() {
        let config = Config::read(&PathBuf::from("src/lib/test_config.yml"))
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_user_ip(), "127.0.0.1");
    }

    #[test]
    fn routing_thread_count() {
        let config = Config::read(&PathBuf::from("src/lib/test_config.yml"))
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_routing_thread_count(), 1);
    }

    #[test]
    fn config_file_not_found() {
        let result = Config::read(&PathBuf::from("nonexistent_file.yml"));
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("nonexistent_file.yml"), "err was: {}", e),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn config_from_default_file() {
        let config = Config::read(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("Could not read default-config.yml");
        assert_eq!(config.get_user_ip(), "127.0.0.1");
        assert!(!config.get_routing_ips().is_empty());
    }
}
