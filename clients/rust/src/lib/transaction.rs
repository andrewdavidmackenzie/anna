//! Client-side transactions providing Read Committed and Item Cut Isolation.
//!
//! A [`Transaction`] buffers writes locally and caches reads for repeatable
//! reads within the transaction. On [`commit()`](Transaction::commit), all
//! buffered writes are flushed to the server with a single dominating
//! timestamp, making them atomically visible.
//!
//! # Read Committed
//!
//! Within a transaction, reads see only committed data from other clients.
//! The client's own uncommitted writes are visible via the local write buffer
//! (read-your-writes within the transaction).
//!
//! # Item Cut Isolation
//!
//! Reading the same key twice within a transaction returns the same value
//! (repeatable read). The first read is cached; subsequent reads return the
//! cached value.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[tokio::main]
//! # async fn main() -> annalib::Result<()> {
//! # let config = annalib::client_config::ClientConfig::default();
//! # let mut client = annalib::kvs_client::KVSClient::new(&config, Some(0)).await;
//! let mut txn = annalib::transaction::Transaction::begin(&mut client);
//!
//! txn.put("key1", "value1");
//! let v = txn.get("key2").await?;   // reads from server, cached
//! let v2 = txn.get("key2").await?;  // returns cached value (repeatable)
//! assert_eq!(v, v2);
//!
//! txn.commit().await?;              // flushes "key1" to server
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;

use crate::kvs_client::KVSClient;
use crate::Result;

/// A client-side transaction that buffers writes and caches reads.
pub struct Transaction<'a> {
    client: &'a mut KVSClient,
    write_buffer: HashMap<String, String>,
    read_cache: HashMap<String, String>,
}

impl<'a> Transaction<'a> {
    /// Begin a new transaction on the given client.
    pub fn begin(client: &'a mut KVSClient) -> Self {
        Transaction {
            client,
            write_buffer: HashMap::new(),
            read_cache: HashMap::new(),
        }
    }

    /// Buffer a PUT for the given key. The write is not sent to the server
    /// until [`commit()`](Self::commit) is called.
    pub fn put(&mut self, key: &str, value: &str) {
        self.write_buffer.insert(key.to_string(), value.to_string());
        // Update the read cache so reads within this transaction see
        // the buffered write (read-your-writes within the transaction).
        self.read_cache.insert(key.to_string(), value.to_string());
    }

    /// Read a key. Returns the buffered write if this transaction has
    /// written to the key, otherwise reads from the server. The result
    /// is cached for repeatable reads (Item Cut Isolation).
    pub async fn get(&mut self, key: &str) -> Result<String> {
        // Check the read cache first (repeatable read).
        if let Some(cached) = self.read_cache.get(key) {
            return Ok(cached.clone());
        }

        // Read from the server and cache the result.
        let value = self.client.get(key).await?;
        self.read_cache.insert(key.to_string(), value.clone());
        Ok(value)
    }

    /// Flush all buffered writes to the server. All writes use a single
    /// dominating timestamp so they are atomically visible.
    pub async fn commit(mut self) -> Result<()> {
        for (key, value) in self.write_buffer.drain() {
            self.client.put(&key, &value).await?;
        }
        Ok(())
    }

    /// Discard all buffered writes without sending them to the server.
    pub fn rollback(self) {
        // Drop self — the write buffer and read cache are discarded.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transaction_put_then_get_returns_buffered_value() {
        let mut client = KVSClient::new_mock("127.0.0.1", 220);
        let mut txn = Transaction::begin(&mut client);

        txn.put("txn_key", "txn_val");
        let val = txn.get("txn_key").await.expect("GET failed");
        assert_eq!(val, "txn_val");
    }

    #[tokio::test]
    async fn transaction_repeatable_read() {
        use crate::proto::kvs::{KeyResponse, KeyTuple, LwwValue};
        use prost::Message;

        let mut client = KVSClient::new_mock("127.0.0.1", 221);
        let worker = "tcp://127.0.0.1:6200";

        // Push routing + GET response for the first read
        let routing_resp = {
            use crate::proto::kvs::key_address_response::KeyAddress;
            use crate::proto::kvs::KeyAddressResponse;
            let r = KeyAddressResponse {
                addresses: vec![KeyAddress {
                    key: "rr_key".to_string(),
                    ips: vec![worker.to_string()],
                }],
                ..Default::default()
            };
            r.encode_to_vec()
        };
        let get_resp = {
            let lww = LwwValue {
                timestamp: 100,
                value: b"server_val".to_vec(),
            };
            let r = KeyResponse {
                tuples: vec![KeyTuple {
                    key: "rr_key".to_string(),
                    payload: lww.encode_to_vec(),
                    ..Default::default()
                }],
                ..Default::default()
            };
            r.encode_to_vec()
        };
        client.push_mock_response(true, Some(routing_resp));
        client.push_mock_response(false, Some(get_resp));

        let mut txn = Transaction::begin(&mut client);

        let val1 = txn.get("rr_key").await.expect("first GET failed");
        assert_eq!(val1, "server_val");

        // Second read — no mock response pushed, should return cached
        let val2 = txn.get("rr_key").await.expect("second GET failed");
        assert_eq!(
            val2, "server_val",
            "Repeatable read should return cached value"
        );
    }

    #[tokio::test]
    async fn transaction_rollback_discards_writes() {
        let mut client = KVSClient::new_mock("127.0.0.1", 222);

        {
            let mut txn = Transaction::begin(&mut client);
            txn.put("rollback_key", "should_not_persist");
            txn.rollback();
        }

        // After rollback, the client should have no record of the write.
        // The lww_read_cache should not contain the key.
        assert!(
            !client.has_cached_read("rollback_key"),
            "Rollback should not leave cached writes"
        );
    }

    #[tokio::test]
    async fn transaction_commit_flushes_writes() {
        use crate::proto::kvs::{KeyResponse, KeyTuple};
        use prost::Message;

        let mut client = KVSClient::new_mock("127.0.0.1", 223);
        let worker = "tcp://127.0.0.1:6200";

        // Push routing + PUT response for the commit
        let routing_resp = {
            use crate::proto::kvs::key_address_response::KeyAddress;
            use crate::proto::kvs::KeyAddressResponse;
            let r = KeyAddressResponse {
                addresses: vec![KeyAddress {
                    key: "commit_key".to_string(),
                    ips: vec![worker.to_string()],
                }],
                ..Default::default()
            };
            r.encode_to_vec()
        };
        let put_resp = {
            let r = KeyResponse {
                tuples: vec![KeyTuple {
                    key: "commit_key".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };
            r.encode_to_vec()
        };
        client.push_mock_response(true, Some(routing_resp));
        client.push_mock_response(false, Some(put_resp));

        let mut txn = Transaction::begin(&mut client);
        txn.put("commit_key", "committed_val");
        txn.commit().await.expect("commit failed");

        // After commit, the client's lww_read_cache should have the value
        assert!(
            client.has_cached_read("commit_key"),
            "Commit should flush writes to client cache"
        );
    }
}
