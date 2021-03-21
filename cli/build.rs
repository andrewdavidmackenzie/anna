use std::io;

const PROTO_FILES: &[&str] = &[
    "shared.proto",
    "anna.proto",
    "cloudburst.proto",
    "causal.proto",
    ];

fn main() -> io::Result<()> {
    // Rust code generation for protobuf definitions
    prost_build::compile_protos(PROTO_FILES, &["src/lib/proto/"])
}