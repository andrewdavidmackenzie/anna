use std::io;

const PROTO_FILES: &[&str] = &[
    "shared.proto",
    "kvs.proto",
    "causal.proto",
    "metadata.proto",
    "benchmark.proto",
];

fn main() -> io::Result<()> {
    prost_build::compile_protos(PROTO_FILES, &["../../protobuf/"])
}
