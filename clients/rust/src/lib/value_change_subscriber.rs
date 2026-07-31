use crate::client_config::ClientConfig;
use crate::errors::{Error, Result};
use crate::proto::kvs::{KeyResponse, LwwValue};
use crate::proto::shared::StringSet;
use crate::types::{Address, Key};
use log::{debug, info};
use omq_tokio::{Context, Message as ZmqMessage, Options, Socket as OmqSocket, SocketType};
use prost::Message;
use std::collections::HashMap;
use std::time::Duration;

// The port on which KVS servers listen for direct cache registration.
const K_CACHE_REGISTRATION_PORT: usize = 7200;

// The port on which cache nodes receive updates from the KVS.
const K_CACHE_UPDATE_PORT: usize = 7150;

/// Subscribes to value changes for specific keys via the KVS gossip mechanism.
///
/// Registers with KVS server threads to watch specific keys. When those keys
/// are updated (including deletes), the KVS pushes the new values during its
/// gossip epoch. Applications can use this for caching, event-driven updates,
/// or any pub-sub pattern over KVS keys.
///
/// # Example
///
/// ```rust,no_run
/// # #[tokio::main]
/// # async fn main() -> annalib::Result<()> {
/// use std::time::Duration;
/// use annalib::client_config::ClientConfig;
/// use annalib::value_change_subscriber::ValueChangeSubscriber;
///
/// let config = ClientConfig::default();
/// let mut cache = ValueChangeSubscriber::new(&config, None).await?;
/// cache.watch(&["my-key".to_string()]).await?;
///
/// // After a gossip epoch, receive updates pushed from KVS
/// if let Some((key, value)) = cache.recv_update(Duration::from_secs(15)).await? {
///     println!("Got update for {}: {} bytes", key, value.len());
/// }
///
/// // Read from local cache
/// if let Some(value) = cache.get_cached("my-key") {
///     println!("Cached value: {} bytes", value.len());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ValueChangeSubscriber {
    ctx: Context,
    cache_ip: Address,
    base_offset: usize,
    server_ip: Address,
    memory_threads: usize,
    socket_cache: HashMap<Address, OmqSocket>,
    update_puller: OmqSocket,
    local_cache: HashMap<Key, Vec<u8>>,
    watched_keys: Vec<Key>,
}

impl ValueChangeSubscriber {
    /// Create a new cache client.
    ///
    /// The `tid` parameter selects which port to listen on for updates.
    /// Pass `None` for the default (tid=0).
    ///
    /// Memory thread count defaults to 1. Use
    /// [`KVSClient::get_cluster_topology()`] to discover the actual count
    /// at runtime, then pass it to [`Self::with_memory_threads()`].
    pub async fn new(config: &ClientConfig, tid: Option<usize>) -> Result<Self> {
        Self::with_memory_threads(config, tid, 1).await
    }

    /// Create a cache client with an explicit memory thread count.
    ///
    /// Use this when you've queried the cluster topology and know the
    /// actual number of memory threads per KVS node.
    pub async fn with_memory_threads(
        config: &ClientConfig,
        tid: Option<usize>,
        memory_threads: usize,
    ) -> Result<Self> {
        let tid = tid.unwrap_or(0);
        let base_offset = config.base_offset();
        let cache_ip = config.client_ip.clone();
        let server_ip = config.routing_ip().unwrap_or("127.0.0.1").to_string();

        let bind_addr = format!(
            "tcp://{}:{}",
            cache_ip,
            tid + K_CACHE_UPDATE_PORT + base_offset
        );
        let ctx = Context::new();
        let update_puller = ctx.socket(SocketType::Pull, Options::default());
        update_puller
            .bind(bind_addr.parse().map_err(|e| {
                Error::Kvs(format!(
                    "Failed to parse cache update bind address {}: {}",
                    bind_addr, e
                ))
            })?)
            .await
            .map_err(|e| {
                Error::Kvs(format!(
                    "Failed to bind cache update puller on {}: {}",
                    bind_addr, e
                ))
            })?;

        info!("Cache client listening for updates on {}", bind_addr);

        Ok(ValueChangeSubscriber {
            ctx,
            cache_ip,
            base_offset,
            server_ip,
            memory_threads,
            socket_cache: HashMap::new(),
            update_puller,
            local_cache: HashMap::new(),
            watched_keys: Vec::new(),
        })
    }

