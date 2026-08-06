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
#[derive(Debug, Clone, Default, PartialEq)]
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

/// Metadata type for server stats keys.
pub enum MetadataType {
    Replication,
    ServerStats,
    KeyAccess,
    KeySize,
}

/// Build a metadata key for server stats, access, or size data.
///
/// Format: `ANNA_METADATA|<type>|<pub_ip>/<priv_ip>|<tid>|<tier_name>`
pub fn get_server_metadata_key(
    public_ip: &str,
    private_ip: &str,
    tid: u32,
    tier: Tier,
    meta_type: MetadataType,
) -> Key {
    let type_name = match meta_type {
        MetadataType::ServerStats => "stats",
        MetadataType::KeyAccess => "access",
        MetadataType::KeySize => "size",
        MetadataType::Replication => return get_metadata_key("", "replication"),
    };
    let tier_name = match tier {
        Tier::Memory => "MEMORY",
        Tier::Disk => "DISK",
        Tier::Routing => "ROUTING",
    };
    format!(
        "{}|{}|{}|{}|{}|{}",
        METADATA_IDENTIFIER, type_name, public_ip, private_ip, tid, tier_name
    )
}

/// Build a replication metadata key for a data key.
pub fn get_replication_key(data_key: &str) -> Key {
    get_metadata_key(data_key, "replication")
}

/// Extract the data key from a replication metadata key.
///
/// Input: `ANNA_METADATA|replication|<data_key>`
/// Output: `<data_key>`
pub fn get_key_from_metadata(metadata_key: &str) -> Option<&str> {
    let parts: Vec<&str> = metadata_key.splitn(3, METADATA_DELIMITER).collect();
    if parts.len() == 3 && parts[0] == METADATA_IDENTIFIER && parts[1] == "replication" {
        Some(parts[2])
    } else {
        None
    }
}

/// Initialize default replication factors for a key.
pub fn init_replication(
    key_replication_map: &mut HashMap<Key, KeyReplication>,
    key: &str,
    default_memory_rep: u32,
    default_disk_rep: u32,
    default_local_rep: u32,
) {
    let rep = key_replication_map.entry(key.to_string()).or_default();
    rep.global_replication
        .insert(Tier::Memory, default_memory_rep);
    rep.global_replication.insert(Tier::Disk, default_disk_rep);
    rep.local_replication
        .insert(Tier::Memory, default_local_rep);
    rep.local_replication.insert(Tier::Disk, default_local_rep);
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

    #[test]
    fn server_metadata_key_format() {
        let key = get_server_metadata_key(
            "1.2.3.4",
            "10.0.0.1",
            0,
            Tier::Memory,
            MetadataType::ServerStats,
        );
        assert_eq!(key, "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY");
    }

    #[test]
    fn server_metadata_key_access() {
        let key = get_server_metadata_key(
            "1.2.3.4",
            "10.0.0.1",
            2,
            Tier::Disk,
            MetadataType::KeyAccess,
        );
        assert_eq!(key, "ANNA_METADATA|access|1.2.3.4|10.0.0.1|2|DISK");
    }

    #[test]
    fn replication_key_format() {
        assert_eq!(
            get_replication_key("mykey"),
            "ANNA_METADATA|replication|mykey"
        );
    }

    #[test]
    fn get_key_from_metadata_replication() {
        assert_eq!(
            get_key_from_metadata("ANNA_METADATA|replication|mykey"),
            Some("mykey")
        );
    }

    #[test]
    fn get_key_from_metadata_non_replication() {
        assert_eq!(get_key_from_metadata("ANNA_METADATA|stats|data"), None);
    }

    #[test]
    fn get_key_from_metadata_not_metadata() {
        assert_eq!(get_key_from_metadata("user_key"), None);
    }

    #[test]
    fn init_replication_sets_defaults() {
        let mut map = HashMap::new();
        init_replication(&mut map, "key1", 2, 1, 1);
        let rep = map.get("key1").unwrap();
        assert_eq!(rep.global_replication[&Tier::Memory], 2);
        assert_eq!(rep.global_replication[&Tier::Disk], 1);
        assert_eq!(rep.local_replication[&Tier::Memory], 1);
        assert_eq!(rep.local_replication[&Tier::Disk], 1);
    }
}
