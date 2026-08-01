use crate::errors::{Error, Result};
use crate::kvs_client::KVSClient;
use crate::proto::metadata::user_feedback::KeyLatency;
use crate::proto::metadata::UserFeedback;
use crate::proto::shared::StringSet;
use crate::types::Address;
use log::info;
use omq_tokio::{Context, Message as ZmqMessage, Options, Socket as OmqSocket, SocketType};
use prost::Message;
use std::collections::HashMap;
use std::time::Duration;

const K_FEEDBACK_REPORT_PORT: usize = 6953;
const MONITORING_IPS_KEY: &str = "ANNA_METADATA|monitoring_ips";

/// Reports client-observed latency to the anna monitor for SLO enforcement.
///
/// When the monitor receives feedback indicating latency above the SLO
/// threshold (3ms), it can trigger hot-key replication or cluster scaling.
///
/// # Example
///
/// ```rust,no_run
/// # #[tokio::main]
/// # async fn main() -> annalib::Result<()> {
/// use annalib::client_config::ClientConfig;
/// use annalib::kvs_client::KVSClient;
/// use annalib::latency_reporter::LatencyReporter;
///
/// let config = ClientConfig::default();
/// let mut client = KVSClient::new(&config, Some(1)).await;
/// let mut reporter = LatencyReporter::new(&mut client, Some(1)).await?;
///
/// reporter.report(5000.0, 100.0, &[("hot_key".into(), 5000.0)]).await?;
/// reporter.finish().await?;
/// # Ok(())
/// # }
/// ```
pub struct LatencyReporter {
    uid: String,
    ctx: Context,
    base_offset: usize,
    warmup: bool,
    socket_cache: HashMap<Address, OmqSocket>,
    monitoring_ips: Vec<Address>,
}

impl LatencyReporter {
    /// Create a new reporter by discovering monitoring IPs via metadata.
    ///
    /// Queries `ANNA_METADATA|monitoring_ips` from the KVS to find the
    /// monitor addresses. Connects ZMQ PUSH sockets to each monitor's
    /// feedback port.
    pub async fn new(client: &mut KVSClient, tid: Option<usize>) -> Result<Self> {
        let tid = tid.unwrap_or(0);
        let base_offset = client.base_offset();

        let monitoring_ips = match client.get_bytes(MONITORING_IPS_KEY).await {
            Ok(bytes) => {
                let string_set = StringSet::decode(bytes.as_slice())
                    .map_err(|e| Error::Kvs(format!("Failed to decode monitoring IPs: {}", e)))?;
                string_set.keys
            }
            Err(_) => {
                info!("No monitoring IPs found in metadata, using routing IP");
                vec![]
            }
        };

        let uid = format!("rust_client:{}", tid);

        Ok(LatencyReporter {
            uid,
            ctx: Context::new(),
            base_offset,
            warmup: false,
            socket_cache: HashMap::new(),
            monitoring_ips,
        })
    }

    /// Create a reporter with explicit monitoring IPs (no metadata lookup).
    pub fn with_monitoring_ips(
        monitoring_ips: Vec<Address>,
        base_offset: usize,
        tid: Option<usize>,
    ) -> Self {
        let tid = tid.unwrap_or(0);
        LatencyReporter {
            uid: format!("rust_client:{}", tid),
            ctx: Context::new(),
            base_offset,
            warmup: false,
            socket_cache: HashMap::new(),
            monitoring_ips,
        }
    }

    /// Pre-connect to all monitoring threads.
    ///
    /// ZMQ connections are asynchronous — calling `connect()` initiates the
    /// TCP/ZMTP handshake but messages sent before it completes may be queued
    /// or dropped. Call this method and wait briefly before the first `report()`
    /// to ensure connections are established.
    pub async fn connect(&mut self) -> Result<()> {
        for ip in self.monitoring_ips.clone() {
            let addr = format!("tcp://{}:{}", ip, K_FEEDBACK_REPORT_PORT + self.base_offset);
            self.get_or_connect(&addr).await?;
        }
        Ok(())
    }

