//! Metadata types and helpers.
//!
//! Mirrors `server/cpp/src/metadata.hpp`.

use crate::proto::kvs::LatticeType;
use crate::types::Key;
use std::collections::HashMap;

/// The metadata key prefix.
pub const METADATA_IDENTIFIER: &str = "ANNA_METADATA";
const METADATA_DELIMITER: char = '|';

/// Storage tier identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    Memory = 1,
    Disk = 2,
    Routing = 3,
}

/// Metadata about a storage tier.
#[derive(Debug, Clone)]
pub struct TierMetadata {
    pub id: Tier,
    pub thread_number: u32,
    pub default_replication: u32,
    pub node_capacity: u64,
}

/// Per-key replication factors.
#[derive(Debug, Clone, Default)]
pub struct KeyReplication {
    pub global_replication: HashMap<Tier, u32>,
    pub local_replication: HashMap<Tier, u32>,
}

/// Compact per-key property (mirrors the 8-byte C++ struct).
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyProperty {
    size_and_type: u32,
    pub expiry_epoch_s: u32,
}

impl KeyProperty {
    pub fn size(&self) -> u32 {
        self.size_and_type >> 8
    }

    pub fn set_size(&mut self, s: u32) {
        self.size_and_type = (s << 8) | (self.size_and_type & 0xFF);
    }

    pub fn lattice_type(&self) -> LatticeType {
        LatticeType::try_from((self.size_and_type & 0xFF) as i32).unwrap_or(LatticeType::None)
    }

    pub fn set_type(&mut self, t: LatticeType) {
        self.size_and_type = (self.size_and_type & 0xFFFF_FF00) | (t as u32 & 0xFF);
    }
}

/// Check if a key is an internal metadata key.
pub fn is_metadata(key: &str) -> bool {
    key.starts_with(METADATA_IDENTIFIER)
        && key.len() > METADATA_IDENTIFIER.len()
        && key.as_bytes()[METADATA_IDENTIFIER.len()] == METADATA_DELIMITER as u8
}

/// Build a metadata key for a given type and data key.
pub fn get_metadata_key(key: &str, metadata_type: &str) -> Key {
    format!(
        "{}{}{}{}{}",
        METADATA_IDENTIFIER, METADATA_DELIMITER, metadata_type, METADATA_DELIMITER, key
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_metadata_true() {
        assert!(is_metadata("ANNA_METADATA|replication|mykey"));
        assert!(is_metadata("ANNA_METADATA|cluster_topology"));
    }

    #[test]
    fn is_metadata_false() {
        assert!(!is_metadata("user_key"));
        assert!(!is_metadata("ANNA_METADATA")); // no delimiter
        assert!(!is_metadata("ANNA_METADATAxyz")); // no delimiter
    }

    #[test]
    fn get_metadata_key_format() {
        assert_eq!(
            get_metadata_key("mykey", "replication"),
            "ANNA_METADATA|replication|mykey"
        );
    }

    #[test]
    fn key_property_size_and_type() {
        let mut kp = KeyProperty::default();
        kp.set_size(1024);
        kp.set_type(LatticeType::Lww);
        assert_eq!(kp.size(), 1024);
        assert_eq!(kp.lattice_type(), LatticeType::Lww);

        kp.set_type(LatticeType::Set);
        assert_eq!(kp.lattice_type(), LatticeType::Set);
        assert_eq!(kp.size(), 1024); // size preserved
    }

    #[test]
    fn key_property_expiry() {
        let mut kp = KeyProperty::default();
        assert_eq!(kp.expiry_epoch_s, 0);
        kp.expiry_epoch_s = 1234567890;
        assert_eq!(kp.expiry_epoch_s, 1234567890);
    }

    #[test]
    fn tier_enum_values_match_cpp() {
        assert_eq!(Tier::Memory as i32, 1);
        assert_eq!(Tier::Disk as i32, 2);
        assert_eq!(Tier::Routing as i32, 3);
    }
}
