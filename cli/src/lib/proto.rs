// Include the `shared` module, which is generated from shared.proto.
pub mod shared {
    include!(concat!(env!("OUT_DIR"), "/shared.rs"));
}

// Include the `anna` module, which is generated from anna.proto.
pub mod anna {
    include!(concat!(env!("OUT_DIR"), "/anna.rs"));
}

// Include the `cloudburst` module, which is generated from cloudburst.proto.
pub mod cloudburst {
    include!(concat!(env!("OUT_DIR"), "/cloudburst.rs"));
}

// Include the `causal` module, which is generated from causal.proto.
pub mod causal {
    include!(concat!(env!("OUT_DIR"), "/causal.rs"));
}
