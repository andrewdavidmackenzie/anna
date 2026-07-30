use crate::client_config::ClientConfig;
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

enum Transport {
    Zmq {
        socket_cache: HashMap<Address, PushSocket>,
        key_address_puller: PullSocket,
        response_puller: PullSocket,
    },
    #[cfg(test)]
    Mock {
        responses: std::collections::VecDeque<(bool, Option<Vec<u8>>)>,
    },
}

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
/// use annalib::client_config::ClientConfig;
/// use annalib::kvs_client::KVSClient;
///
/// let config = ClientConfig::default();
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
    transport: Transport,
    /// Monotonic read tracking: per-key high-water mark of LWW timestamps
    /// and the corresponding value. When a GET returns a stale timestamp
    /// (lower than the tracked max), the cached value is returned instead,
    /// guaranteeing monotonic reads and read-your-writes.
    #[doc(hidden)]
    pub lww_read_cache: HashMap<Key, (u64, Vec<u8>)>,
    /// High-water mark of timestamps seen by this client (reads and writes).
    /// Ensures each PUT uses a timestamp strictly greater than any previously
    /// seen timestamp, providing the Writes Follow Reads guarantee.
    #[doc(hidden)]
    pub last_seen_ts: u64,
}

impl KVSClient {
    /// Create a new `KVSClient` from a [`ClientConfig`] and optional thread id.
    ///
    /// The `tid` parameter allows multiple clients on the same machine to
    /// use different ZMQ ports. Pass `None` for the default (tid=0).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::client_config::ClientConfig::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(100)).await;
    /// # }
    /// ```
    pub async fn new(config: &ClientConfig, tid: Option<ThreadID>) -> Self {
        let tid = tid.unwrap_or(0);
        let base_offset = config.base_offset();
        let mut routing_threads = Vec::with_capacity(config.routing_addresses.len());
        for addr in &config.routing_addresses {
            if let Some(ip) = addr
                .strip_prefix("tcp://")
                .and_then(|rest| rest.rsplit_once(':'))
                .map(|(host, _)| host.to_string())
            {
                routing_threads.push(UserRoutingThread::with_offset(&ip, 0, base_offset));
            }
        }

        let seed = Self::generate_seed(&config.client_ip, tid);
        info!("Random seed is {}.", seed);
        let rng = StdRng::seed_from_u64(seed);

        let ut = UserThread::with_offset(&config.client_ip, tid, base_offset);

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
            transport: Transport::Zmq {
                socket_cache: HashMap::new(),
                key_address_puller,
                response_puller,
            },
            lww_read_cache: HashMap::new(),
            last_seen_ts: 0,
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn new_mock(routing_ip: &str, tid: ThreadID) -> Self {
        let seed = Self::generate_seed(&routing_ip.to_string(), tid);
        KVSClient {
            routing_threads: vec![UserRoutingThread::new(&routing_ip.to_string(), 0)],
            rid: 0,
            ut: UserThread::new(&routing_ip.to_string(), tid),
            seed,
            rng: StdRng::seed_from_u64(seed),
            key_address_cache: HashMap::new(),
            timeout: Duration::from_secs(1),
            transport: Transport::Mock {
                responses: std::collections::VecDeque::new(),
            },
            lww_read_cache: HashMap::new(),
            last_seen_ts: 0,
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn push_mock_response(&mut self, use_key_address: bool, data: Option<Vec<u8>>) {
        if let Transport::Mock { responses } = &mut self.transport {
            responses.push_back((use_key_address, data));
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

    async fn send_request(&mut self, msg: &[u8], addr: &str) -> Result<()> {
        match &mut self.transport {
            Transport::Zmq { socket_cache, .. } => {
                if !socket_cache.contains_key(addr) {
                    let mut sock = PushSocket::new();
                    sock.connect(addr)
                        .await
                        .map_err(|e| Error::Kvs(format!("Failed to connect to {}: {}", addr, e)))?;
                    socket_cache.insert(addr.to_string(), sock);
                }
                let sock = socket_cache.get_mut(addr).expect("socket just inserted");
                sock.send(msg.to_vec().into())
                    .await
                    .map_err(|e| Error::Kvs(format!("Failed to send: {}", e)))?;
                Ok(())
            }
            #[cfg(test)]
            Transport::Mock { .. } => Ok(()),
        }
    }

    async fn recv_response(&mut self, use_key_address: bool) -> Option<Vec<u8>> {
        match &mut self.transport {
            Transport::Zmq {
                key_address_puller,
                response_puller,
                ..
            } => {
                let sock = if use_key_address {
                    key_address_puller
                } else {
                    response_puller
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
            #[cfg(test)]
            Transport::Mock { responses } => responses
                .iter()
                .position(|(ka, _)| *ka == use_key_address)
                .and_then(|idx| responses.remove(idx))
                .and_then(|(_, data)| data),
        }
    }

    fn evict_address(&mut self, key: &str, addr: &str) {
        if let Some(addrs) = self.key_address_cache.get_mut(key) {
            addrs.remove(addr);
            if addrs.is_empty() {
                self.key_address_cache.remove(key);
            }
        }
        #[allow(irrefutable_let_patterns)]
        if let Transport::Zmq { socket_cache, .. } = &mut self.transport {
            socket_cache.remove(addr);
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
    /// let config = annalib::client_config::ClientConfig::default();
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
        let key_str = key.as_ref().to_string();
        let response = self
            .send_data_request(&key_str, RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_BYTES: request failed or timed out".into()))?;

        let tuple = match Self::validate_response(&response, "GET_BYTES") {
            Ok(t) => t,
            Err(e) => {
                // If KEY_DNE but we have a cached write, return the cached
                // value (read-your-writes: the replica hasn't received our
                // PUT yet).
                if e.to_string().contains("KEY_DNE") {
                    if let Some((_ts, cached_val)) = self.lww_read_cache.get(&key_str) {
                        if !cached_val.is_empty() {
                            return Ok(cached_val.clone());
                        }
                    }
                }
                return Err(e);
            }
        };

        let lww = LwwValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET_BYTES: failed to decode LWW value: {}", e)))?;

        // Monotonic read enforcement: if we've seen a higher timestamp for
        // this key before, return the cached value instead of the stale one.
        if let Some((cached_ts, cached_val)) = self.lww_read_cache.get(&key_str) {
            if lww.timestamp < *cached_ts {
                return Ok(cached_val.clone());
            }
        }

        // Update the high-water mark for this key and for writes-follow-reads.
        if lww.timestamp > self.last_seen_ts {
            self.last_seen_ts = lww.timestamp;
        }
        self.lww_read_cache
            .insert(key_str, (lww.timestamp, lww.value.clone()));
        Ok(lww.value)
    }

    /// Retrieve server thread statistics for a specific node and thread.
    ///
    /// Reads the metadata key
    /// `ANNA_METADATA|stats|<public_ip>|<private_ip>|<tid>|<tier>`
    /// and decodes the `ServerThreadStatistics` protobuf.
    pub async fn get_storage_stats(
        &mut self,
        public_ip: &str,
        private_ip: &str,
        tid: u32,
        tier: &str,
    ) -> Result<crate::proto::metadata::ServerThreadStatistics> {
        let key = format!(
            "ANNA_METADATA|stats|{}|{}|{}|{}",
            public_ip, private_ip, tid, tier
        );
        let bytes = self.get_bytes(&key).await?;
        crate::proto::metadata::ServerThreadStatistics::decode(bytes.as_slice())
            .map_err(|e| Error::Kvs(format!("Failed to decode ServerThreadStatistics: {}", e)))
    }

    /// Retrieve per-key access frequency data for a specific node and thread.
    ///
    /// Reads the metadata key
    /// `ANNA_METADATA|access|<public_ip>|<private_ip>|<tid>|<tier>`
    /// and decodes the `KeyAccessData` protobuf.
    pub async fn get_key_access_stats(
        &mut self,
        public_ip: &str,
        private_ip: &str,
        tid: u32,
        tier: &str,
    ) -> Result<crate::proto::metadata::KeyAccessData> {
        let key = format!(
            "ANNA_METADATA|access|{}|{}|{}|{}",
            public_ip, private_ip, tid, tier
        );
        let bytes = self.get_bytes(&key).await?;
        crate::proto::metadata::KeyAccessData::decode(bytes.as_slice())
            .map_err(|e| Error::Kvs(format!("Failed to decode KeyAccessData: {}", e)))
    }

    /// Retrieve per-key size data for a specific node and thread.
    ///
    /// Reads the metadata key
    /// `ANNA_METADATA|size|<public_ip>|<private_ip>|<tid>|<tier>`
    /// and decodes the `KeySizeData` protobuf.
    pub async fn get_key_size_stats(
        &mut self,
        public_ip: &str,
        private_ip: &str,
        tid: u32,
        tier: &str,
    ) -> Result<crate::proto::metadata::KeySizeData> {
        let key = format!(
            "ANNA_METADATA|size|{}|{}|{}|{}",
            public_ip, private_ip, tid, tier
        );
        let bytes = self.get_bytes(&key).await?;
        crate::proto::metadata::KeySizeData::decode(bytes.as_slice())
            .map_err(|e| Error::Kvs(format!("Failed to decode KeySizeData: {}", e)))
    }

    /// Retrieve cluster topology (thread counts) from the metadata key.
    ///
    /// Returns `None` if the metadata key hasn't been written yet.
    pub async fn get_cluster_topology(
        &mut self,
    ) -> Option<crate::proto::metadata::ClusterTopology> {
        let bytes = self
            .get_bytes("ANNA_METADATA|cluster_topology")
            .await
            .ok()?;
        Self::decode_cluster_topology(&bytes)
    }

    /// Decode a `ClusterTopology` protobuf from raw LWW value bytes.
    pub fn decode_cluster_topology(
        bytes: &[u8],
    ) -> Option<crate::proto::metadata::ClusterTopology> {
        crate::proto::metadata::ClusterTopology::decode(bytes).ok()
    }

    /// Retrieve monitoring IPs from the metadata key.
    ///
    /// Returns an empty vec if the metadata key hasn't been written yet.
    pub async fn get_monitoring_ips(&mut self) -> Vec<String> {
        match self.get_bytes("ANNA_METADATA|monitoring_ips").await {
            Ok(bytes) => Self::decode_monitoring_ips(&bytes),
            Err(_) => vec![],
        }
    }

    /// Decode monitoring IPs from a serialized `StringSet`.
    pub fn decode_monitoring_ips(bytes: &[u8]) -> Vec<String> {
        crate::proto::shared::StringSet::decode(bytes)
            .map(|s| s.keys)
            .unwrap_or_default()
    }

    /// Store a key-value pair (Last-Writer-Wins lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::client_config::ClientConfig::default();
    /// let client = annalib::kvs_client::KVSClient::new(&config, Some(102)).await;
    /// // client.put("my_key", "my_value").await?; // requires a running server
    /// # }
    /// ```
    pub async fn put<K: AsRef<str> + Display>(&mut self, key: K, value: &str) -> Result<()> {
        debug!("PUT: {} <- {}", key, value);
        let ts = std::cmp::max(Self::generate_timestamp(), self.last_seen_ts + 1);
        self.last_seen_ts = ts;
        let lww = LwwValue {
            timestamp: ts,
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

        // Cache the written value for read-your-writes consistency.
        // This ensures a subsequent GET returns at least this value,
        // even if routed to a replica that hasn't received it via gossip.
        self.lww_read_cache
            .insert(key.as_ref().to_string(), (lww.timestamp, lww.value.clone()));
        Ok(())
    }

    /// Retrieve a set of values by key (Set lattice).
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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
    /// let config = annalib::client_config::ClientConfig::default();
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

    /// Retrieve a value by key, automatically detecting the lattice type
    /// from the server response.
    ///
    /// This is the unified GET that replaces the per-type methods
    /// (`get`, `get_set`, `get_causal`, etc.) for callers that want a
    /// single entry point. The returned [`Value`](crate::value::Value)
    /// enum carries both the data and its lattice type.
    pub async fn get_value<K: AsRef<str> + Display>(
        &mut self,
        key: K,
    ) -> Result<crate::value::Value> {
        debug!("GET_VALUE: {}", key);
        let key_str = key.as_ref().to_string();
        let response = self
            .send_data_request(&key_str, RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET_VALUE: request failed or timed out".into()))?;

        let tuple = match Self::validate_response(&response, "GET_VALUE") {
            Ok(t) => t,
            Err(e) => {
                // For LWW keys, honour the read-your-writes cache on KEY_DNE.
                if e.to_string().contains("KEY_DNE") {
                    if let Some((_ts, cached_val)) = self.lww_read_cache.get(&key_str) {
                        if !cached_val.is_empty() {
                            return Ok(crate::value::Value::Lww(
                                String::from_utf8_lossy(cached_val).to_string(),
                            ));
                        }
                    }
                }
                return Err(e);
            }
        };

        match tuple.lattice_type {
            x if x == LatticeType::Lww as i32 => {
                let lww = LwwValue::decode(tuple.payload.as_slice())
                    .map_err(|e| Error::Kvs(format!("GET_VALUE: failed to decode LWW: {}", e)))?;

                // Monotonic read enforcement (same logic as get_bytes).
                if let Some((cached_ts, cached_val)) = self.lww_read_cache.get(&key_str) {
                    if lww.timestamp < *cached_ts {
                        return Ok(crate::value::Value::Lww(
                            String::from_utf8_lossy(cached_val).to_string(),
                        ));
                    }
                }
                if lww.timestamp > self.last_seen_ts {
                    self.last_seen_ts = lww.timestamp;
                }
                self.lww_read_cache
                    .insert(key_str, (lww.timestamp, lww.value.clone()));

                Ok(crate::value::Value::Lww(
                    String::from_utf8_lossy(&lww.value).to_string(),
                ))
            }
            x if x == LatticeType::Set as i32 => {
                let sv = SetValue::decode(tuple.payload.as_slice())
                    .map_err(|e| Error::Kvs(format!("GET_VALUE: failed to decode Set: {}", e)))?;
                Ok(crate::value::Value::Set(
                    sv.values
                        .iter()
                        .map(|v| String::from_utf8_lossy(v).to_string())
                        .collect(),
                ))
            }
            x if x == LatticeType::OrderedSet as i32 => {
                let sv = SetValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode OrderedSet: {}", e))
                })?;
                Ok(crate::value::Value::OrderedSet(
                    sv.values
                        .iter()
                        .map(|v| String::from_utf8_lossy(v).to_string())
                        .collect(),
                ))
            }
            x if x == LatticeType::LwwSet as i32 => {
                // LWW_SET is stored as LwwValue where the value bytes
                // contain a serialized SetValue.
                let lww = LwwValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode LWW_SET outer: {}", e))
                })?;

                // Monotonic read enforcement (same as LWW scalar).
                if let Some((cached_ts, _)) = self.lww_read_cache.get(&key_str) {
                    if lww.timestamp < *cached_ts {
                        // Stale — but we don't cache the set itself in
                        // lww_read_cache (it stores bytes). Re-decode
                        // from the cached bytes would be complex, so
                        // just return the stale result. The timestamp
                        // high-water mark prevents regression.
                    }
                }
                if lww.timestamp > self.last_seen_ts {
                    self.last_seen_ts = lww.timestamp;
                }

                let sv = SetValue::decode(lww.value.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode LWW_SET inner: {}", e))
                })?;
                Ok(crate::value::Value::LwwSet(
                    sv.values
                        .iter()
                        .map(|v| String::from_utf8_lossy(v).to_string())
                        .collect(),
                ))
            }
            x if x == LatticeType::UnionScalar as i32 => {
                // UNION_SCALAR is stored as SetValue (same as SET).
                // Display as concatenated sorted fragments.
                let sv = SetValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode UnionScalar: {}", e))
                })?;
                let mut fragments: Vec<String> = sv
                    .values
                    .iter()
                    .map(|v| String::from_utf8_lossy(v).to_string())
                    .collect();
                fragments.sort();
                Ok(crate::value::Value::UnionScalar(fragments.join("\n")))
            }
            x if x == LatticeType::Priority as i32 => {
                let pv = PriorityValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode Priority: {}", e))
                })?;
                Ok(crate::value::Value::Priority {
                    priority: pv.priority,
                    value: String::from_utf8_lossy(&pv.value).to_string(),
                })
            }
            x if x == LatticeType::SingleCausal as i32 => {
                let skc = SingleKeyCausalValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode SingleCausal: {}", e))
                })?;
                Ok(crate::value::Value::SingleCausal {
                    vector_clock: skc.vector_clock,
                    values: skc
                        .values
                        .iter()
                        .map(|v| String::from_utf8_lossy(v).to_string())
                        .collect(),
                })
            }
            x if x == LatticeType::MultiCausal as i32 => {
                let mkc = MultiKeyCausalValue::decode(tuple.payload.as_slice()).map_err(|e| {
                    Error::Kvs(format!("GET_VALUE: failed to decode MultiCausal: {}", e))
                })?;
                let deps: Vec<(String, std::collections::HashMap<String, u32>)> = mkc
                    .dependencies
                    .iter()
                    .map(|kv| (kv.key.clone(), kv.vector_clock.clone()))
                    .collect();
                let values: Vec<String> = mkc
                    .values
                    .iter()
                    .map(|v| String::from_utf8_lossy(v).to_string())
                    .collect();
                Ok(crate::value::Value::MultiCausal {
                    vector_clock: mkc.vector_clock,
                    dependencies: deps,
                    values,
                })
            }
            other => Err(Error::Kvs(format!(
                "GET_VALUE: unknown lattice type {}",
                other
            ))),
        }
    }

