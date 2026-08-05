//! Shared types and infrastructure for Anna KVS Rust servers.
//!
//! This crate provides the foundation used by all Rust server binaries
//! (anna-monitor, anna-route, anna-kvs). It mirrors the shared C++ code
//! in `server/cpp/src/` (common.hpp, metadata.hpp, kvs_threads.hpp,
//! hash_ring/, etc.) and is wire-compatible with the C++ servers.

pub mod config;
pub mod hash_ring;
pub mod metadata;
pub mod ports;
pub mod proto;
pub mod signal;
pub mod threads;
pub mod types;
