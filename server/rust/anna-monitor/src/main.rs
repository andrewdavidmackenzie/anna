//! Anna KVS monitoring server (Rust port).
//!
//! Wire-compatible with the C++ anna-monitor. Maintains cluster
//! membership, collects stats from KVS nodes, detects crashes,
//! and runs policy engines (storage, movement, SLO).

mod handlers;
mod monitor;
mod policies;
mod stats;
mod types;

use anna_server_common::config::Config;
use clap::Parser;
use log::info;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anna-monitor-rs", about = "Anna KVS monitoring server")]
struct Args {
    /// Path to the anna config file.
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();

    let config = Config::load(&args.config).unwrap_or_else(|e| {
        eprintln!("Failed to load config {:?}: {}", args.config, e);
        std::process::exit(1);
    });

    info!(
        "Starting anna-monitor-rs (base_offset={})",
        config.ports.base_offset
    );

    if let Err(e) = monitor::run(config).await {
        eprintln!("Monitor error: {}", e);
        std::process::exit(1);
    }
}
