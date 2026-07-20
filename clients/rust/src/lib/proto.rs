// Include the `shared` module, which is generated from shared.proto.
#[allow(warnings, missing_docs)]
pub mod shared {
    include!(concat!(env!("OUT_DIR"), "/shared.rs"));
}

// Include the `kvs` module, which is generated from kvs.proto.
#[allow(warnings, missing_docs)]
pub mod kvs {
    include!(concat!(env!("OUT_DIR"), "/kvs.rs"));
}

// Include the `causal` module, which is generated from causal.proto.
#[allow(warnings, missing_docs)]
pub mod causal {
    include!(concat!(env!("OUT_DIR"), "/causal.rs"));
}

// Include the `metadata` module, which is generated from metadata.proto
// and benchmark.proto (both have no package declaration, so prost merges
// them into _.rs).
#[allow(warnings, missing_docs)]
pub mod metadata {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}
