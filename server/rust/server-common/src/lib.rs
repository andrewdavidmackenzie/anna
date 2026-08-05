//! Shared types and infrastructure for Anna KVS Rust servers.
//!
//! This crate provides the foundation used by all Rust server binaries
//! (anna-monitor, anna-route, anna-kvs). It mirrors the shared C++ code
//! in `server/cpp/src/` (common.hpp, metadata.hpp, kvs_threads.hpp,
//! hash_ring/, etc.).
//!
//! Wire compatibility with C++ servers is maintained via shared protobuf
//! messages and port layout. The hash ring uses Rust's `DefaultHasher`
//! which produces different values than C++ `std::hash` — Rust and C++
//! servers do not share hash ring state.

pub mod config;
pub mod hash_ring;
pub mod metadata;
pub mod ports;
pub mod proto;
pub mod signal;
pub mod threads;
pub mod types;
