//! KVS request handler functions.
//!
//! Each handler processes a deserialized protobuf message and may produce
//! outgoing ZMQ messages. Handlers take `&mut KvsContext` for shared state
//! and return `Vec<OutgoingMessage>` for messages that need to be sent.

pub mod cache_ip_response;
pub mod cache_registration;
pub mod gossip;
pub mod management_node_response;
pub mod node_depart;
pub mod node_join;
pub mod replication_change;
pub mod replication_response;
pub mod scan;
pub mod self_depart;
pub mod user_request;
pub mod utils;
