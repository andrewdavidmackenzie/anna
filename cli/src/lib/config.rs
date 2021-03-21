use std::fs::File;
use std::io::Read;

use serde_derive::Deserialize;

use crate::errors::*;
use crate::kvs_client::Address;

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

/// `Config` Contains the Anna configuration deserialized form Yaml config file
impl Config {
    /// Read the `Config` from a yaml config file and return it or Error
    pub fn read(filename: &str) -> Result<Config> {
        let mut file =
            File::open(filename).chain_err(|| format!("Could not open file '{:?}'", filename))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .chain_err(|| format!("Could not read content from '{:?}'", filename))?;
        serde_yaml::from_str(&content)
            .chain_err(|| format!("Error deserializing Yaml config from: '{}'", filename))
    }

    /// Return a vector of `Address` used for routing
    pub fn get_routing_ips(&self) -> &Vec<Address> {
        match &self.routing_elb {
            Some(elb_ip) => &elb_ip,
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

    #[test]
    fn routing_ips_no_elb() {
        let config = Config::read("src/lib/test_config.yml")
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_routing_ips(), &vec!("127.0.0.1".to_string()));
    }

    #[test]
    fn user_ip() {
        let config = Config::read("src/lib/test_config.yml")
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_user_ip(), "127.0.0.1");
    }

    #[test]
    fn routing_thread_count() {
        let config = Config::read("src/lib/test_config.yml")
            .expect("Could not read the 'test_config.yml' config file");
        assert_eq!(config.get_routing_thread_count(), 1);
    }
}