    /// Set the warmup flag for subsequent reports.
    ///
    /// When warmup is true, the monitor ignores policy decisions (e.g.
    /// won't trigger replication changes based on this feedback).
    pub fn set_warmup(&mut self, warmup: bool) {
        self.warmup = warmup;
    }

    /// Report latency feedback to all monitoring threads.
    ///
    /// - `latency_us`: aggregate perceived latency in microseconds
    /// - `throughput`: operations per second
    /// - `key_latencies`: per-key latency observations `(key, latency_us)`
    pub async fn report(
        &mut self,
        latency_us: f64,
        throughput: f64,
        key_latencies: &[(String, f64)],
    ) -> Result<()> {
        let feedback = UserFeedback {
            uid: self.uid.clone(),
            latency: latency_us,
            throughput,
            finish: false,
            warmup: self.warmup,
            key_latency: key_latencies
                .iter()
                .map(|(key, latency)| KeyLatency {
                    key: key.clone(),
                    latency: *latency,
                })
                .collect(),
        };

        self.send_feedback(&feedback).await
    }

    /// Signal that this client is done reporting.
    pub async fn finish(&mut self) -> Result<()> {
        let feedback = UserFeedback {
            uid: self.uid.clone(),
            finish: true,
            ..Default::default()
        };

        self.send_feedback(&feedback).await
    }

