use crate::config::Config;
use crate::errors::*;
use crate::proto::kvs::{
    AnnaError, KeyAddressRequest, KeyAddressResponse, KeyRequest, KeyResponse, KeyTuple,
    LatticeType, LwwValue, RequestType, SetValue,
};
use crate::threads::{UserRoutingThread, UserThread};
use crate::types::{Address, Key, ThreadID};
use log::{debug, error, info, warn};
use prost::Message;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zmq::Context;

/// `KVSClient` provides operations against the Anna Key-Value Store server.
/// It communicates with the routing tier to discover worker addresses and
/// sends GET/PUT requests directly to worker nodes via ZMQ.
pub struct KVSClient {
    routing_threads: Vec<UserRoutingThread>,
    rid: usize,
    ut: UserThread,
    #[allow(dead_code)]
    seed: u64,
    rng: StdRng,
    context: Context,
    key_address_cache: HashMap<Key, HashSet<Address>>,
    timeout: i64,
    socket_cache: HashMap<Address, zmq::Socket>,
    key_address_puller: zmq::Socket,
    response_puller: zmq::Socket,
}

impl KVSClient {
    /// Create a new `KVSClient` from a `Config` and optional thread id.
    pub fn new(config: &Config, tid: Option<ThreadID>) -> Self {
        let tid = tid.unwrap_or(0);
        let thread_count = config.get_routing_thread_count();
        let routing_ips = config.get_routing_ips();
        let mut routing_threads = Vec::with_capacity(routing_ips.len() * thread_count);
        for address in routing_ips {
            for i in 0..thread_count {
                routing_threads.push(UserRoutingThread::new(address, i));
            }
        }

        let seed = Self::generate_seed(config.get_user_ip(), tid);
        info!("Random seed is {}.", seed);
        let rng = StdRng::seed_from_u64(seed);

        let context = Context::new();

        let key_address_puller = context.socket(zmq::PULL).expect("Failed to create ZMQ PULL socket");
        let response_puller = context.socket(zmq::PULL).expect("Failed to create ZMQ PULL socket");

        let ut = UserThread::new(config.get_user_ip(), tid);
        key_address_puller
            .bind(&ut.key_address_bind_address())
            .expect("Failed to bind key address puller");
        response_puller
            .bind(&ut.response_bind_address())
            .expect("Failed to bind response puller");

        KVSClient {
            routing_threads,
            rid: 0,
            ut,
            seed,
            rng,
            context,
            key_address_cache: HashMap::new(),
            timeout: 10000,
            socket_cache: HashMap::new(),
            key_address_puller,
            response_puller,
        }
    }

