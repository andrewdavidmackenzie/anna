use std::io;

const PROTO_FILES: &[&str] = &[
    "shared.proto",
    "anna.proto",
    "cloudburst.proto",
    "causal.proto",
    ];

fn main() -> io::Result<()> {
    prost_build::compile_protos(PROTO_FILES, &["src/lib/proto/"])?;

    Ok(())
}