    async fn send_feedback(&mut self, feedback: &UserFeedback) -> Result<()> {
        let payload = feedback.encode_to_vec();

        for ip in self.monitoring_ips.clone() {
            let addr = format!("tcp://{}:{}", ip, K_FEEDBACK_REPORT_PORT + self.base_offset);
            let socket = self.get_or_connect(&addr).await?;
            socket
                .send(ZmqMessage::from(payload.clone()))
                .await
                .map_err(|e| Error::Kvs(format!("Failed to send feedback to {}: {}", addr, e)))?;
        }

        Ok(())
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
    fn feedback_protobuf_roundtrip() {
        let feedback = UserFeedback {
            uid: "test:0".into(),
            latency: 5000.0,
            throughput: 100.0,
            finish: false,
            warmup: false,
            key_latency: vec![
                KeyLatency {
                    key: "key_a".into(),
                    latency: 4000.0,
                },
                KeyLatency {
                    key: "key_b".into(),
                    latency: 6000.0,
                },
            ],
        };

        let encoded = feedback.encode_to_vec();
        let decoded = UserFeedback::decode(encoded.as_slice()).expect("decode failed");
        assert_eq!(decoded.uid, "test:0");
        assert!((decoded.latency - 5000.0).abs() < f64::EPSILON);
        assert!((decoded.throughput - 100.0).abs() < f64::EPSILON);
        assert!(!decoded.finish);
        assert!(!decoded.warmup);
        assert_eq!(decoded.key_latency.len(), 2);
        assert_eq!(decoded.key_latency[0].key, "key_a");
    }

    #[test]
    fn finish_message_roundtrip() {
        let feedback = UserFeedback {
            uid: "test:1".into(),
            finish: true,
            ..Default::default()
        };

        let encoded = feedback.encode_to_vec();
        let decoded = UserFeedback::decode(encoded.as_slice()).expect("decode failed");
        assert_eq!(decoded.uid, "test:1");
        assert!(decoded.finish);
        assert!(decoded.key_latency.is_empty());
    }

    #[test]
    fn with_monitoring_ips_constructor() {
        let reporter = LatencyReporter::with_monitoring_ips(vec!["10.0.0.1".into()], 100, Some(5));
        assert_eq!(reporter.uid, "rust_client:5");
        assert_eq!(reporter.base_offset, 100);
        assert_eq!(reporter.monitoring_ips, vec!["10.0.0.1"]);
        assert!(!reporter.warmup);
    }

    #[test]
    fn set_warmup_flag() {
        let mut reporter = LatencyReporter::with_monitoring_ips(vec!["10.0.0.1".into()], 0, None);
        assert!(!reporter.warmup);
        reporter.set_warmup(true);
        assert!(reporter.warmup);
    }

    #[tokio::test]
    async fn send_and_receive_feedback() {
        let port = 6953 + 80;
        let ctx = Context::new();
        let puller = ctx.socket(SocketType::Pull, Options::default());
        puller
            .bind(format!("tcp://127.0.0.1:{}", port).parse().unwrap())
            .await
            .expect("bind failed");

        let mut reporter =
            LatencyReporter::with_monitoring_ips(vec!["127.0.0.1".into()], 80, Some(0));

        tokio::time::sleep(Duration::from_millis(100)).await;

        reporter
            .report(5000.0, 200.0, &[("hot_key".into(), 5000.0)])
            .await
            .expect("report failed");

        let msg = tokio::time::timeout(Duration::from_secs(5), puller.recv())
            .await
            .expect("recv timed out")
            .expect("recv failed");
        let bytes: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
        let decoded = UserFeedback::decode(bytes.as_slice()).expect("decode failed");
        assert_eq!(decoded.uid, "rust_client:0");
        assert!((decoded.latency - 5000.0).abs() < f64::EPSILON);
        assert!((decoded.throughput - 200.0).abs() < f64::EPSILON);
        assert_eq!(decoded.key_latency.len(), 1);
        assert_eq!(decoded.key_latency[0].key, "hot_key");
    }

    #[tokio::test]
    async fn send_finish_signal() {
        let port = 6953 + 81;
        let ctx = Context::new();
        let puller = ctx.socket(SocketType::Pull, Options::default());
        puller
            .bind(format!("tcp://127.0.0.1:{}", port).parse().unwrap())
            .await
            .expect("bind failed");

        let mut reporter =
            LatencyReporter::with_monitoring_ips(vec!["127.0.0.1".into()], 81, Some(0));

        tokio::time::sleep(Duration::from_millis(100)).await;

        reporter.finish().await.expect("finish failed");

        let msg = tokio::time::timeout(Duration::from_secs(5), puller.recv())
            .await
            .expect("recv timed out")
            .expect("recv failed");
        let bytes: Vec<u8> = msg.iter().flat_map(|f| f.to_vec()).collect();
        let decoded = UserFeedback::decode(bytes.as_slice()).expect("decode failed");
        assert!(decoded.finish);
    }

    #[tokio::test]
    async fn connect_pre_establishes_sockets() {
        let port = 6953 + 82;
        let ctx = Context::new();
        let puller = ctx.socket(SocketType::Pull, Options::default());
        puller
            .bind(format!("tcp://127.0.0.1:{}", port).parse().unwrap())
            .await
            .expect("bind failed");

        let mut reporter =
            LatencyReporter::with_monitoring_ips(vec!["127.0.0.1".into()], 82, Some(0));
        reporter.connect().await.expect("connect failed");
        assert_eq!(reporter.socket_cache.len(), 1);
    }

    #[tokio::test]
    async fn get_or_connect_rejects_invalid_address() {
        let mut reporter = LatencyReporter::with_monitoring_ips(vec!["127.0.0.1".into()], 0, None);
        let result = reporter.get_or_connect("not_a_valid_endpoint").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid address"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn new_from_mock_client() {
        let mut client = KVSClient::new_mock("127.0.0.1", 83);
        // Mock has no monitoring IPs metadata, so new() falls back to empty vec.
        let reporter = LatencyReporter::new(&mut client, Some(83))
            .await
            .expect("new failed");
        assert_eq!(reporter.uid, "rust_client:83");
        assert!(reporter.monitoring_ips.is_empty());
    }
}
