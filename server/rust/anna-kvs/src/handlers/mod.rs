//! KVS request handler functions.
//!
//! Each handler processes a deserialized protobuf message and may produce
//! outgoing ZMQ messages. Handlers take `&mut KvsContext` for shared state
//! and return `Vec<OutgoingMessage>` for messages that need to be sent.

pub(crate) mod cache_ip_response;
pub(crate) mod cache_registration;
pub(crate) mod gossip;
pub(crate) mod management_node_response;
pub(crate) mod node_depart;
pub(crate) mod node_join;
pub(crate) mod replication_change;
pub(crate) mod replication_response;
pub(crate) mod scan;
pub(crate) mod self_depart;
pub(crate) mod user_request;
pub(crate) mod utils;
