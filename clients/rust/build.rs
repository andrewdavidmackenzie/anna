use std::io;

const PROTO_FILES: &[&str] = &[
    "shared.proto",
    "kvs.proto",
    "causal.proto",
    "metadata.proto",
    "benchmark.proto",
];

fn main() -> io::Result<()> {
    // Rust code generation for protobuf definitions
    prost_build::compile_protos(PROTO_FILES, &["../../server/protobuf/"])
}
