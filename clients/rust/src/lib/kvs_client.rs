use crate::config::Config;
use crate::errors::{Error, Result};
use crate::proto::kvs::{
    AnnaError, KeyAddressRequest, KeyAddressResponse, KeyRequest, KeyResponse, KeyTuple,
    LatticeType, LwwValue, MultiKeyCausalValue, PriorityValue, RequestType, SetValue,
    SingleKeyCausalValue,
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
use zeromq::{PullSocket, PushSocket, Socket, SocketRecv, SocketSend};

/// Async client for the Anna Key-Value Store.
///
/// Communicates with the routing tier to discover worker addresses and
/// sends GET/PUT requests directly to worker nodes via ZeroMQ.
///
/// # Example
///
/// ```rust
/// # #[tokio::main]
/// # async fn main() {
/// use annalib::config::Config;
/// use annalib::kvs_client::KVSClient;
///
/// let config = Config::default();
/// let client = KVSClient::new(&config, Some(105)).await;
/// // Use client.get("key") and client.put("key", "value") with a running server
/// # }
/// ```
pub struct KVSClient {
    routing_threads: Vec<UserRoutingThread>,
    rid: usize,
    ut: UserThread,
    #[allow(dead_code)]
    seed: u64,
    rng: StdRng,
    key_address_cache: HashMap<Key, HashSet<Address>>,
    timeout: Duration,
    socket_cache: HashMap<Address, PushSocket>,
    key_address_puller: PullSocket,
    response_puller: PullSocket,
}

impl KVSClient {
    /// Create a new `KVSClient` from a `Config` and optional thread id.
    ///
    /// The `tid` parameter allows multiple clients on the same machine to
    /// use different ZMQ ports. Pass `None` for the default (tid=0).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(100)).await;
    /// # }
    /// ```
    pub async fn new(config: &Config, tid: Option<ThreadID>) -> Self {
        let tid = tid.unwrap_or(0);
        let base_offset = config.get_base_offset();
        let thread_count = config.get_routing_thread_count();
        let routing_ips = config.get_routing_ips();
        let mut routing_threads = Vec::with_capacity(routing_ips.len() * thread_count);
        for address in routing_ips {
            for i in 0..thread_count {
                routing_threads.push(UserRoutingThread::with_offset(address, i, base_offset));
            }
        }

        let seed = Self::generate_seed(config.get_user_ip(), tid);
        info!("Random seed is {}.", seed);
        let rng = StdRng::seed_from_u64(seed);

        let ut = UserThread::with_offset(config.get_user_ip(), tid, base_offset);

        let mut key_address_puller = PullSocket::new();
        key_address_puller
            .bind(&ut.key_address_bind_address())
            .await
            .expect("Failed to bind key address puller");

        let mut response_puller = PullSocket::new();
        response_puller
            .bind(&ut.response_bind_address())
            .await
            .expect("Failed to bind response puller");

        KVSClient {
            routing_threads,
            rid: 0,
            ut,
            seed,
            rng,
            key_address_cache: HashMap::new(),
            timeout: Duration::from_secs(10),
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

    async fn get_socket(&mut self, addr: &str) -> Result<&mut PushSocket> {
        if !self.socket_cache.contains_key(addr) {
            let mut sock = PushSocket::new();
            sock.connect(addr)
                .await
                .map_err(|e| Error::Kvs(format!("Failed to connect to {}: {}", addr, e)))?;
            self.socket_cache.insert(addr.to_string(), sock);
        }
        Ok(self
            .socket_cache
            .get_mut(addr)
            .expect("socket just inserted"))
    }

    async fn send_request(&mut self, msg: &[u8], addr: &str) -> Result<()> {
        let sock = self.get_socket(addr).await?;
        sock.send(msg.to_vec().into())
            .await
            .map_err(|e| Error::Kvs(format!("Failed to send: {}", e)))?;
        Ok(())
    }

    async fn recv_response(&mut self, use_key_address: bool) -> Option<Vec<u8>> {
        let sock = if use_key_address {
            &mut self.key_address_puller
        } else {
            &mut self.response_puller
        };

        match tokio::time::timeout(self.timeout, sock.recv()).await {
            Ok(Ok(msg)) => msg.into_vec().pop().map(|b| b.to_vec()),
            Ok(Err(e)) => {
                error!("ZMQ recv error: {}", e);
                None
            }
            Err(_) => None,
        }
    }

    async fn query_routing(&mut self, key: &str) -> Vec<Address> {
        let mut request = KeyAddressRequest {
            request_id: self.get_request_id(),
            response_address: self.ut.key_address_connect_address(),
            ..Default::default()
        };
        request.keys.push(key.to_string());

        let rt_thread = self.get_routing_thread();
        let encoded = request.encode_to_vec();
        self.send_request(&encoded, &rt_thread).await.ok();

        match self.recv_response(true).await {
            Some(data) => {
                debug!(
                    "Routing response: {} bytes, hex: {:02x?}",
                    data.len(),
                    &data[..std::cmp::min(data.len(), 64)]
                );
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

    async fn get_worker_address(&mut self, key: &str) -> Option<Address> {
        if !self.key_address_cache.contains_key(key) || self.key_address_cache[key].is_empty() {
            let addrs = self.query_routing(key).await;
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

    fn evict_address(&mut self, key: &str, addr: &str) {
        if let Some(addrs) = self.key_address_cache.get_mut(key) {
            addrs.remove(addr);
            if addrs.is_empty() {
                self.key_address_cache.remove(key);
            }
        }
        self.socket_cache.remove(addr);
    }

    async fn send_data_request(
        &mut self,
        key: &str,
        req_type: i32,
        lattice_type: Option<i32>,
        payload: Option<Vec<u8>>,
    ) -> Option<KeyResponse> {
        const MAX_RETRIES: usize = 5;

        for attempt in 0..=MAX_RETRIES {
            let worker = self.get_worker_address(key).await?;

            let mut request = KeyRequest {
                request_id: self.get_request_id(),
                response_address: self.ut.response_connect_address(),
                r#type: req_type,
                ..Default::default()
            };

            let mut tuple = KeyTuple {
                key: key.to_string(),
                ..Default::default()
            };
            if let Some(lt) = lattice_type {
                tuple.lattice_type = lt;
            }
            if let Some(ref p) = payload {
                tuple.payload = p.clone();
            }
            if let Some(cache) = self.key_address_cache.get(key) {
                tuple.address_cache_size = cache.len() as u32;
            }
            request.tuples.push(tuple);

            let encoded = request.encode_to_vec();

            let result = tokio::time::timeout(self.timeout, async {
                self.send_request(&encoded, &worker).await?;
                match self.recv_response(false).await {
                    Some(data) => Ok(data),
                    None => Err(Error::Kvs("recv timed out".into())),
                }
            })
            .await;

            match result {
                Ok(Ok(data)) => match KeyResponse::decode(data.as_slice()) {
                    Ok(response) => {
                        if !response.tuples.is_empty() {
                            let t = &response.tuples[0];
                            if t.error == AnnaError::WrongThread as i32 && attempt < MAX_RETRIES {
                                debug!("WRONG_THREAD for key {} at {}, retrying", key, worker);
                                self.evict_address(key, &worker);
                                continue;
                            }
                            if t.invalidate {
                                self.key_address_cache.remove(key);
                            }
                        }
                        return Some(response);
                    }
                    Err(e) => {
                        error!("Failed to decode response: {}", e);
                        return None;
                    }
                },
                _ => {
                    warn!(
                        "Request timed out for key {} at {}, attempt {}",
                        key, worker, attempt
                    );
                    self.evict_address(key, &worker);
                    if attempt < MAX_RETRIES {
                        continue;
                    }
                    return None;
                }
            }
        }
        None
    }

    fn generate_timestamp() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        now.as_millis() as u64 * 10
    }

    fn anna_error_name(code: i32) -> &'static str {
        match code {
            0 => "NO_ERROR",
            1 => "KEY_DNE",
            2 => "WRONG_THREAD",
            3 => "TIMEOUT",
            4 => "LATTICE",
            5 => "NO_SERVERS",
            _ => "UNKNOWN",
        }
    }

    fn validate_response<'a>(response: &'a KeyResponse, op: &str) -> Result<&'a KeyTuple> {
        if response.tuples.is_empty() {
            return Err(Error::Kvs(format!("{}: no tuples in response", op)));
        }
        let tuple = &response.tuples[0];
        if tuple.error != AnnaError::NoError as i32 {
            return Err(Error::Kvs(format!(
                "{}: {}",
                op,
                Self::anna_error_name(tuple.error)
            )));
        }
        Ok(tuple)
    }

    /// Retrieve a value by key (Last-Writer-Wins lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(101)).await;
    /// // let value = client.get("my_key").await?; // requires a running server
    /// # }
    /// ```
    pub async fn get<K: AsRef<str> + Display>(&mut self, key: K) -> Result<String> {
        let bytes = self.get_bytes(key).await?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Retrieve a raw binary value by key (LWW lattice, no UTF-8 conversion).
    ///
    /// Returns the inner value bytes from the LWW wrapper. Useful for reading
    /// metadata keys that contain serialized protobuf payloads.
    pub async fn get_bytes<K: AsRef<str> + Display>(&mut self, key: K) -> Result<Vec<u8>> {
        debug!("GET_BYTES: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_BYTES: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_BYTES")?;

        let lww = LwwValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_BYTES: failed to decode LWW value: {}", e)))?;
        Ok(lww.value)
    }

    /// Store a key-value pair (Last-Writer-Wins lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(102)).await;
    /// // client.put("my_key", "my_value").await?; // requires a running server
    /// # }
    /// ```
    pub async fn put<K: AsRef<str> + Display>(&mut self, key: K, value: &str) -> Result<()> {
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
            .await
            .ok_or_else(|| Error::Kvs("PUT: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT")?;
        Ok(())
    }

    /// Retrieve a set of values by key (Set lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(103)).await;
    /// // let values = client.get_set("my_set").await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "set")]
    pub async fn get_set<K: AsRef<str> + Display>(&mut self, key: K) -> Result<Vec<String>> {
        debug!("GET SET: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_SET: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_SET")?;

        let set_val = SetValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_SET: failed to decode Set value: {}", e)))?;
        Ok(set_val
            .values
            .iter()
            .map(|v| String::from_utf8_lossy(v).to_string())
            .collect())
    }

    /// Store a set of values by key (Set lattice, union semantics).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(104)).await;
    /// // client.put_set("my_set", &["a", "b", "c"]).await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "set")]
    pub async fn put_set<K: AsRef<str> + Display>(&mut self, key: K, set: &[&str]) -> Result<()> {
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
            .await
            .ok_or_else(|| Error::Kvs("PUT_SET: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_SET")?;
        Ok(())
    }

    /// Retrieve a value by key (Multi-Key Causal lattice).
    ///
    /// Returns (vector_clock, dependencies, value).
    #[cfg(feature = "causal")]
    pub async fn get_causal<K: AsRef<str> + Display>(
        &mut self,
        key: K,
    ) -> Result<(
        std::collections::HashMap<String, u32>,
        Vec<(String, std::collections::HashMap<String, u32>)>,
        String,
    )> {
        debug!("GET_CAUSAL: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_CAUSAL: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_CAUSAL")?;

        let mkc = MultiKeyCausalValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_CAUSAL: failed to decode: {}", e)))?;

        let vc = mkc.vector_clock;
        let deps: Vec<(String, std::collections::HashMap<String, u32>)> = mkc
            .dependencies
            .iter()
            .map(|kv| (kv.key.clone(), kv.vector_clock.clone()))
            .collect();
        let value = mkc
            .values
            .first()
            .map(|v| String::from_utf8_lossy(v).to_string())
            .unwrap_or_default();

        Ok((vc, deps, value))
    }

    /// Store a value by key (Multi-Key Causal lattice).
    #[cfg(feature = "causal")]
    pub async fn put_causal<K: AsRef<str> + Display>(&mut self, key: K, value: &str) -> Result<()> {
        debug!("PUT_CAUSAL: {} <- {}", key, value);
        let mut vc = std::collections::HashMap::new();
        vc.insert("test".to_string(), 1u32);

        let dep = crate::proto::shared::KeyVersion {
            key: "dep1".to_string(),
            vector_clock: {
                let mut m = std::collections::HashMap::new();
                m.insert("test1".to_string(), 1u32);
                m
            },
        };

        let mkc = MultiKeyCausalValue {
            vector_clock: vc,
            dependencies: vec![dep],
            values: vec![value.as_bytes().to_vec()],
        };
        let payload = mkc.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::MultiCausal as i32),
                Some(payload),
            )
            .await
            .ok_or_else(|| Error::Kvs("PUT_CAUSAL: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_CAUSAL")?;
        Ok(())
    }

    /// Retrieve a set of values by key (Ordered Set lattice).
    ///
    /// Returns values in the order provided by the server.
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(110)).await;
    /// // let values = client.get_ordered_set("my_oset").await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "set")]
    pub async fn get_ordered_set<K: AsRef<str> + Display>(
        &mut self,
        key: K,
    ) -> Result<Vec<String>> {
        debug!("GET ORDERED_SET: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_ORDERED_SET: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_ORDERED_SET")?;

        let set_val = SetValue::decode(tuple.payload.as_slice()).map_err(|e| {
            Error::Kvs(format!(
                "GET_ORDERED_SET: failed to decode Set value: {}",
                e
            ))
        })?;
        Ok(set_val
            .values
            .iter()
            .map(|v| String::from_utf8_lossy(v).to_string())
            .collect())
    }

    /// Store a set of values by key (Ordered Set lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(111)).await;
    /// // client.put_ordered_set("my_oset", &["x", "y", "z"]).await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "set")]
    pub async fn put_ordered_set<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        set: &[&str],
    ) -> Result<()> {
        debug!("PUT ORDERED_SET: {} <- {:?}", key, set);
        let set_val = SetValue {
            values: set.iter().map(|s| s.as_bytes().to_vec()).collect(),
        };
        let payload = set_val.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::OrderedSet as i32),
                Some(payload),
            )
            .await
            .ok_or_else(|| Error::Kvs("PUT_ORDERED_SET: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_ORDERED_SET")?;
        Ok(())
    }

    /// Retrieve a value by key (Single-Key Causal lattice).
    ///
    /// Returns (vector_clock, value).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(112)).await;
    /// // let (vc, val) = client.get_single_causal("my_key").await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "causal")]
    pub async fn get_single_causal<K: AsRef<str> + Display>(
        &mut self,
        key: K,
    ) -> Result<(HashMap<String, u32>, Vec<String>)> {
        debug!("GET_SINGLE_CAUSAL: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_SINGLE_CAUSAL: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_SINGLE_CAUSAL")?;

        let skc = SingleKeyCausalValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_SINGLE_CAUSAL: failed to decode: {}", e)))?;

        let vc = skc.vector_clock;
        let values: Vec<String> = skc
            .values
            .iter()
            .map(|v| String::from_utf8_lossy(v).to_string())
            .collect();

        Ok((vc, values))
    }

    /// Store a value by key (Single-Key Causal lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(113)).await;
    /// // client.put_single_causal("my_key", "my_value").await?; // requires a running server
    /// # }
    /// ```
    #[cfg(feature = "causal")]
    pub async fn put_single_causal<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        value: &str,
    ) -> Result<()> {
        debug!("PUT_SINGLE_CAUSAL: {} <- {}", key, value);
        let mut vc = HashMap::new();
        vc.insert("test".to_string(), 1u32);

        let skc = SingleKeyCausalValue {
            vector_clock: vc,
            values: vec![value.as_bytes().to_vec()],
        };
        let payload = skc.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::SingleCausal as i32),
                Some(payload),
            )
            .await
            .ok_or_else(|| Error::Kvs("PUT_SINGLE_CAUSAL: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_SINGLE_CAUSAL")?;
        Ok(())
    }

    /// Retrieve a value by key (Priority lattice).
    ///
    /// Returns (priority, value).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(114)).await;
    /// // let (priority, val) = client.get_priority("my_key").await?; // requires a running server
    /// # }
    /// ```
    pub async fn get_priority<K: AsRef<str> + Display>(&mut self, key: K) -> Result<(f64, String)> {
        debug!("GET_PRIORITY: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_PRIORITY: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET_PRIORITY")?;

        let pv = PriorityValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_PRIORITY: failed to decode: {}", e)))?;

        let value = String::from_utf8_lossy(&pv.value).to_string();
        Ok((pv.priority, value))
    }

    /// Store a value by key with a priority (Priority lattice).
    ///
    /// Lower priority values win in the lattice merge.
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(115)).await;
    /// // client.put_priority("my_key", 1.0, "my_value").await?; // requires a running server
    /// # }
    /// ```
    pub async fn put_priority<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        priority: f64,
        value: &str,
    ) -> Result<()> {
        debug!("PUT_PRIORITY: {} <- {} (priority {})", key, value, priority);
        let pv = PriorityValue {
            priority,
            value: value.as_bytes().to_vec(),
        };
        let payload = pv.encode_to_vec();

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(LatticeType::Priority as i32),
                Some(payload),
            )
            .await
            .ok_or_else(|| Error::Kvs("PUT_PRIORITY: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_PRIORITY")?;
        Ok(())
    }

    /// Delete a key by writing an empty LWW value with a dominating timestamp.
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::config::Config::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(116)).await;
    /// // client.delete("my_key").await?; // requires a running server
    /// # }
    /// ```
    pub async fn delete<K: AsRef<str> + Display>(&mut self, key: K) -> Result<()> {
        self.put(key, "").await
    }

    /// Retrieve multiple keys in a single batched request (LWW lattice).
    ///
    /// Keys that map to the same worker are sent in one `KeyRequest` with
    /// multiple tuples, which is more efficient than individual `get` calls.
    /// Returns a map from key to value for all keys that were found.
    /// Keys that return `KEY_DNE` are omitted from the result.
    pub async fn get_multi<K: AsRef<str> + Display>(
        &mut self,
        keys: &[K],
    ) -> Result<HashMap<String, String>> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        const MAX_RETRIES: usize = 3;
        let mut results = HashMap::new();
        let mut pending: Vec<String> = keys.iter().map(|k| k.as_ref().to_string()).collect();

        for attempt in 0..=MAX_RETRIES {
            if pending.is_empty() {
                break;
            }

            let mut worker_keys: HashMap<Address, Vec<String>> = HashMap::new();
            for key in &pending {
                if let Some(worker) = self.get_worker_address(key).await {
                    worker_keys.entry(worker).or_default().push(key.clone());
                } else {
                    return Err(Error::Kvs(format!(
                        "GET_MULTI: failed to resolve address for key {}",
                        key
                    )));
                }
            }

            let mut retry_keys = Vec::new();

            for (worker, batch_keys) in &worker_keys {
                let mut request = KeyRequest {
                    request_id: self.get_request_id(),
                    response_address: self.ut.response_connect_address(),
                    r#type: RequestType::Get as i32,
                    ..Default::default()
                };

                for key in batch_keys {
                    let mut tuple = KeyTuple {
                        key: key.clone(),
                        ..Default::default()
                    };
                    if let Some(cache) = self.key_address_cache.get(key) {
                        tuple.address_cache_size = cache.len() as u32;
                    }
                    request.tuples.push(tuple);
                }

                let encoded = request.encode_to_vec();
                self.send_request(&encoded, worker).await?;

                match self.recv_response(false).await {
                    Some(data) => {
                        let response = KeyResponse::decode(data.as_slice())
                            .map_err(|e| Error::Kvs(format!("GET_MULTI: decode error: {}", e)))?;

                        for tuple in &response.tuples {
                            if tuple.invalidate {
                                self.key_address_cache.remove(&tuple.key);
                            }
                            if tuple.error == AnnaError::WrongThread as i32 {
                                self.key_address_cache.remove(&tuple.key);
                                if attempt < MAX_RETRIES {
                                    retry_keys.push(tuple.key.clone());
                                }
                            } else if tuple.error == AnnaError::NoError as i32 {
                                let lww =
                                    LwwValue::decode(tuple.payload.as_slice()).map_err(|e| {
                                        Error::Kvs(format!(
                                            "GET_MULTI: failed to decode LWW for key {}: {}",
                                            tuple.key, e
                                        ))
                                    })?;
                                results.insert(
                                    tuple.key.clone(),
                                    String::from_utf8_lossy(&lww.value).to_string(),
                                );
                            }
                        }
                    }
                    None => {
                        for key in batch_keys {
                            self.key_address_cache.remove(key);
                        }
                        return Err(Error::Kvs("GET_MULTI: request timed out".into()));
                    }
                }
            }

            pending = retry_keys;
        }

        Ok(results)
    }

    /// Set the per-key replication factor by writing to the metadata key.
    ///
    /// The replication metadata is stored as an LWW value containing a
    /// serialized `ReplicationFactor` protobuf at key
    /// `ANNA_METADATA|replication|<key>`.
    pub async fn put_replication_factor(
        &mut self,
        key: &str,
        memory_replication: u32,
        local_replication: u32,
    ) -> Result<()> {
        use crate::proto::metadata::{
            replication_factor::ReplicationValue, ReplicationFactor, Tier,
        };

        let rep = ReplicationFactor {
            key: key.to_string(),
            global: vec![
                ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: memory_replication,
                },
                ReplicationValue {
                    tier: Tier::Disk as i32,
                    value: 0,
                },
            ],
            local: vec![
                ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: local_replication,
                },
                ReplicationValue {
                    tier: Tier::Disk as i32,
                    value: 0,
                },
            ],
        };

        let meta_key = format!("ANNA_METADATA|replication|{}", key);
        let payload = rep.encode_to_vec();
        let lww = LwwValue {
            timestamp: Self::generate_timestamp(),
            value: payload,
        };

        let response = self
            .send_data_request(
                &meta_key,
                RequestType::Put as i32,
                Some(LatticeType::Lww as i32),
                Some(lww.encode_to_vec()),
            )
            .await
            .ok_or_else(|| {
                Error::Kvs("PUT_REPLICATION_FACTOR: request failed or timed out".into())
            })?;

        Self::validate_response(&response, "PUT_REPLICATION_FACTOR")?;
        Ok(())
    }

    /// Query routing for a key and return all responsible server addresses.
    pub async fn get_key_addresses(&mut self, key: &str) -> Vec<Address> {
        self.key_address_cache.remove(key);
        self.query_routing(key).await
    }

    /// Clear the key-address cache.
    pub fn clear_cache(&mut self) {
        self.key_address_cache.clear()
    }

    /// Set the request timeout duration.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_seed_is_deterministic_for_same_inputs_at_same_time() {
        let s1 = KVSClient::generate_seed(&"127.0.0.1".to_string(), 0);
        let s2 = KVSClient::generate_seed(&"127.0.0.1".to_string(), 0);
        assert!((s1 as i64 - s2 as i64).unsigned_abs() < 100);
    }

    #[test]
    fn generate_seed_differs_by_tid() {
        let s1 = KVSClient::generate_seed(&"127.0.0.1".to_string(), 0);
        let s2 = KVSClient::generate_seed(&"127.0.0.1".to_string(), 1);
        assert_ne!(s1, s2);
    }

    #[test]
    fn generate_seed_differs_by_ip() {
        let s1 = KVSClient::generate_seed(&"127.0.0.1".to_string(), 0);
        let s2 = KVSClient::generate_seed(&"10.0.0.1".to_string(), 0);
        assert_ne!(s1, s2);
    }

    #[test]
    fn generate_timestamp_is_positive() {
        let ts = KVSClient::generate_timestamp();
        assert!(ts > 0);
    }

    #[test]
    fn generate_timestamp_increases() {
        let ts1 = KVSClient::generate_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let ts2 = KVSClient::generate_timestamp();
        assert!(ts2 > ts1);
    }

    #[test]
    fn lww_value_roundtrip() {
        let original = LwwValue {
            timestamp: 12345,
            value: b"hello world".to_vec(),
        };
        let encoded = original.encode_to_vec();
        let decoded = LwwValue::decode(encoded.as_slice()).expect("failed to decode LwwValue");
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.value, b"hello world");
    }

    #[test]
    fn set_value_roundtrip() {
        let original = SetValue {
            values: vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
        };
        let encoded = original.encode_to_vec();
        let decoded = SetValue::decode(encoded.as_slice()).expect("failed to decode SetValue");
        assert_eq!(decoded.values.len(), 3);
        assert!(decoded.values.contains(&b"a".to_vec()));
        assert!(decoded.values.contains(&b"b".to_vec()));
        assert!(decoded.values.contains(&b"c".to_vec()));
    }

    #[tokio::test]
    async fn client_construction_and_request_id() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(99)).await;
        let id1 = client.get_request_id();
        let id2 = client.get_request_id();
        assert!(id1.contains("127.0.0.1"));
        assert!(id1.contains("99"));
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn client_routing_thread_returns_address() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(98)).await;
        let addr = client.get_routing_thread();
        assert!(addr.starts_with("tcp://"), "addr was: {}", addr);
        assert!(addr.contains("127.0.0.1"), "addr was: {}", addr);
    }

    #[tokio::test]
    async fn client_clear_cache() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(97)).await;
        client
            .key_address_cache
            .insert("test_key".into(), ["addr1".to_string()].into());
        assert!(!client.key_address_cache.is_empty());
        client.clear_cache();
        assert!(client.key_address_cache.is_empty());
    }

    #[tokio::test]
    async fn get_worker_address_returns_cached() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(96)).await;
        client.key_address_cache.insert(
            "cached_key".into(),
            ["tcp://127.0.0.1:6200".to_string()].into(),
        );
        let addr = client.get_worker_address("cached_key").await;
        assert_eq!(addr, Some("tcp://127.0.0.1:6200".to_string()));
    }

    #[tokio::test]
    async fn get_worker_address_picks_from_multi_address_cache() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(95)).await;
        let mut addrs = HashSet::new();
        addrs.insert("tcp://10.0.0.1:6200".to_string());
        addrs.insert("tcp://10.0.0.2:6200".to_string());
        client.key_address_cache.insert("multi_key".into(), addrs);
        let addr = client
            .get_worker_address("multi_key")
            .await
            .expect("expected cached address for multi_key");
        assert!(
            addr == "tcp://10.0.0.1:6200" || addr == "tcp://10.0.0.2:6200",
            "unexpected addr: {}",
            addr
        );
    }

    #[tokio::test]
    async fn invalidate_cache_removes_key() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(92)).await;
        client.key_address_cache.insert(
            "evict_me".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        assert!(client.key_address_cache.contains_key("evict_me"));
        client.key_address_cache.remove("evict_me");
        assert!(!client.key_address_cache.contains_key("evict_me"));
    }

    #[tokio::test]
    async fn evict_address_removes_single_address() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(88)).await;
        let mut addrs = HashSet::new();
        addrs.insert("tcp://10.0.0.1:6200".to_string());
        addrs.insert("tcp://10.0.0.2:6200".to_string());
        client.key_address_cache.insert("multi_addr".into(), addrs);

        client.evict_address("multi_addr", "tcp://10.0.0.1:6200");
        let remaining = &client.key_address_cache["multi_addr"];
        assert_eq!(remaining.len(), 1);
        assert!(remaining.contains("tcp://10.0.0.2:6200"));
    }

    #[tokio::test]
    async fn evict_address_removes_key_when_last_address() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(87)).await;
        client.key_address_cache.insert(
            "single_addr".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );

        client.evict_address("single_addr", "tcp://10.0.0.1:6200");
        assert!(!client.key_address_cache.contains_key("single_addr"));
    }

    #[tokio::test]
    async fn evict_address_also_removes_socket() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(86)).await;
        client.key_address_cache.insert(
            "sock_test".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        let sock = PushSocket::new();
        client
            .socket_cache
            .insert("tcp://10.0.0.1:6200".to_string(), sock);

        client.evict_address("sock_test", "tcp://10.0.0.1:6200");
        assert!(!client.socket_cache.contains_key("tcp://10.0.0.1:6200"));
    }

    #[tokio::test]
    async fn set_timeout_changes_duration() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(85)).await;
        assert_eq!(client.timeout, Duration::from_secs(10));
        client.set_timeout(Duration::from_secs(3));
        assert_eq!(client.timeout, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn get_key_addresses_clears_cache_first() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(84)).await;
        client.key_address_cache.insert(
            "stale_key".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        let addrs = client.get_key_addresses("stale_key").await;
        assert!(
            addrs.is_empty(),
            "Expected empty (no routing server), got {:?}",
            addrs
        );
        assert!(!client.key_address_cache.contains_key("stale_key"));
    }

    #[test]
    fn replication_factor_protobuf_roundtrip() {
        use crate::proto::metadata::{
            replication_factor::ReplicationValue, ReplicationFactor, Tier,
        };

        let rep = ReplicationFactor {
            key: "test_key".to_string(),
            global: vec![
                ReplicationValue {
                    tier: Tier::Memory as i32,
                    value: 2,
                },
                ReplicationValue {
                    tier: Tier::Disk as i32,
                    value: 0,
                },
            ],
            local: vec![ReplicationValue {
                tier: Tier::Memory as i32,
                value: 1,
            }],
        };

        let encoded = rep.encode_to_vec();
        let decoded = ReplicationFactor::decode(encoded.as_slice())
            .expect("failed to decode ReplicationFactor");
        assert_eq!(decoded.key, "test_key");
        assert_eq!(decoded.global.len(), 2);
        assert_eq!(decoded.global[0].value, 2);
        assert_eq!(decoded.local[0].value, 1);
    }

    #[test]
    fn empty_set_value_roundtrip() {
        let original = SetValue { values: vec![] };
        let encoded = original.encode_to_vec();
        let decoded =
            SetValue::decode(encoded.as_slice()).expect("failed to decode empty SetValue");
        assert!(decoded.values.is_empty());
    }

    #[test]
    fn lww_value_with_empty_payload() {
        let original = LwwValue {
            timestamp: 0,
            value: vec![],
        };
        let encoded = original.encode_to_vec();
        let decoded =
            LwwValue::decode(encoded.as_slice()).expect("failed to decode empty LwwValue");
        assert_eq!(decoded.timestamp, 0);
        assert!(decoded.value.is_empty());
    }

    #[tokio::test]
    async fn request_id_format() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .expect("failed to read default config");
        let mut client = KVSClient::new(&config, Some(91)).await;
        let id = client.get_request_id();
        let parts: Vec<&str> = id.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[1].contains('_'));
    }

    #[test]
    fn anna_error_name_known_codes() {
        assert_eq!(KVSClient::anna_error_name(0), "NO_ERROR");
        assert_eq!(KVSClient::anna_error_name(1), "KEY_DNE");
        assert_eq!(KVSClient::anna_error_name(2), "WRONG_THREAD");
        assert_eq!(KVSClient::anna_error_name(3), "TIMEOUT");
        assert_eq!(KVSClient::anna_error_name(5), "NO_SERVERS");
        assert_eq!(KVSClient::anna_error_name(99), "UNKNOWN");
    }

    #[test]
    fn validate_response_empty_tuples() {
        let response = KeyResponse::default();
        let result = KVSClient::validate_response(&response, "TEST");
        assert!(result.is_err());
        assert!(result
            .expect_err("expected error for empty tuples")
            .to_string()
            .contains("no tuples"));
    }

    #[test]
    fn validate_response_error_code() {
        let mut response = KeyResponse::default();
        let tuple = KeyTuple {
            error: AnnaError::KeyDne as i32,
            ..Default::default()
        };
        response.tuples.push(tuple);
        let result = KVSClient::validate_response(&response, "TEST");
        assert!(result.is_err());
        assert!(result
            .expect_err("expected error for KEY_DNE")
            .to_string()
            .contains("KEY_DNE"));
    }

    #[test]
    fn validate_response_success() {
        let mut response = KeyResponse::default();
        let tuple = KeyTuple {
            error: AnnaError::NoError as i32,
            key: "mykey".into(),
            ..Default::default()
        };
        response.tuples.push(tuple);
        let result = KVSClient::validate_response(&response, "TEST");
        assert!(result.is_ok());
        assert_eq!(result.expect("expected successful validation").key, "mykey");
    }

    #[test]
    fn key_request_protobuf_roundtrip() {
        let lww = LwwValue {
            timestamp: 100,
            value: b"test".to_vec(),
        };
        let tuple = KeyTuple {
            key: "mykey".to_string(),
            lattice_type: LatticeType::Lww as i32,
            payload: lww.encode_to_vec(),
            ..Default::default()
        };
        let mut request = KeyRequest {
            request_id: "test:0_1".to_string(),
            response_address: "tcp://127.0.0.1:6800".to_string(),
            r#type: RequestType::Put as i32,
            ..Default::default()
        };
        request.tuples.push(tuple);

        let encoded = request.encode_to_vec();
        let decoded = KeyRequest::decode(encoded.as_slice()).expect("failed to decode KeyRequest");
        assert_eq!(decoded.request_id, "test:0_1");
        assert_eq!(decoded.tuples[0].key, "mykey");
        assert_eq!(decoded.tuples[0].lattice_type, LatticeType::Lww as i32);
    }

    #[test]
    fn key_address_request_protobuf_roundtrip() {
        let mut request = KeyAddressRequest {
            request_id: "addr_req_1".to_string(),
            response_address: "tcp://127.0.0.1:6850".to_string(),
            ..Default::default()
        };
        request.keys.push("lookup_key".to_string());

        let encoded = request.encode_to_vec();
        let decoded = KeyAddressRequest::decode(encoded.as_slice())
            .expect("failed to decode KeyAddressRequest");
        assert_eq!(decoded.request_id, "addr_req_1");
        assert_eq!(decoded.keys[0], "lookup_key");
    }

    #[test]
    fn priority_value_roundtrip() {
        let original = PriorityValue {
            priority: 2.75,
            value: b"important".to_vec(),
        };
        let encoded = original.encode_to_vec();
        let decoded =
            PriorityValue::decode(encoded.as_slice()).expect("failed to decode PriorityValue");
        assert!((decoded.priority - 2.75).abs() < f64::EPSILON);
        assert_eq!(decoded.value, b"important");
    }

    #[test]
    fn ordered_set_value_roundtrip() {
        let original = SetValue {
            values: vec![b"x".to_vec(), b"y".to_vec(), b"z".to_vec()],
        };
        let encoded = original.encode_to_vec();
        let decoded =
            SetValue::decode(encoded.as_slice()).expect("failed to decode ordered SetValue");
        assert_eq!(decoded.values.len(), 3);
        assert_eq!(decoded.values[0], b"x");
        assert_eq!(decoded.values[1], b"y");
        assert_eq!(decoded.values[2], b"z");
    }

    #[test]
    fn single_causal_value_roundtrip() {
        let mut vc = HashMap::new();
        vc.insert("node1".to_string(), 5u32);
        vc.insert("node2".to_string(), 3u32);

        let original = SingleKeyCausalValue {
            vector_clock: vc,
            values: vec![b"causal_data".to_vec()],
        };
        let encoded = original.encode_to_vec();
        let decoded = SingleKeyCausalValue::decode(encoded.as_slice())
            .expect("failed to decode SingleKeyCausalValue");
        assert_eq!(decoded.vector_clock.len(), 2);
        assert_eq!(decoded.vector_clock["node1"], 5);
        assert_eq!(decoded.vector_clock["node2"], 3);
        assert_eq!(decoded.values.len(), 1);
        assert_eq!(decoded.values[0], b"causal_data");
    }
}
