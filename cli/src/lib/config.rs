use std::fs::File;
use std::io::Read;

use serde_derive::Deserialize;

use crate::errors::*;
use crate::kvs_client::Address;

/// `Config` contains the configuration read from the yaml config file
#[derive(Deserialize)]
struct Config {
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

/*
monitoring:
  mgmt_ip: 127.0.0.1
  ip: 127.0.0.1
  */
#[derive(Deserialize)]
struct Monitoring {
    mgmt_ip: Address,
    ip: Address,
}

/*
routing:
  monitoring:
      - 127.0.0.1
  ip: 127.0.0.1
  */
#[derive(Deserialize)]
struct Routing {
    monitoring: Vec<Address>,
    ip: Address
}

/*
user:
  monitoring:
      - 127.0.0.1
  routing:
      - 127.0.0.1
  ip: 127.0.0.1
  */
#[derive(Deserialize)]
struct User {
    monitoring: Vec<Address>,
    routing: Vec<Address>,
    ip: Address
}

/*
server:
  monitoring:
      - 127.0.0.1
  routing:
      - 127.0.0.1
  seed_ip: 127.0.0.1
  public_ip: 127.0.0.1
  private_ip: 127.0.0.1
  mgmt_ip: "NULL"
  */
#[derive(Deserialize)]
struct Server {
    monitoring: Vec<Address>,
    routing: Vec<Address>,
    seed_ip: Address,
    public_ip: Address,
    private_ip: Address,
    mgmt_ip: Address,
}

/*
policy:
  elasticity: false
  selective-rep: false
  tiering: false
 */
#[derive(Deserialize)]
struct Policy {
    elasticity: bool,
    #[serde(rename = "selective-rep")]
    selective_rep: bool,
    tiering: bool,
}

/*
ebs: ./
*/
#[derive(Deserialize)]
struct Ebs(String);

/*
capacities: # in GB
  memory-cap: 1
  ebs-cap: 0
*/
#[derive(Deserialize)]
struct Capacities {
    #[serde(rename = "memory-cap")]
    memory_cap: usize,
    #[serde(rename = "ebs-cap")]
    ebs_cap: usize
}

/*
threads:
  memory: 1
  ebs: 1
  routing: 1
  benchmark: 1
*/
#[derive(Deserialize)]
struct Threads {
    memory: usize,
    ebs: usize,
    routing: usize,
    benchmark: usize,
}

/*
replication:
  memory: 1
  ebs: 0
  minimum: 1
  local: 1
*/
#[derive(Deserialize)]
struct Replication {
    memory: usize,
    ebs: usize,
    minimum: usize,
    local: usize,
}

/// `Config` Contains the Anna configuration deserialized form Yaml config file
impl Config {
    // Get chain_err() working with Foreign error types from lib,.rs
    // read the YAML config from a file into a Config structure
    pub fn read(filename: &str) -> Result<Config> {
        let mut file = File::open(filename)
            .map_err(|_| format!("Could not open file '{:?}'", filename))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| format!("Could not read content from '{:?}'", filename))?;
        serde_yaml::from_str(&content)
            .chain_err(|| format!("Error deserializing Yaml config from: '{}'", filename))

    }

    //   if (YAML::Node elb = user["routing-elb"]) {
    //     routing_ips.push_back(elb.as<string>());
    //   } else {
    //     YAML::Node routing = user["routing"];
    //     for (const YAML::Node &node : routing) {
    //       routing_ips.push_back(node.as<Address>());
    //     }
    //   }
    pub fn get_routing_ips(&self) -> &Vec<Address> {
        match &self.routing_elb {
            Some(elb_ip) => &elb_ip,
            None => &self.user.routing
        }
    }

    pub fn get_user_ip(&self) -> &Address {
        &self.user.ip
    }

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