use crate::config::Config;
use crate::errors::{Error, Result};
use crate::proto::kvs::{KeyResponse, LwwValue};
use crate::proto::shared::StringSet;
use crate::types::{Address, Key};
use log::{debug, info};
use prost::Message;
use std::collections::HashMap;
use std::time::Duration;
use zeromq::{PullSocket, PushSocket, Socket, SocketRecv, SocketSend};

// The port on which KVS servers listen for direct cache registration.
const K_CACHE_REGISTRATION_PORT: usize = 7200;

// The port on which cache nodes receive updates from the KVS.
const K_CACHE_UPDATE_PORT: usize = 7150;

/// A cache client that receives key updates pushed from the KVS during gossip.
///
/// The cache client registers with KVS server threads to watch specific keys.
/// When those keys are updated, the KVS pushes the new values during its
/// gossip epoch. The cache client receives these updates and stores them
/// locally for fast reads.
///
/// # Example
///
/// ```rust,no_run
/// # #[tokio::main]
/// # async fn main() -> annalib::Result<()> {
/// use annalib::config::Config;
/// use annalib::cache_client::CacheClient;
///
/// let config = Config::default();
/// let mut cache = CacheClient::new(&config, None).await?;
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
pub struct CacheClient {
    cache_ip: Address,
    base_offset: usize,
    server_ip: Address,
    memory_threads: usize,
    socket_cache: HashMap<Address, PushSocket>,
    update_puller: PullSocket,
    local_cache: HashMap<Key, Vec<u8>>,
    watched_keys: Vec<Key>,
}

impl CacheClient {
    /// Create a new cache client.
    ///
    /// The `tid` parameter selects which port to listen on for updates.
    /// Pass `None` for the default (tid=0).
    pub async fn new(config: &Config, tid: Option<usize>) -> Result<Self> {
        let tid = tid.unwrap_or(0);
        let base_offset = config.get_base_offset();
        let cache_ip = config.get_user_ip().clone();
        let server_ip = config.get_server_public_ip().clone();
        let memory_threads = config.get_memory_thread_count();

        let bind_addr = format!(
            "tcp://{}:{}",
            cache_ip,
            tid + K_CACHE_UPDATE_PORT + base_offset
        );
        let mut update_puller = PullSocket::new();
        update_puller.bind(&bind_addr).await.map_err(|e| {
            Error::Kvs(format!(
                "Failed to bind cache update puller on {}: {}",
                bind_addr, e
            ))
        })?;

        info!("Cache client listening for updates on {}", bind_addr);

        Ok(CacheClient {
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
                .send(zeromq::ZmqMessage::from(payload.clone()))
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
                let bytes: Vec<u8> = msg
                    .into_vec()
                    .into_iter()
                    .flat_map(|frame| frame.to_vec())
                    .collect();

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

    async fn get_or_connect(&mut self, addr: &str) -> Result<&mut PushSocket> {
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
        let decoded = CacheClient::decode_lww_value(&encoded).expect("decode failed");
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_lww_value_empty() {
        let lww = LwwValue {
            timestamp: 0,
            value: vec![],
        };
        let encoded = lww.encode_to_vec();
        let decoded = CacheClient::decode_lww_value(&encoded).expect("decode failed");
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
}
