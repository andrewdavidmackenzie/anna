use crate::types::Address;

// The port on which clients send key address requests to routing nodes.
const K_KEY_ADDRESS_PORT: usize = 6450;

// The port on which clients receive responses from the KVS.
const K_USER_RESPONSE_PORT: usize = 6800;

// The port on which clients receive responses from the routing tier.
const K_USER_KEY_ADDRESS_PORT: usize = 6850;

// The port on which cache nodes listen for updates from the KVS.
const K_CACHE_UPDATE_PORT: usize = 7150;

const K_BIND_BASE: &str = "tcp://0.0.0.0:";

/// `Thread` is a base type for a number of other thread types
pub struct Thread {
    ip: Address,
    ip_base: Address,
    tid: usize,
}

impl Thread {
    /// return the `ìp` IP address of this thread
    pub fn ip(&self) -> &Address {
        &self.ip
    }

    /// return the `tid` thread id of this thread
    pub fn tid(&self) -> usize {
        self.tid
    }

    /// return the ZMQ `Address` used to bind to a Key
    pub fn key_address_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_USER_KEY_ADDRESS_PORT)
    }

    /// return the ZMQ `Address` used to connect to a Key
    pub fn key_address_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_USER_KEY_ADDRESS_PORT)
    }
}

/// `UserThread` extends `Thread` with response address methods
pub type UserThread = Thread;

impl UserThread {
    /// create a new `UserThread` from the `Address` and `tid`thread id
    pub fn new(ip: &Address, tid: usize) -> Self {
        UserThread {
            ip: ip.clone(),
            tid,
            ip_base: format!("tcp://{}:", ip),
        }
    }

    /// return the ZMQ `Address` to use to bind to a response
    pub fn response_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_USER_RESPONSE_PORT)
    }

    /// return the ZMQ `Address` to use to connect to a response
    pub fn response_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_USER_RESPONSE_PORT)
    }
}

/// `UserRoutingThread` connects to the routing tier on port 6450 (not 6850).
pub struct UserRoutingThread {
    ip_base: Address,
    tid: usize,
}

impl UserRoutingThread {
    /// Create a new routing thread reference.
    pub fn new(ip: &Address, tid: usize) -> Self {
        UserRoutingThread {
            ip_base: format!("tcp://{}:", ip),
            tid,
        }
    }

    /// The address to connect to for sending key address requests to the routing tier.
    pub fn key_address_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_KEY_ADDRESS_PORT)
    }
}

/// `CacheThread` extends `Thread` with a number of methods
pub type CacheThread = Thread;

impl CacheThread {
    /// return the `Address` to use to bind to a `GET` in the cache
    pub fn cache_get_bind_address(&self) -> Address {
        "ipc:///requests/get".into()
    }

    /// return the `Address` to use to connect to a `GET` in the cache
    pub fn cache_get_connect_address(&self) -> Address {
        "ipc:///requests/get".into()
    }

    /// return the `Address` to use to bind to a `PUT` in the cache
    pub fn cache_put_bind_address(&self) -> Address {
        "ipc:///requests/put".into()
    }

    /// return the `Address` to use to connect to a `PUT` in the cache
    pub fn cache_put_connect_address(&self) -> Address {
        "ipc:///requests/put".into()
    }

    /// return the `Address` to use to bind to an update in the cache
    pub fn cache_update_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_CACHE_UPDATE_PORT)
    }

    /// return the `Address` to use to connect to an update in the cache
    pub fn cache_update_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_CACHE_UPDATE_PORT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_thread_accessors() {
        let ut = UserThread::new(&"10.0.0.1".into(), 3);
        assert_eq!(ut.ip(), "10.0.0.1");
        assert_eq!(ut.tid(), 3);
    }

    #[test]
    fn user_thread_response_addresses() {
        let ut = UserThread::new(&"10.0.0.1".into(), 0);
        assert_eq!(ut.response_bind_address(), "tcp://0.0.0.0:6800");
        assert_eq!(ut.response_connect_address(), "tcp://10.0.0.1:6800");
    }

    #[test]
    fn user_thread_response_with_offset() {
        let ut = UserThread::new(&"10.0.0.1".into(), 2);
        assert_eq!(ut.response_bind_address(), "tcp://0.0.0.0:6802");
        assert_eq!(ut.response_connect_address(), "tcp://10.0.0.1:6802");
    }

    #[test]
    fn user_thread_key_addresses() {
        let ut = UserThread::new(&"10.0.0.1".into(), 0);
        assert_eq!(ut.key_address_bind_address(), "tcp://0.0.0.0:6850");
        assert_eq!(ut.key_address_connect_address(), "tcp://10.0.0.1:6850");
    }

    #[test]
    fn user_thread_key_addresses_with_offset() {
        let ut = UserThread::new(&"10.0.0.1".into(), 3);
        assert_eq!(ut.key_address_bind_address(), "tcp://0.0.0.0:6853");
        assert_eq!(ut.key_address_connect_address(), "tcp://10.0.0.1:6853");
    }

    #[test]
    fn cache_thread_static_addresses() {
        let ct = UserThread::new(&"10.0.0.1".into(), 0);
        assert_eq!(ct.cache_get_bind_address(), "ipc:///requests/get");
        assert_eq!(ct.cache_get_connect_address(), "ipc:///requests/get");
        assert_eq!(ct.cache_put_bind_address(), "ipc:///requests/put");
        assert_eq!(ct.cache_put_connect_address(), "ipc:///requests/put");
    }

    #[test]
    fn cache_thread_update_addresses() {
        let ct = UserThread::new(&"10.0.0.1".into(), 1);
        assert_eq!(ct.cache_update_bind_address(), "tcp://0.0.0.0:7151");
        assert_eq!(ct.cache_update_connect_address(), "tcp://10.0.0.1:7151");
    }

    #[test]
    fn routing_thread_uses_port_6450() {
        let rt = super::UserRoutingThread::new(&"10.0.0.1".into(), 0);
        assert_eq!(rt.key_address_connect_address(), "tcp://10.0.0.1:6450");
    }

    #[test]
    fn routing_thread_with_offset() {
        let rt = super::UserRoutingThread::new(&"10.0.0.1".into(), 2);
        assert_eq!(rt.key_address_connect_address(), "tcp://10.0.0.1:6452");
    }
}
