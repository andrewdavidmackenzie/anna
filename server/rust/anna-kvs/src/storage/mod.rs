//! Storage layer: Serializer trait, memory and disk implementations.
//!
//! Mirrors `server/cpp/src/kvs/server_utils.hpp` and `base_kv_store.hpp`.

pub mod memory;

use anna_server_common::proto::kvs::LatticeType;
use std::collections::HashMap;

/// Result of a GET operation: (serialized_payload, error_code).
/// Error 0 = success, 1 = KEY_DNE.
pub type GetResult = (Vec<u8>, u32);

/// The serializer interface for KVS storage.
///
/// Each lattice type has a serializer that handles:
/// - `put`: merge a new value with the existing value (CRDT semantics)
/// - `get`: retrieve the current serialized value
/// - `remove`: delete a key
///
/// Mirrors the C++ `Serializer` base class.
pub trait Serializer: Send {
    /// Merge `payload` into the stored value for `key`.
    /// Returns the size of the stored value after merge.
    fn put(&mut self, key: &str, payload: &[u8]) -> usize;

    /// Get the serialized value for `key`.
    /// Returns (payload, error_code). Error 1 = KEY_DNE.
    fn get(&self, key: &str) -> GetResult;

    /// Remove a key. Returns true if the key existed.
    fn remove(&mut self, key: &str) -> bool;
}

/// Map from LatticeType to boxed Serializer.
pub type SerializerMap = HashMap<i32, Box<dyn Serializer>>;

/// Create a SerializerMap with all memory serializers.
pub fn create_memory_serializers() -> SerializerMap {
    let mut map = SerializerMap::new();

    // Each lattice type gets its own serializer instance.
    // Types that share serializer logic (e.g., LWW_SET reuses LWW)
    // still get separate instances for separate storage.
    map.insert(
        LatticeType::Lww as i32,
        Box::new(memory::LwwSerializer::new()),
    );
    map.insert(
        LatticeType::Set as i32,
        Box::new(memory::SetSerializer::new()),
    );
    map.insert(
        LatticeType::OrderedSet as i32,
        Box::new(memory::SetSerializer::new()),
    );
    map.insert(
        LatticeType::SingleCausal as i32,
        Box::new(memory::SingleCausalSerializer::new()),
    );
    map.insert(
        LatticeType::MultiCausal as i32,
        Box::new(memory::MultiCausalSerializer::new()),
    );
    map.insert(
        LatticeType::Priority as i32,
        Box::new(memory::PrioritySerializer::new()),
    );
    map.insert(
        LatticeType::Counter as i32,
        Box::new(memory::CounterSerializer::new()),
    );
    map.insert(
        LatticeType::OrSet as i32,
        Box::new(memory::OrSetSerializer::new()),
    );

    // Compound types reuse the base serializer but have separate storage.
    map.insert(
        LatticeType::LwwSet as i32,
        Box::new(memory::LwwSerializer::new()),
    );
    map.insert(
        LatticeType::LwwOrderedSet as i32,
        Box::new(memory::LwwSerializer::new()),
    );
    map.insert(
        LatticeType::UnionScalar as i32,
        Box::new(memory::SetSerializer::new()),
    );
    map.insert(
        LatticeType::PrioritySet as i32,
        Box::new(memory::PrioritySerializer::new()),
    );
    map.insert(
        LatticeType::PriorityOrderedSet as i32,
        Box::new(memory::PrioritySerializer::new()),
    );
    map.insert(
        LatticeType::CausalSet as i32,
        Box::new(memory::SingleCausalSerializer::new()),
    );
    map.insert(
        LatticeType::CausalOrderedSet as i32,
        Box::new(memory::SingleCausalSerializer::new()),
    );
    map.insert(
        LatticeType::MultiCausalSet as i32,
        Box::new(memory::MultiCausalSerializer::new()),
    );
    map.insert(
        LatticeType::MultiCausalOrderedSet as i32,
        Box::new(memory::MultiCausalSerializer::new()),
    );

    map
}
