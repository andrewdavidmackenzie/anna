//! KVS request handler functions.
//!
//! Each handler processes a deserialized protobuf message and may produce
//! outgoing ZMQ messages. Handlers take `&mut KvsContext` for shared state
//! and return `Vec<OutgoingMessage>` for messages that need to be sent.

pub(crate) mod node_depart;
pub(crate) mod scan;
pub(crate) mod utils;
