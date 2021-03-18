use crate::kvs_client::Address;

// The port on which clients send key address requests to routing nodes.
const K_KEY_ADDRESS_PORT: usize = 6450;

// The port on which clients receive responses from the KVS.
const K_USER_RESPONSE_PORT: usize = 6800;

// The port on which clients receive responses from the routing tier.
const K_USER_KEY_ADDRESS_PORT: usize = 6850;

// The port on which cache nodes listen for updates from the KVS.
const K_CACHE_UPDATE_PORT: usize = 7150;

const K_BIND_BASE: &str = "tcp://*:";

pub struct Thread {
    ip: Address,
    ip_base: Address,
    tid: usize,
}

impl Thread {
    pub fn ip(&self) -> &Address {
        &self.ip
    }

    pub fn tid(&self) -> usize {
        self.tid
    }

    pub fn key_address_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_USER_KEY_ADDRESS_PORT)
    }

    pub fn key_address_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_USER_KEY_ADDRESS_PORT)
    }
}

// UserThread
pub type UserThread = Thread;

impl UserThread {
    pub fn new(ip: &Address, tid: usize) -> Self {
        UserThread {
            ip: ip.clone(),
            tid,
            ip_base: format!("tcp://{}:", ip),
        }
    }

    pub fn response_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_USER_RESPONSE_PORT)
    }

    pub fn response_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_USER_RESPONSE_PORT)
    }
}

// UserRoutingThread
pub type UserRoutingThread = UserThread;

// CacheThread
pub type CacheThread = Thread;

impl CacheThread {
    pub fn cache_get_bind_address(&self) -> Address {
        "ipc:///requests/get".into()
    }

    pub fn cache_get_connect_address(&self) -> Address {
        "ipc:///requests/get".into()
    }

    pub fn cache_put_bind_address(&self) -> Address {
        "ipc:///requests/put".into()
    }

    pub fn cache_put_connect_address(&self) -> Address {
        "ipc:///requests/put".into()
    }

    pub fn cache_update_bind_address(&self) -> Address {
        format!("{}{}", K_BIND_BASE, self.tid + K_CACHE_UPDATE_PORT)
    }

    pub fn cache_update_connect_address(&self) -> Address {
        format!("{}{}", self.ip_base, self.tid + K_CACHE_UPDATE_PORT)
    }
}