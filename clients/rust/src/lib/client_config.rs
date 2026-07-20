/// Minimal client-side configuration for connecting to an anna cluster.
///
/// Clients only need routing addresses and their own bind IP. Everything else
/// (monitoring IPs, thread counts) is discovered at runtime or uses sensible
/// defaults. Server-side config files are never needed.
///
/// # Example
///
/// ```rust
/// use annalib::client_config::ClientConfig;
///
/// let config = ClientConfig {
///     routing_addresses: vec!["tcp://10.0.0.1:6450".to_string()],
///     client_ip: "127.0.0.1".to_string(),
/// };
/// assert_eq!(config.base_offset(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// ZMQ addresses of routing tier nodes, e.g. `["tcp://10.0.0.1:6450"]`.
    pub routing_addresses: Vec<String>,
    /// IP address this client binds on for receiving responses.
    pub client_ip: String,
}

const K_KEY_ADDRESS_PORT: usize = 6450;

impl ClientConfig {
    /// Derive the port base offset from the first routing address.
    ///
    /// The routing tier listens on `6450 + base_offset`. This method extracts
    /// the port from the first routing address and subtracts 6450.
    ///
    /// ```rust
    /// use annalib::client_config::ClientConfig;
    ///
    /// let config = ClientConfig {
    ///     routing_addresses: vec!["tcp://10.0.0.1:6550".to_string()],
    ///     client_ip: "127.0.0.1".to_string(),
    /// };
    /// assert_eq!(config.base_offset(), 100);
    /// ```
    pub fn base_offset(&self) -> usize {
        self.routing_addresses
            .first()
            .and_then(|addr| addr.rsplit(':').next())
            .and_then(|port_str| port_str.parse::<usize>().ok())
            .map(|port| port.saturating_sub(K_KEY_ADDRESS_PORT))
            .unwrap_or(0)
    }

    /// Extract the IP from the first routing address.
    ///
    /// Parses `tcp://host:port` to return `host`.
    pub fn routing_ip(&self) -> Option<&str> {
        self.routing_addresses.first().and_then(|addr| {
            addr.strip_prefix("tcp://")
                .and_then(|rest| rest.rsplit_once(':'))
                .map(|(host, _port)| host)
        })
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            routing_addresses: vec![format!("tcp://127.0.0.1:{}", K_KEY_ADDRESS_PORT)],
            client_ip: "127.0.0.1".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.routing_addresses, vec!["tcp://127.0.0.1:6450"]);
        assert_eq!(config.client_ip, "127.0.0.1");
    }

    #[test]
    fn base_offset_zero() {
        let config = ClientConfig {
            routing_addresses: vec!["tcp://10.0.0.1:6450".to_string()],
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(config.base_offset(), 0);
    }

    #[test]
    fn base_offset_nonzero() {
        let config = ClientConfig {
            routing_addresses: vec!["tcp://10.0.0.1:6550".to_string()],
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(config.base_offset(), 100);
    }

    #[test]
    fn base_offset_empty_addresses() {
        let config = ClientConfig {
            routing_addresses: vec![],
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(config.base_offset(), 0);
    }

    #[test]
    fn routing_ip_extraction() {
        let config = ClientConfig {
            routing_addresses: vec!["tcp://10.0.0.1:6450".to_string()],
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(config.routing_ip(), Some("10.0.0.1"));
    }

    #[test]
    fn routing_ip_empty() {
        let config = ClientConfig {
            routing_addresses: vec![],
            client_ip: "127.0.0.1".to_string(),
        };
        assert_eq!(config.routing_ip(), None);
    }
}