    fn generate_seed(ip: &Address, tid: ThreadID) -> u64 {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_micros(42));
        let mut seed =
            since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000;
        let mut hasher = DefaultHasher::new();
        ip.hash(&mut hasher);
        seed += hasher.finish();
        seed += tid as u64;
        seed
    }

    fn get_request_id(&mut self) -> String {
        self.rid += 1;
        format!("{}:{}_{}", self.ut.ip(), self.ut.tid(), self.rid)
    }

    fn get_routing_thread(&mut self) -> Address {
        self.routing_threads[self.rng.random_range(0..self.routing_threads.len())]
            .key_address_connect_address()
    }

    fn get_socket(&mut self, addr: &str) -> &zmq::Socket {
        if !self.socket_cache.contains_key(addr) {
            let sock = self.context.socket(zmq::PUSH).expect("Failed to create PUSH socket");
            sock.connect(addr).expect("Failed to connect PUSH socket");
            self.socket_cache.insert(addr.to_string(), sock);
        }
        &self.socket_cache[addr]
    }

    fn send_request(&mut self, msg: &[u8], addr: &str) {
        let sock = self.get_socket(addr);
        sock.send(msg, 0).expect("Failed to send ZMQ message");
    }

    fn recv_response(&self, sock: &zmq::Socket) -> Option<Vec<u8>> {
        let mut items = [sock.as_poll_item(zmq::POLLIN)];
        zmq::poll(&mut items, self.timeout).ok()?;
        if items[0].is_readable() {
            sock.recv_bytes(0).ok()
        } else {
            None
        }
    }

    fn query_routing(&mut self, key: &str) -> Vec<Address> {
        let mut request = KeyAddressRequest::default();
        request.request_id = self.get_request_id();
        request.response_address = self.ut.key_address_connect_address();
        request.keys.push(key.to_string());

        let rt_thread = self.get_routing_thread();
        let encoded = request.encode_to_vec();
        self.send_request(&encoded, &rt_thread);

        match self.recv_response(&self.key_address_puller) {
            Some(data) => {
                match KeyAddressResponse::decode(data.as_slice()) {
                    Ok(response) => {
                        if response.error != AnnaError::NoError as i32 {
                            warn!("Routing query returned error {}", response.error);
                            return vec![];
                        }
                        let mut addrs = vec![];
                        for addr in &response.addresses {
                            if addr.key == key {
                                for ip in &addr.ips {
                                    addrs.push(ip.clone());
                                }
                            }
                        }
                        addrs
                    }
                    Err(e) => {
                        error!("Failed to decode routing response: {}", e);
                        vec![]
                    }
                }
            }
            None => {
                warn!("Routing query timed out for key {}", key);
                vec![]
            }
        }
    }

    fn get_worker_address(&mut self, key: &str) -> Option<Address> {
        if !self.key_address_cache.contains_key(key)
            || self.key_address_cache[key].is_empty()
        {
            let addrs = self.query_routing(key);
            if addrs.is_empty() {
                return None;
            }
            self.key_address_cache
                .insert(key.to_string(), addrs.into_iter().collect());
        }

        let addrs: Vec<&Address> = self.key_address_cache[key].iter().collect();
        if addrs.is_empty() {
            None
        } else {
            let idx = self.rng.random_range(0..addrs.len());
            Some(addrs[idx].clone())
        }
    }

    fn send_data_request(&mut self, key: &str, req_type: i32, lattice_type: Option<i32>, payload: Option<Vec<u8>>) -> Option<KeyResponse> {
        let worker = self.get_worker_address(key)?;

        let mut request = KeyRequest::default();
        request.request_id = self.get_request_id();
        request.response_address = self.ut.response_connect_address();
        request.r#type = req_type;

        let mut tuple = KeyTuple::default();
        tuple.key = key.to_string();
        if let Some(lt) = lattice_type {
            tuple.lattice_type = lt;
        }
        if let Some(p) = payload {
            tuple.payload = p;
        }
        if let Some(cache) = self.key_address_cache.get(key) {
            tuple.address_cache_size = cache.len() as u32;
        }
        request.tuples.push(tuple);

        let encoded = request.encode_to_vec();
        self.send_request(&encoded, &worker);

        match self.recv_response(&self.response_puller) {
            Some(data) => {
                match KeyResponse::decode(data.as_slice()) {
                    Ok(response) => {
                        if !response.tuples.is_empty() && response.tuples[0].invalidate {
                            self.key_address_cache.remove(key);
                        }
                        Some(response)
                    }
                    Err(e) => {
                        error!("Failed to decode response: {}", e);
                        None
                    }
                }
            }
            None => {
                warn!("Request timed out for key {}", key);
                None
            }
        }
    }

    fn generate_timestamp() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        now.as_millis() as u64 * 10
    }

    /// Perform a blocking GET for a LWW key, returning the value as a String.
    pub fn get<K: AsRef<str> + Display>(&mut self, key: K) -> Result<String> {
        debug!("GET: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .ok_or("Request failed or timed out")?;

        if response.tuples.is_empty() {
            return Err("No tuples in response".into());
        }

        let tuple = &response.tuples[0];
        if tuple.error != AnnaError::NoError as i32 {
            return Err(format!("Error {}", tuple.error).into());
        }

        let lww = LwwValue::decode(tuple.payload.as_slice())
            .map_err(|e| format!("Failed to decode LWW value: {}", e))?;
        Ok(String::from_utf8_lossy(&lww.value).to_string())
    }

    /// Perform a blocking PUT of a LWW key-value pair.
    pub fn put<K: AsRef<str> + Display>(&mut self, key: K, value: &str) -> Result<()> {
        debug!("PUT: {} <- {}", key, value);
        let lww = LwwValue {
            timestamp: Self::generate_timestamp(),
            value: value.as_bytes().to_vec(),
        };
        let payload = lww.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::Lww as i32),
                Some(payload),
            )
            .ok_or("Request failed or timed out")?;

        if !response.tuples.is_empty() && response.tuples[0].error != AnnaError::NoError as i32 {
            return Err(format!("PUT error {}", response.tuples[0].error).into());
        }

        Ok(())
    }

    /// Perform a blocking GET for a Set key, returning the values.
    #[cfg(feature = "set")]
    pub fn get_set<K: AsRef<str> + Display>(&mut self, key: K) -> Result<Vec<String>> {
        debug!("GET SET: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .ok_or("Request failed or timed out")?;

        if response.tuples.is_empty() {
            return Err("No tuples in response".into());
        }

        let tuple = &response.tuples[0];
        if tuple.error != AnnaError::NoError as i32 {
            return Err(format!("Error {}", tuple.error).into());
        }

        let set_val = SetValue::decode(tuple.payload.as_slice())
            .map_err(|e| format!("Failed to decode Set value: {}", e))?;
        Ok(set_val
            .values
            .iter()
            .map(|v| String::from_utf8_lossy(v).to_string())
            .collect())
    }

    /// Perform a blocking PUT of a Set key with the given values.
    #[cfg(feature = "set")]
    pub fn put_set<K: AsRef<str> + Display>(&mut self, key: K, set: &[&str]) -> Result<()> {
        debug!("PUT SET: {} <- {:?}", key, set);
        let set_val = SetValue {
            values: set.iter().map(|s| s.as_bytes().to_vec()).collect(),
        };
        let payload = set_val.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::Set as i32),
                Some(payload),
            )
            .ok_or("Request failed or timed out")?;

        if !response.tuples.is_empty() && response.tuples[0].error != AnnaError::NoError as i32 {
            return Err(format!("PUT_SET error {}", response.tuples[0].error).into());
        }

        Ok(())
    }

    /// Perform a blocking causal GET (not yet implemented).
    #[cfg(feature = "causal")]
    pub fn get_causal<K: AsRef<str> + Display>(&mut self, key: K) -> Result<String> {
        debug!("GET_CAUSAL: {}", key);
        unimplemented!("Causal operations require additional protocol support")
    }

    /// Perform a blocking causal PUT (not yet implemented).
    #[cfg(feature = "causal")]
    pub fn put_causal<K: AsRef<str> + Display>(&mut self, key: K, value: &str) -> Result<()> {
        debug!("PUT_CAUSAL: {} <- {}", key, value);
        unimplemented!("Causal operations require additional protocol support")
    }

    /// Clear the key-address cache.
    pub fn clear_cache(&mut self) {
        self.key_address_cache.clear()
    }
}
