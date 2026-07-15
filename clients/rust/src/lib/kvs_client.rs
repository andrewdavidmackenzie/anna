use crate::config::Config;
use crate::errors::{Error, Result};
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
use zeromq::{PullSocket, PushSocket, Socket, SocketRecv, SocketSend};

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
    key_address_cache: HashMap<Key, HashSet<Address>>,
    timeout: Duration,
    socket_cache: HashMap<Address, PushSocket>,
    key_address_puller: PullSocket,
    response_puller: PullSocket,
}

impl KVSClient {
    /// Create a new `KVSClient` from a `Config` and optional thread id.
    pub async fn new(config: &Config, tid: Option<ThreadID>) -> Self {
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

        let ut = UserThread::new(config.get_user_ip(), tid);

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
        Ok(self.socket_cache.get_mut(addr).expect("socket just inserted"))
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
        let mut request = KeyAddressRequest::default();
        request.request_id = self.get_request_id();
        request.response_address = self.ut.key_address_connect_address();
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
        if !self.key_address_cache.contains_key(key)
            || self.key_address_cache[key].is_empty()
        {
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
        let worker = self.get_worker_address(key).await?;

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
        if self.send_request(&encoded, &worker).await.is_err() { return None; }

        match self.recv_response(false).await {
            Some(data) => match KeyResponse::decode(data.as_slice()) {
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
            },
            None => {
                warn!("Request timed out for key {}", key);
                self.key_address_cache.remove(key);
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

    /// Perform a blocking GET for a LWW key, returning the value as a String.
    pub async fn get<K: AsRef<str> + Display>(&mut self, key: K) -> Result<String> {
        debug!("GET: {}", key);
        let response = self
            .send_data_request(key.as_ref(), RequestType::Get as i32, None, None)
            .await
            .ok_or_else(|| Error::Kvs("GET: request failed or timed out".into()))?;

        let tuple = Self::validate_response(&response, "GET")?;

        let lww = LwwValue::decode(tuple.payload.as_slice())
            .map_err(|e| Error::Kvs(format!("GET: failed to decode LWW value: {}", e)))?;
        Ok(String::from_utf8_lossy(&lww.value).to_string())
    }

    /// Perform a blocking PUT of a LWW key-value pair.
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

    /// Perform a blocking GET for a Set key, returning the values.
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

    /// Perform a blocking PUT of a Set key with the given values.
    #[cfg(feature = "set")]
    pub async fn put_set<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        set: &[&str],
    ) -> Result<()> {
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

    /// Perform a blocking causal GET (not yet implemented).
    #[cfg(feature = "causal")]
    pub async fn get_causal<K: AsRef<str> + Display>(&mut self, key: K) -> Result<String> {
        debug!("GET_CAUSAL: {}", key);
        Err(Error::Kvs("Causal GET is not yet implemented".into()))
    }

    /// Perform a blocking causal PUT (not yet implemented).
    #[cfg(feature = "causal")]
    pub async fn put_causal<K: AsRef<str> + Display>(
        &mut self,
        key: K,
        value: &str,
    ) -> Result<()> {
        debug!("PUT_CAUSAL: {} <- {}", key, value);
        Err(Error::Kvs("Causal PUT is not yet implemented".into()))
    }

    /// Clear the key-address cache.
    pub fn clear_cache(&mut self) {
        self.key_address_cache.clear()
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
        let decoded = LwwValue::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.value, b"hello world");
    }

    #[test]
    fn set_value_roundtrip() {
        let original = SetValue {
            values: vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
        };
        let encoded = original.encode_to_vec();
        let decoded = SetValue::decode(encoded.as_slice()).unwrap();
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
        .unwrap();
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
        .unwrap();
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
        .unwrap();
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
        .unwrap();
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
        .unwrap();
        let mut client = KVSClient::new(&config, Some(95)).await;
        let mut addrs = HashSet::new();
        addrs.insert("tcp://10.0.0.1:6200".to_string());
        addrs.insert("tcp://10.0.0.2:6200".to_string());
        client.key_address_cache.insert("multi_key".into(), addrs);
        let addr = client.get_worker_address("multi_key").await.unwrap();
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
        .unwrap();
        let mut client = KVSClient::new(&config, Some(92)).await;
        client.key_address_cache.insert(
            "evict_me".into(),
            ["tcp://10.0.0.1:6200".to_string()].into(),
        );
        assert!(client.key_address_cache.contains_key("evict_me"));
        client.key_address_cache.remove("evict_me");
        assert!(!client.key_address_cache.contains_key("evict_me"));
    }

    #[test]
    fn empty_set_value_roundtrip() {
        let original = SetValue { values: vec![] };
        let encoded = original.encode_to_vec();
        let decoded = SetValue::decode(encoded.as_slice()).unwrap();
        assert!(decoded.values.is_empty());
    }

    #[test]
    fn lww_value_with_empty_payload() {
        let original = LwwValue {
            timestamp: 0,
            value: vec![],
        };
        let encoded = original.encode_to_vec();
        let decoded = LwwValue::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded.timestamp, 0);
        assert!(decoded.value.is_empty());
    }

    #[tokio::test]
    async fn request_id_format() {
        let config = Config::read(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("default-config.yml"),
        )
        .unwrap();
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
        assert!(result.unwrap_err().to_string().contains("no tuples"));
    }

    #[test]
    fn validate_response_error_code() {
        let mut response = KeyResponse::default();
        let mut tuple = KeyTuple::default();
        tuple.error = AnnaError::KeyDne as i32;
        response.tuples.push(tuple);
        let result = KVSClient::validate_response(&response, "TEST");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("KEY_DNE"));
    }

    #[test]
    fn validate_response_success() {
        let mut response = KeyResponse::default();
        let mut tuple = KeyTuple::default();
        tuple.error = AnnaError::NoError as i32;
        tuple.key = "mykey".into();
        response.tuples.push(tuple);
        let result = KVSClient::validate_response(&response, "TEST");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "mykey");
    }

    #[test]
    fn key_request_protobuf_roundtrip() {
        let mut request = KeyRequest::default();
        request.request_id = "test:0_1".to_string();
        request.response_address = "tcp://127.0.0.1:6800".to_string();
        request.r#type = RequestType::Put as i32;

        let mut tuple = KeyTuple::default();
        tuple.key = "mykey".to_string();
        tuple.lattice_type = LatticeType::Lww as i32;
        let lww = LwwValue {
            timestamp: 100,
            value: b"test".to_vec(),
        };
        tuple.payload = lww.encode_to_vec();
        request.tuples.push(tuple);

        let encoded = request.encode_to_vec();
        let decoded = KeyRequest::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded.request_id, "test:0_1");
        assert_eq!(decoded.tuples[0].key, "mykey");
        assert_eq!(decoded.tuples[0].lattice_type, LatticeType::Lww as i32);
    }

    #[test]
    fn key_address_request_protobuf_roundtrip() {
        let mut request = KeyAddressRequest::default();
        request.request_id = "addr_req_1".to_string();
        request.response_address = "tcp://127.0.0.1:6850".to_string();
        request.keys.push("lookup_key".to_string());

        let encoded = request.encode_to_vec();
        let decoded = KeyAddressRequest::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded.request_id, "addr_req_1");
        assert_eq!(decoded.keys[0], "lookup_key");
    }
}
