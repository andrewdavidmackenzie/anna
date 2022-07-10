use crate::types::Address;

// The port on which clients send key address requests to routing nodes.
#[allow(dead_code)]
const K_KEY_ADDRESS_PORT: usize = 6450;

// The port on which clients receive responses from the KVS.
const K_USER_RESPONSE_PORT: usize = 6800;

// The port on which clients receive responses from the routing tier.
const K_USER_KEY_ADDRESS_PORT: usize = 6850;

// The port on which cache nodes listen for updates from the KVS.
const K_CACHE_UPDATE_PORT: usize = 7150;

const K_BIND_BASE: &str = "tcp://*:";

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

/// `UserRoutingThread` - is a special case of a `UserThread` that is used for routing
pub type UserRoutingThread = UserThread;

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