    /// Register interest in the given keys with all KVS server threads.
    ///
    /// The registration message is sent to each KVS thread's cache registration
    /// port. The server will then include these keys in its gossip-to-caches
    /// during each gossip epoch.
    pub async fn watch(&mut self, keys: &[Key]) -> Result<()> {
        self.watched_keys.extend(keys.iter().cloned());

        let mut msg = StringSet::default();
        msg.keys.push(self.cache_ip.clone());
        for key in keys {
            msg.keys.push(key.clone());
        }
        let payload = msg.encode_to_vec();

        for tid in 0..self.memory_threads {
            let addr = format!(
                "tcp://{}:{}",
                self.server_ip,
                tid + K_CACHE_REGISTRATION_PORT + self.base_offset
            );
            let socket = self.get_or_connect(&addr).await?;
            socket
                .send(ZmqMessage::from(payload.clone()))
                .await
                .map_err(|e| {
                    Error::Kvs(format!("Failed to send registration to {}: {}", addr, e))
                })?;
            debug!(
                "Registered {} keys with KVS thread {} at {}",
                keys.len(),
                tid,
                addr
            );
        }

        info!(
            "Registered {} keys with {} KVS threads",
            keys.len(),
            self.memory_threads
        );
        Ok(())
    }

    /// Receive the next update pushed from the KVS.
    ///
    /// Blocks up to `timeout` waiting for a gossip push. Returns `None` if
    /// the timeout expires without receiving an update.
    ///
    /// Updates are `KeyResponse` protobuf messages containing the key and
    /// its new serialized value.
    pub async fn recv_update(&mut self, timeout: Duration) -> Result<Option<(Key, Vec<u8>)>> {
        let result = tokio::time::timeout(timeout, self.update_puller.recv()).await;

        match result {
            Ok(Ok(msg)) => {
                let bytes: Vec<u8> = msg.iter().flat_map(|frame| frame.to_vec()).collect();

                let response = KeyResponse::decode(bytes.as_slice())
                    .map_err(|e| Error::Kvs(format!("Failed to decode cache update: {}", e)))?;

                for tuple in &response.tuples {
                    let key = tuple.key.clone();
                    let payload = tuple.payload.clone();

                    if !payload.is_empty() {
                        self.local_cache.insert(key.clone(), payload.clone());
                        debug!("Cache updated for key: {}", key);
                        return Ok(Some((key, payload)));
                    }
                }

                Ok(None)
            }
            Ok(Err(e)) => Err(Error::Kvs(format!("ZMQ recv error: {}", e))),
            Err(_) => Ok(None),
        }
    }

    /// Read a value from the local cache.
    ///
    /// Returns `None` if the key has not been received via a gossip update yet.
    pub fn get_cached(&self, key: &str) -> Option<&Vec<u8>> {
        self.local_cache.get(key)
    }

    /// Decode a cached LWW payload into its string value.
    ///
    /// The raw cache payload is a serialized `LwwValue` protobuf. This helper
    /// decodes it and returns the inner value bytes.
    pub fn decode_lww_value(payload: &[u8]) -> Result<Vec<u8>> {
        let lww = LwwValue::decode(payload)
            .map_err(|e| Error::Kvs(format!("Failed to decode LWW value: {}", e)))?;
        Ok(lww.value)
    }

    /// Return the list of currently watched keys.
    pub fn watched_keys(&self) -> &[Key] {
        &self.watched_keys
    }