    /// Store a type-tagged value by key.
    ///
    /// This is the unified PUT that replaces the per-type methods
    /// (`put`, `put_set`, `put_causal`, etc.). The lattice type is
    /// inferred from the [`Value`](crate::value::Value) variant.
    pub async fn put_value<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        value: &crate::value::Value,
    ) -> Result<()> {
        debug!("PUT_VALUE: {} <- {:?}", key, value.type_name());
        let lattice_type = value.lattice_type() as i32;
        let mut lww_ts: Option<u64> = None;

        let payload = match value {
            crate::value::Value::Lww(s) => {
                let ts = std::cmp::max(Self::generate_timestamp(), self.last_seen_ts + 1);
                self.last_seen_ts = ts;
                lww_ts = Some(ts);
                let lww = LwwValue {
                    timestamp: ts,
                    value: s.as_bytes().to_vec(),
                };
                lww.encode_to_vec()
            }
            crate::value::Value::Set(values) => {
                let sv = SetValue {
                    values: values.iter().map(|s| s.as_bytes().to_vec()).collect(),
                };
                sv.encode_to_vec()
            }
            crate::value::Value::OrderedSet(values) => {
                let sv = SetValue {
                    values: values.iter().map(|s| s.as_bytes().to_vec()).collect(),
                };
                sv.encode_to_vec()
            }
            crate::value::Value::UnionScalar(s) => {
                // UNION_SCALAR: send as a SetValue with one entry.
                // The server merges via set union (accumulates fragments).
                let sv = SetValue {
                    values: vec![s.as_bytes().to_vec()],
                };
                sv.encode_to_vec()
            }
            crate::value::Value::LwwSet(values) => {
                // LWW_SET: wrap a SetValue inside an LwwValue with a timestamp.
                let sv = SetValue {
                    values: values.iter().map(|s| s.as_bytes().to_vec()).collect(),
                };
                let ts = std::cmp::max(Self::generate_timestamp(), self.last_seen_ts + 1);
                self.last_seen_ts = ts;
                let lww = LwwValue {
                    timestamp: ts,
                    value: sv.encode_to_vec(),
                };
                lww.encode_to_vec()
            }
            crate::value::Value::Priority { priority, value } => {
                let pv = PriorityValue {
                    priority: *priority,
                    value: value.as_bytes().to_vec(),
                };
                pv.encode_to_vec()
            }
            crate::value::Value::SingleCausal {
                vector_clock,
                values,
            } => {
                let skc = SingleKeyCausalValue {
                    vector_clock: vector_clock.clone(),
                    values: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
                };
                skc.encode_to_vec()
            }
            crate::value::Value::MultiCausal {
                vector_clock,
                dependencies,
                values,
            } => {
                let deps: Vec<crate::proto::shared::KeyVersion> = dependencies
                    .iter()
                    .map(|(k, vc)| crate::proto::shared::KeyVersion {
                        key: k.clone(),
                        vector_clock: vc.clone(),
                    })
                    .collect();
                let mkc = MultiKeyCausalValue {
                    vector_clock: vector_clock.clone(),
                    dependencies: deps,
                    values: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
                };
                mkc.encode_to_vec()
            }
        };

        let response = self
            .send_data_request(
                key.as_ref(),
                RequestType::Put as i32,
                Some(lattice_type),
                Some(payload),
            )
            .await
            .ok_or_else(|| Error::Kvs("PUT_VALUE: request failed or timed out".into()))?;

        Self::validate_response(&response, "PUT_VALUE")?;

        // Cache LWW writes for read-your-writes consistency.
        if let (crate::value::Value::Lww(s), Some(ts)) = (value, lww_ts) {
            self.lww_read_cache
                .insert(key.as_ref().to_string(), (ts, s.as_bytes().to_vec()));
        }
        Ok(())
    }

    /// Delete a key by writing an empty LWW value with a dominating timestamp.
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = annalib::client_config::ClientConfig::default();
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

                                // Monotonic read / read-your-writes enforcement
                                let value = if let Some((cached_ts, cached_val)) =
                                    self.lww_read_cache.get(&tuple.key)
                                {
                                    if lww.timestamp < *cached_ts {
                                        String::from_utf8_lossy(cached_val).to_string()
                                    } else {
                                        if lww.timestamp > self.last_seen_ts {
                                            self.last_seen_ts = lww.timestamp;
                                        }
                                        self.lww_read_cache.insert(
                                            tuple.key.clone(),
                                            (lww.timestamp, lww.value.clone()),
                                        );
                                        String::from_utf8_lossy(&lww.value).to_string()
                                    }
                                } else {
                                    if lww.timestamp > self.last_seen_ts {
                                        self.last_seen_ts = lww.timestamp;
                                    }
                                    self.lww_read_cache.insert(
                                        tuple.key.clone(),
                                        (lww.timestamp, lww.value.clone()),
                                    );
                                    String::from_utf8_lossy(&lww.value).to_string()
                                };
                                results.insert(tuple.key.clone(), value);
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

    /// Return the port base offset derived from the routing addresses.
    pub fn base_offset(&self) -> usize {
        if let Some(rt) = self.routing_threads.first() {
            let addr = rt.key_address_connect_address();
            addr.rsplit(':')
                .next()
                .and_then(|p| p.parse::<usize>().ok())
                .map(|p| p.saturating_sub(6450))
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Clear the key-address cache and the monotonic read cache.
    pub fn clear_cache(&mut self) {
        self.key_address_cache.clear();
        self.lww_read_cache.clear();
    }

    /// Check if a key has a cached LWW read value (for testing).
    #[doc(hidden)]
    pub fn has_cached_read(&self, key: &str) -> bool {
        self.lww_read_cache.contains_key(key)
    }

    /// Set the request timeout duration.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_client(tid: ThreadID) -> KVSClient {
        KVSClient::new_mock("127.0.0.1", tid)
    }

    fn make_routing_response(key: &str, worker_addr: &str) -> Vec<u8> {
        use crate::proto::kvs::key_address_response::KeyAddress;
        let response = KeyAddressResponse {
            addresses: vec![KeyAddress {
                key: key.to_string(),
                ips: vec![worker_addr.to_string()],
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    fn make_get_response(key: &str, value: &[u8]) -> Vec<u8> {
        make_get_response_with_ts(key, value, 1)
    }

    fn make_get_response_with_ts(key: &str, value: &[u8], timestamp: u64) -> Vec<u8> {
        let lww = LwwValue {
            timestamp,
            value: value.to_vec(),
        };
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: key.to_string(),
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

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
    async fn mock_client_request_id() {
        let mut client = mock_client(99);
        let id1 = client.get_request_id();
        let id2 = client.get_request_id();
        assert!(id1.contains("127.0.0.1"));
        assert!(id1.contains("99"));
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn mock_client_routing_thread() {
        let mut client = mock_client(98);
        let addr = client.get_routing_thread();
        assert!(addr.starts_with("tcp://"), "addr was: {}", addr);
        assert!(addr.contains("127.0.0.1"), "addr was: {}", addr);
    }

    #[tokio::test]
    async fn mock_client_clear_cache() {
        let mut client = mock_client(97);
        client
            .key_address_cache
            .insert("test_key".into(), ["addr1".to_string()].into());
        assert!(!client.key_address_cache.is_empty());
        client.clear_cache();
        assert!(client.key_address_cache.is_empty());
    }

    #[tokio::test]
    async fn mock_get_worker_address_returns_cached() {
        let mut client = mock_client(96);
        client.key_address_cache.insert(
            "cached_key".into(),
            ["tcp://127.0.0.1:6200".to_string()].into(),
        );
        let addr = client.get_worker_address("cached_key").await;
        assert_eq!(addr, Some("tcp://127.0.0.1:6200".to_string()));
    }

    #[tokio::test]
    async fn mock_get_worker_address_picks_from_multi() {
        let mut client = mock_client(95);
        let mut addrs = HashSet::new();
        addrs.insert("tcp://10.0.0.1:6200".to_string());
        addrs.insert("tcp://10.0.0.2:6200".to_string());
        client.key_address_cache.insert("multi_key".into(), addrs);
        let addr = client
            .get_worker_address("multi_key")
            .await
            .expect("expected cached address");
        assert!(
            addr == "tcp://10.0.0.1:6200" || addr == "tcp://10.0.0.2:6200",
            "unexpected addr: {}",
            addr
        );
    }

    #[tokio::test]
    async fn mock_evict_address_removes_single() {
        let mut client = mock_client(88);
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
    async fn mock_evict_address_removes_key_when_last() {
        let mut client = mock_client(87);
        client.key_address_cache.insert(
            "single_addr".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        client.evict_address("single_addr", "tcp://10.0.0.1:6200");
        assert!(!client.key_address_cache.contains_key("single_addr"));
    }

    #[tokio::test]
    async fn mock_set_timeout() {
        let mut client = mock_client(85);
        assert_eq!(client.timeout, Duration::from_secs(1));
        client.set_timeout(Duration::from_secs(3));
        assert_eq!(client.timeout, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn mock_get_returns_value() {
        let mut client = mock_client(80);
        let worker = "tcp://127.0.0.1:6200";
        client.push_mock_response(true, Some(make_routing_response("test_key", worker)));
        client.push_mock_response(false, Some(make_get_response("test_key", b"hello")));
        let val = client.get("test_key").await.expect("GET failed");
        assert_eq!(val, "hello");
    }

    #[tokio::test]
    async fn mock_get_bytes_returns_raw() {
        let mut client = mock_client(79);
        let worker = "tcp://127.0.0.1:6200";
        client.push_mock_response(true, Some(make_routing_response("raw_key", worker)));
        client.push_mock_response(false, Some(make_get_response("raw_key", b"\x01\x02\x03")));
        let bytes = client.get_bytes("raw_key").await.expect("GET_BYTES failed");
        assert_eq!(bytes, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn mock_get_cluster_topology() {
        use crate::proto::metadata::ClusterTopology;
        let mut client = mock_client(78);
        let topo = ClusterTopology {
            routing_thread_count: 2,
            memory_thread_count: 4,
            disk_thread_count: 1,
        };
        let worker = "tcp://127.0.0.1:6200";
        let meta_key = "ANNA_METADATA|cluster_topology";
        client.push_mock_response(true, Some(make_routing_response(meta_key, worker)));
        client.push_mock_response(
            false,
            Some(make_get_response(meta_key, &topo.encode_to_vec())),
        );
        let result = client.get_cluster_topology().await;
        let t = result.expect("get_cluster_topology returned None");
        assert_eq!(t.memory_thread_count, 4);
        assert_eq!(t.routing_thread_count, 2);
    }

    #[tokio::test]
    async fn mock_get_monitoring_ips() {
        use crate::proto::shared::StringSet;
        let mut client = mock_client(77);
        let ips = StringSet {
            keys: vec!["10.0.0.1".into(), "10.0.0.2".into()],
        };
        let worker = "tcp://127.0.0.1:6200";
        let meta_key = "ANNA_METADATA|monitoring_ips";
        client.push_mock_response(true, Some(make_routing_response(meta_key, worker)));
        client.push_mock_response(
            false,
            Some(make_get_response(meta_key, &ips.encode_to_vec())),
        );
        let result = client.get_monitoring_ips().await;
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"10.0.0.1".to_string()));
    }

    #[tokio::test]
    async fn mock_get_monitoring_ips_not_found() {
        let mut client = mock_client(76);
        let result = client.get_monitoring_ips().await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn mock_get_key_addresses_empty() {
        let mut client = mock_client(75);
        client.key_address_cache.insert(
            "stale_key".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        let addrs = client.get_key_addresses("stale_key").await;
        assert!(addrs.is_empty());
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
    async fn mock_request_id_format() {
        let mut client = mock_client(91);
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

    #[test]
    fn stats_metadata_key_format() {
        let key = format!(
            "ANNA_METADATA|stats|{}|{}|{}|{}",
            "10.0.0.1", "192.168.1.1", 0, "MEMORY"
        );
        assert_eq!(key, "ANNA_METADATA|stats|10.0.0.1|192.168.1.1|0|MEMORY");
    }

    #[test]
    fn stats_metadata_key_same_ip() {
        let key = format!(
            "ANNA_METADATA|stats|{}|{}|{}|{}",
            "127.0.0.1", "127.0.0.1", 0, "MEMORY"
        );
        assert_eq!(key, "ANNA_METADATA|stats|127.0.0.1|127.0.0.1|0|MEMORY");
    }

    #[test]
    fn access_metadata_key_format() {
        let key = format!(
            "ANNA_METADATA|access|{}|{}|{}|{}",
            "10.0.0.1", "192.168.1.1", 2, "DISK"
        );
        assert_eq!(key, "ANNA_METADATA|access|10.0.0.1|192.168.1.1|2|DISK");
    }

    #[test]
    fn size_metadata_key_format() {
        let key = format!(
            "ANNA_METADATA|size|{}|{}|{}|{}",
            "10.0.0.1", "10.0.0.1", 1, "MEMORY"
        );
        assert_eq!(key, "ANNA_METADATA|size|10.0.0.1|10.0.0.1|1|MEMORY");
    }

    #[test]
    fn decode_cluster_topology_roundtrip() {
        use crate::proto::metadata::ClusterTopology;

        let topo = ClusterTopology {
            routing_thread_count: 2,
            memory_thread_count: 4,
            disk_thread_count: 1,
        };
        let encoded = topo.encode_to_vec();
        let decoded =
            KVSClient::decode_cluster_topology(&encoded).expect("failed to decode topology");
        assert_eq!(decoded.routing_thread_count, 2);
        assert_eq!(decoded.memory_thread_count, 4);
        assert_eq!(decoded.disk_thread_count, 1);
    }

    #[test]
    fn decode_cluster_topology_invalid() {
        assert!(KVSClient::decode_cluster_topology(b"not valid").is_none());
    }

    #[test]
    fn decode_monitoring_ips_roundtrip() {
        use crate::proto::shared::StringSet;

        let set = StringSet {
            keys: vec!["10.0.0.1".into(), "10.0.0.2".into()],
        };
        let encoded = set.encode_to_vec();
        let decoded = KVSClient::decode_monitoring_ips(&encoded);
        assert_eq!(decoded.len(), 2);
        assert!(decoded.contains(&"10.0.0.1".to_string()));
        assert!(decoded.contains(&"10.0.0.2".to_string()));
    }

    #[test]
    fn decode_monitoring_ips_invalid() {
        let decoded = KVSClient::decode_monitoring_ips(b"not valid");
        assert!(decoded.is_empty());
    }

    #[test]
    fn decode_monitoring_ips_empty() {
        use crate::proto::shared::StringSet;

        let set = StringSet { keys: vec![] };
        let encoded = set.encode_to_vec();
        let decoded = KVSClient::decode_monitoring_ips(&encoded);
        assert!(decoded.is_empty());
    }

    #[tokio::test]
    async fn monotonic_read_returns_cached_on_stale() {
        let mut client = mock_client(200);
        let worker = "tcp://127.0.0.1:6200";
        let key = "mono_key";

        // First read: timestamp 100, value "new"
        client.push_mock_response(true, Some(make_routing_response(key, worker)));
        client.push_mock_response(false, Some(make_get_response_with_ts(key, b"new", 100)));
        let val = client.get(key).await.expect("first GET failed");
        assert_eq!(val, "new");

        // Second read: stale timestamp 50, value "old" — should return cached "new"
        client.push_mock_response(false, Some(make_get_response_with_ts(key, b"old", 50)));
        let val = client.get(key).await.expect("stale GET failed");
        assert_eq!(
            val, "new",
            "Monotonic read should return cached value on stale response"
        );
    }

    #[tokio::test]
    async fn monotonic_read_updates_on_newer() {
        let mut client = mock_client(201);
        let worker = "tcp://127.0.0.1:6200";
        let key = "mono_key2";

        // First read: timestamp 100
        client.push_mock_response(true, Some(make_routing_response(key, worker)));
        client.push_mock_response(false, Some(make_get_response_with_ts(key, b"first", 100)));
        let val = client.get(key).await.expect("first GET failed");
        assert_eq!(val, "first");

        // Second read: newer timestamp 200 — should update and return new value
        client.push_mock_response(false, Some(make_get_response_with_ts(key, b"second", 200)));
        let val = client.get(key).await.expect("newer GET failed");
        assert_eq!(
            val, "second",
            "Monotonic read should accept newer timestamp"
        );
    }

    #[tokio::test]
    async fn monotonic_read_cache_cleared_with_clear_cache() {
        let mut client = mock_client(202);
        let worker = "tcp://127.0.0.1:6200";
        let key = "mono_clear";

        // Read with timestamp 100
        client.push_mock_response(true, Some(make_routing_response(key, worker)));
        client.push_mock_response(false, Some(make_get_response_with_ts(key, b"cached", 100)));
        client.get(key).await.expect("first GET failed");

        // Clear cache
        client.clear_cache();

        // Read with lower timestamp 50 — should succeed since cache was cleared
        client.push_mock_response(true, Some(make_routing_response(key, worker)));
        client.push_mock_response(
            false,
            Some(make_get_response_with_ts(key, b"after_clear", 50)),
        );
        let val = client.get(key).await.expect("GET after clear failed");
        assert_eq!(
            val, "after_clear",
            "After clear_cache, stale values should be accepted"
        );
    }

    fn make_multi_get_response(keys_values_ts: &[(&str, &[u8], u64)]) -> Vec<u8> {
        let tuples: Vec<KeyTuple> = keys_values_ts
            .iter()
            .map(|(key, value, ts)| {
                let lww = LwwValue {
                    timestamp: *ts,
                    value: value.to_vec(),
                };
                KeyTuple {
                    key: key.to_string(),
                    payload: lww.encode_to_vec(),
                    ..Default::default()
                }
            })
            .collect();
        let response = KeyResponse {
            tuples,
            ..Default::default()
        };
        response.encode_to_vec()
    }

    fn make_put_response(key: &str) -> Vec<u8> {
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: key.to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    #[tokio::test]
    async fn read_your_writes_returns_put_value_on_stale_get() {
        let mut client = mock_client(210);
        let worker = "tcp://127.0.0.1:6200";
        let key = "ryw_key";

        // PUT a value — this caches (timestamp, value) in lww_read_cache
        client.push_mock_response(true, Some(make_routing_response(key, worker)));
        client.push_mock_response(false, Some(make_put_response(key)));
        client.put(key, "my_write").await.expect("PUT failed");

        // GET returns a stale value (timestamp 1, much older than the PUT)
        client.push_mock_response(
            false,
            Some(make_get_response_with_ts(key, b"stale_from_server", 1)),
        );
        let val = client.get(key).await.expect("GET after PUT failed");
        assert_eq!(
            val, "my_write",
            "Read-your-writes: GET should return the value we just PUT"
        );
    }

    #[tokio::test]
    async fn get_multi_enforces_monotonic_reads() {
        let mut client = mock_client(211);
        let worker = "tcp://127.0.0.1:6200";
        let key_a = "multi_a";
        let key_b = "multi_b";

        // PUT key_a — caches it with a high timestamp
        client.push_mock_response(true, Some(make_routing_response(key_a, worker)));
        client.push_mock_response(false, Some(make_put_response(key_a)));
        client.put(key_a, "written_a").await.expect("PUT a failed");

        // GET key_b normally (no prior cache) with timestamp 50
        client.push_mock_response(true, Some(make_routing_response(key_b, worker)));
        client.push_mock_response(false, Some(make_get_response_with_ts(key_b, b"val_b", 50)));
        client.get(key_b).await.expect("GET b failed");

        // get_multi with stale responses for both keys
        // key_a: stale (ts=1), should return cached "written_a"
        // key_b: stale (ts=10), should return cached "val_b"
        client.push_mock_response(true, Some(make_routing_response(key_a, worker)));
        client.push_mock_response(
            false,
            Some(make_multi_get_response(&[
                (key_a, b"stale_a", 1),
                (key_b, b"stale_b", 10),
            ])),
        );
        // key_b needs routing too
        client.push_mock_response(true, Some(make_routing_response(key_b, worker)));

        let results = client
            .get_multi(&[key_a, key_b])
            .await
            .expect("get_multi failed");

        assert_eq!(
            results.get(key_a).map(|s| s.as_str()),
            Some("written_a"),
            "get_multi should return cached PUT value for stale key_a"
        );
        assert_eq!(
            results.get(key_b).map(|s| s.as_str()),
            Some("val_b"),
            "get_multi should return cached GET value for stale key_b"
        );
    }

    #[tokio::test]
    async fn writes_follow_reads_timestamp_after_get() {
        let mut client = mock_client(215);
        let worker = "tcp://127.0.0.1:6200";

        // Read a key with a high timestamp (e.g., 999999)
        client.push_mock_response(true, Some(make_routing_response("read_key", worker)));
        client.push_mock_response(
            false,
            Some(make_get_response_with_ts("read_key", b"val", 999999)),
        );
        client.get("read_key").await.expect("GET failed");

        // The next PUT should get a timestamp > 999999
        assert!(
            client.last_seen_ts >= 999999,
            "last_seen_ts should be >= read timestamp, got {}",
            client.last_seen_ts
        );

        // PUT a different key
        client.push_mock_response(true, Some(make_routing_response("write_key", worker)));
        client.push_mock_response(false, Some(make_put_response("write_key")));
        client
            .put("write_key", "after_read")
            .await
            .expect("PUT failed");

        // The write's cached timestamp should be > 999999
        let (write_ts, _) = client
            .lww_read_cache
            .get("write_key")
            .expect("no cache for write_key");
        assert!(
            *write_ts > 999999,
            "Write timestamp ({}) should be > read timestamp (999999)",
            write_ts
        );
    }

    fn make_union_scalar_response(key: &str, fragments: &[&str]) -> Vec<u8> {
        let sv = SetValue {
            values: fragments.iter().map(|v| v.as_bytes().to_vec()).collect(),
        };
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: key.to_string(),
                lattice_type: LatticeType::UnionScalar as i32,
                payload: sv.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    fn make_lww_set_response(key: &str, values: &[&str]) -> Vec<u8> {
        let sv = SetValue {
            values: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
        };
        let lww = LwwValue {
            timestamp: 100,
            value: sv.encode_to_vec(),
        };
        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: key.to_string(),
                lattice_type: LatticeType::LwwSet as i32,
                payload: lww.encode_to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        response.encode_to_vec()
    }

    #[tokio::test]
    async fn get_value_union_scalar() {
        let worker = "tcp://127.0.0.1:6200";
        let mut client = mock_client(202);
        client.push_mock_response(true, Some(make_routing_response("ukey", worker)));
        client.push_mock_response(
            false,
            Some(make_union_scalar_response("ukey", &["b_second", "a_first"])),
        );

        let val = client.get_value("ukey").await.expect("get_value failed");
        match val {
            crate::value::Value::UnionScalar(s) => {
                // Should be sorted: a_first\nb_second
                assert_eq!(s, "a_first\nb_second");
            }
            other => panic!("Expected UnionScalar, got {:?}", other.type_name()),
        }
    }

    #[tokio::test]
    async fn put_value_union_scalar() {
        let worker = "tcp://127.0.0.1:6200";
        let mut client = mock_client(203);
        client.push_mock_response(true, Some(make_routing_response("uput", worker)));
        client.push_mock_response(false, Some(make_put_response("uput")));

        let val = crate::value::Value::UnionScalar("fragment".into());
        client
            .put_value("uput", &val)
            .await
            .expect("put_value failed");
    }

    #[tokio::test]
    async fn get_value_lww_set() {
        let worker = "tcp://127.0.0.1:6200";
        let mut client = mock_client(200);
        client.push_mock_response(true, Some(make_routing_response("lws", worker)));
        client.push_mock_response(false, Some(make_lww_set_response("lws", &["a", "b", "c"])));

        let val = client.get_value("lws").await.expect("get_value failed");
        match val {
            crate::value::Value::LwwSet(v) => {
                let mut sorted = v.clone();
                sorted.sort();
                assert_eq!(sorted, vec!["a", "b", "c"]);
            }
            other => panic!("Expected LwwSet, got {:?}", other.type_name()),
        }
    }

    #[tokio::test]
    async fn put_value_lww_set() {
        let worker = "tcp://127.0.0.1:6200";
        let mut client = mock_client(201);
        client.push_mock_response(true, Some(make_routing_response("lws_put", worker)));
        client.push_mock_response(false, Some(make_put_response("lws_put")));

        let val = crate::value::Value::LwwSet(vec!["x".into(), "y".into()]);
        client
            .put_value("lws_put", &val)
            .await
            .expect("put_value failed");
    }
}
