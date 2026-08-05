//! Core type aliases used throughout the server codebase.
//!
//! Mirrors `server/cpp/src/types.hpp`.

use std::collections::HashMap;

/// A network address string (e.g., "tcp://127.0.0.1:6200").
pub type Address = String;

/// A key in the KVS.
pub type Key = String;

/// A thread ID (0-based).
pub type ThreadID = u32;

/// Hash map type alias.
pub type Map<K, V> = HashMap<K, V>;
