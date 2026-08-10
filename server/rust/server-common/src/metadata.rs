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

/// Metadata type discriminator — mirrors C++ `MetadataType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataType {
    Replication,
    ServerStats,
    KeyAccess,
    KeySize,
}

impl MetadataType {
    fn as_str(&self) -> &'static str {
        match self {
            MetadataType::Replication => "replication",
            MetadataType::ServerStats => "stats",
            MetadataType::KeyAccess => "access",
            MetadataType::KeySize => "size",
        }
    }
}

/// All tiers that store user data.
pub const ALL_DATA_TIERS: &[Tier] = &[Tier::Memory, Tier::Disk];

/// Initialize a key's replication factors to the defaults from tier metadata.
pub fn init_replication(
    key_replication_map: &mut HashMap<Key, KeyReplication>,
    key: &Key,
    tier_metadata: &HashMap<Tier, TierMetadata>,
    default_local_replication: u32,
) {
    let kr = key_replication_map.entry(key.clone()).or_default();
    for tier in ALL_DATA_TIERS {
        if let Some(tm) = tier_metadata.get(tier) {
            kr.global_replication
                .entry(*tier)
                .or_insert(tm.default_replication);
        }
        kr.local_replication
            .entry(*tier)
            .or_insert(default_local_replication);
    }
}

/// Check if a key is an internal metadata key.
pub fn is_metadata(key: &str) -> bool {
    key.starts_with(METADATA_IDENTIFIER)
        && key.len() > METADATA_IDENTIFIER.len()
        && key.as_bytes()[METADATA_IDENTIFIER.len()] == METADATA_DELIMITER as u8
}

/// Build a metadata key for a given type and data key.
/// Example: `get_metadata_key("mykey", "replication")` → `"ANNA_METADATA|replication|mykey"`
pub fn get_metadata_key(key: &str, metadata_type: &str) -> Key {
    format!(
        "{}{}{}{}{}",
        METADATA_IDENTIFIER, METADATA_DELIMITER, metadata_type, METADATA_DELIMITER, key
    )
}

/// Build a server-stats metadata key for a specific server thread.
/// Example: `"ANNA_METADATA|stats|1.2.3.4|10.0.0.1|2|MEMORY"`
pub fn get_server_metadata_key(
    st: &crate::threads::ServerThread,
    tier: Tier,
    thread_num: u32,
    metadata_type: MetadataType,
) -> Key {
    match metadata_type {
        MetadataType::Replication => String::new(), // use get_metadata_key instead
        _ => format!(
            "{}{}{}{}{}{}{}{}{}{}{}",
            METADATA_IDENTIFIER,
            METADATA_DELIMITER,
            metadata_type.as_str(),
            METADATA_DELIMITER,
            st.public_ip(),
            METADATA_DELIMITER,
            st.private_ip(),
            METADATA_DELIMITER,
            thread_num,
            METADATA_DELIMITER,
            tier_name(tier),
        ),
    }
}

/// Extract the data key from a replication metadata key.
/// Returns `None` for non-replication metadata keys.
pub fn get_key_from_metadata(metadata_key: &str) -> Option<&str> {
    // Format: ANNA_METADATA|replication|<data_key>
    let rest = metadata_key.strip_prefix(METADATA_IDENTIFIER)?;
    let rest = rest.strip_prefix(METADATA_DELIMITER)?;
    if let Some(data_key) = rest.strip_prefix("replication") {
        Some(data_key.strip_prefix(METADATA_DELIMITER).unwrap_or(""))
    } else {
        None
    }
}

fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Memory => "MEMORY",
        Tier::Disk => "DISK",
        Tier::Routing => "ROUTING",
    }
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
    fn get_server_metadata_key_stats() {
        let st = crate::threads::ServerThread::new("1.2.3.4", "10.0.0.1", 0, 0);
        let key = get_server_metadata_key(&st, Tier::Memory, 2, MetadataType::ServerStats);
        assert_eq!(key, "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|2|MEMORY");
    }

    #[test]
    fn get_server_metadata_key_replication_returns_empty() {
        let st = crate::threads::ServerThread::new("1.2.3.4", "10.0.0.1", 0, 0);
        let key = get_server_metadata_key(&st, Tier::Memory, 0, MetadataType::Replication);
        assert!(key.is_empty());
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
        assert_eq!(
            get_key_from_metadata("ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY"),
            None
        );
    }

    #[test]
    fn get_key_from_metadata_not_metadata() {
        assert_eq!(get_key_from_metadata("user_key"), None);
    }

    #[test]
    fn init_replication_sets_defaults() {
        let mut kr_map = std::collections::HashMap::new();
        let mut tier_metadata = std::collections::HashMap::new();
        tier_metadata.insert(
            Tier::Memory,
            TierMetadata {
                id: Tier::Memory,
                thread_number: 1,
                default_replication: 2,
                node_capacity: 1024,
            },
        );
        init_replication(&mut kr_map, &"test_key".to_string(), &tier_metadata, 1);
        let kr = &kr_map["test_key"];
        assert_eq!(kr.global_replication[&Tier::Memory], 2);
        assert_eq!(kr.local_replication[&Tier::Memory], 1);
    }

    #[test]
    fn metadata_type_as_str() {
        assert_eq!(MetadataType::Replication.as_str(), "replication");
        assert_eq!(MetadataType::ServerStats.as_str(), "stats");
        assert_eq!(MetadataType::KeyAccess.as_str(), "access");
        assert_eq!(MetadataType::KeySize.as_str(), "size");
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
