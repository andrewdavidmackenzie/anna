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

// Include the `cloudburst` module, which is generated from cloudburst.proto.
#[allow(warnings, missing_docs)]
pub mod cloudburst {
    include!(concat!(env!("OUT_DIR"), "/cloudburst.rs"));
}

// Include the `causal` module, which is generated from causal.proto.
#[allow(warnings, missing_docs)]
pub mod causal {
    include!(concat!(env!("OUT_DIR"), "/causal.rs"));
}