    async fn get_or_connect(&mut self, addr: &str) -> Result<&mut OmqSocket> {
        if !self.socket_cache.contains_key(addr) {
            let mut last_err = None;
            for attempt in 0..5 {
                let sock = self.ctx.socket(SocketType::Push, Options::default());
                let endpoint = addr
                    .parse()
                    .map_err(|e| Error::Kvs(format!("Invalid address {}: {}", addr, e)))?;
                match tokio::time::timeout(Duration::from_secs(5), sock.connect(endpoint)).await {
                    Ok(Ok(())) => {
                        self.socket_cache.insert(addr.to_string(), sock);
                        last_err = None;
                        break;
                    }
                    Ok(Err(e)) => {
                        last_err = Some(format!("attempt {}: {}", attempt + 1, e));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    Err(_) => {
                        last_err = Some(format!("attempt {}: connect timed out", attempt + 1));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            if let Some(err) = last_err {
                return Err(Error::Kvs(format!(
                    "Failed to connect to {} after retries: {}",
                    addr, err
                )));
            }
        }
        Ok(self
            .socket_cache
            .get_mut(addr)
            .expect("socket was just inserted"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_lww_value_roundtrip() {
        let lww = LwwValue {
            timestamp: 12345,
            value: b"hello world".to_vec(),
        };
        let encoded = lww.encode_to_vec();
        let decoded = ValueChangeSubscriber::decode_lww_value(&encoded).expect("decode failed");
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_lww_value_empty() {
        let lww = LwwValue {
            timestamp: 0,
            value: vec![],
        };
        let encoded = lww.encode_to_vec();
        let decoded = ValueChangeSubscriber::decode_lww_value(&encoded).expect("decode failed");
        assert!(decoded.is_empty());
    }

    #[test]
    fn cache_registration_port_constant() {
        assert_eq!(K_CACHE_REGISTRATION_PORT, 7200);
    }

    #[test]
    fn cache_update_port_constant() {
        assert_eq!(K_CACHE_UPDATE_PORT, 7150);
    }

    #[test]
    fn decode_lww_value_invalid_proto() {
        let result = ValueChangeSubscriber::decode_lww_value(b"not valid protobuf");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn new_value_change_subscriber() {
        let config = crate::client_config::ClientConfig::default();
        let cache = ValueChangeSubscriber::new(&config, Some(90))
            .await
            .expect("Failed to create subscriber");
        assert!(cache.watched_keys().is_empty());
        assert!(cache.get_cached("nonexistent").is_none());
    }

    #[tokio::test]
    async fn recv_update_timeout() {
        let config = crate::client_config::ClientConfig::default();
        let mut cache = ValueChangeSubscriber::new(&config, Some(91))
            .await
            .expect("Failed to create cache client");
        let result = cache
            .recv_update(Duration::from_millis(100))
            .await
            .expect("recv_update error");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn recv_update_receives_pushed_value() {
        use crate::proto::kvs::{KeyResponse, KeyTuple};

        let config = crate::client_config::ClientConfig::default();
        let mut sub = ValueChangeSubscriber::new(&config, Some(92))
            .await
            .expect("create failed");

        let update_addr = format!("tcp://127.0.0.1:{}", 92 + K_CACHE_UPDATE_PORT);
        let ctx = Context::new();
        let pusher = ctx.socket(SocketType::Push, Options::default());
        pusher
            .connect(update_addr.parse().unwrap())
            .await
            .expect("connect failed");
        tokio::time::sleep(Duration::from_millis(100)).await;

        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: "pushed_key".into(),
                payload: b"pushed_value".to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let bytes = response.encode_to_vec();
        pusher
            .send(ZmqMessage::from(bytes))
            .await
            .expect("send failed");

        let result = sub
            .recv_update(Duration::from_secs(5))
            .await
            .expect("recv error");
        let (key, payload) = result.expect("recv_update returned None");
        assert_eq!(key, "pushed_key");
        assert_eq!(payload, b"pushed_value");
        assert_eq!(
            sub.get_cached("pushed_key").expect("key not in cache"),
            b"pushed_value"
        );
    }

    #[tokio::test]
    async fn recv_update_skips_empty_payload() {
        use crate::proto::kvs::{KeyResponse, KeyTuple};

        let config = crate::client_config::ClientConfig::default();
        let mut sub = ValueChangeSubscriber::new(&config, Some(93))
            .await
            .expect("create failed");

        let update_addr = format!("tcp://127.0.0.1:{}", 93 + K_CACHE_UPDATE_PORT);
        let ctx = Context::new();
        let pusher = ctx.socket(SocketType::Push, Options::default());
        pusher
            .connect(update_addr.parse().unwrap())
            .await
            .expect("connect failed");
        tokio::time::sleep(Duration::from_millis(100)).await;

        let response = KeyResponse {
            tuples: vec![KeyTuple {
                key: "empty_key".into(),
                payload: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        pusher
            .send(ZmqMessage::from(response.encode_to_vec()))
            .await
            .expect("send failed");

        let result = sub
            .recv_update(Duration::from_secs(2))
            .await
            .expect("recv error");
        assert!(result.is_none());
        assert!(sub.get_cached("empty_key").is_none());
    }

    #[tokio::test]
    async fn watched_keys_accumulate() {
        let config = crate::client_config::ClientConfig::default();
        let mut sub = ValueChangeSubscriber::new(&config, Some(94))
            .await
            .expect("create failed");
        assert!(sub.watched_keys().is_empty());

        sub.watched_keys = vec!["a".into(), "b".into()];
        assert_eq!(sub.watched_keys().len(), 2);
        assert_eq!(sub.watched_keys()[0], "a");
    }

    #[tokio::test]
    async fn with_memory_threads_sets_count() {
        let config = crate::client_config::ClientConfig::default();
        let sub = ValueChangeSubscriber::with_memory_threads(&config, Some(89), 4)
            .await
            .expect("create failed");
        assert_eq!(sub.memory_threads, 4);
    }

    #[tokio::test]
    async fn get_or_connect_rejects_invalid_address() {
        let config = crate::client_config::ClientConfig::default();
        let mut sub = ValueChangeSubscriber::new(&config, Some(95))
            .await
            .expect("create failed");
        let result = sub.get_or_connect("not_a_valid_endpoint").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid address"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn new_rejects_unparseable_bind_address() {
        let config = crate::client_config::ClientConfig {
            routing_addresses: vec!["tcp://127.0.0.1:6450".to_string()],
            client_ip: "not a valid ip".to_string(),
        };
        let result = ValueChangeSubscriber::new(&config, Some(96)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[cfg(unix)] // Windows SO_REUSEADDR allows duplicate binds
    async fn new_fails_on_port_conflict() {
        let config = crate::client_config::ClientConfig::default();
        // First subscriber binds the port successfully.
        let _sub1 = ValueChangeSubscriber::new(&config, Some(97))
            .await
            .expect("first create failed");
        // Second subscriber tries to bind the same port and should fail.
        let result = ValueChangeSubscriber::new(&config, Some(97)).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to bind"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn watch_sends_registration() {
        let config = crate::client_config::ClientConfig::default();
        // Use memory_threads=1 and create a PULL socket to receive the
        // registration message sent by watch().
        let mut sub = ValueChangeSubscriber::with_memory_threads(&config, Some(98), 1)
            .await
            .expect("create failed");

        let reg_addr = format!("tcp://127.0.0.1:{}", K_CACHE_REGISTRATION_PORT);
        let ctx = Context::new();
        let puller = ctx.socket(SocketType::Pull, Options::default());
        puller
            .bind(reg_addr.parse().expect("parse failed"))
            .await
            .expect("bind failed");
        tokio::time::sleep(Duration::from_millis(100)).await;

        sub.watch(&["test_key".to_string()])
            .await
            .expect("watch failed");

        let msg = tokio::time::timeout(Duration::from_secs(5), puller.recv())
            .await
            .expect("recv timed out")
            .expect("recv failed");
        let bytes: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
        let ss = StringSet::decode(bytes.as_slice()).expect("decode failed");
        assert!(ss.keys.contains(&"test_key".to_string()));
    }
}
