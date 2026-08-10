//! Anna KVS server (Rust port).
//!
//! Usage: `anna-kvs-rs --config <config.yml>`

mod context;
mod handlers;
mod kvs_server;
mod storage;

use anna_server_common::config::Config;
use anna_server_common::metadata::Tier;
use anna_server_common::signal;
use clap::Parser;

#[derive(Parser)]
#[command(name = "anna-kvs-rs", about = "Anna KVS server (Rust)")]
struct Cli {
    #[arg(long)]
    config: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let config = Config::load(std::path::Path::new(&cli.config)).unwrap_or_else(|e| {
        eprintln!("Failed to load config {}: {}", cli.config, e);
        std::process::exit(1);
    });

    // Determine tier from SERVER_TYPE env var (default: memory).
    let self_tier = match std::env::var("SERVER_TYPE")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "disk" => Tier::Disk,
        _ => Tier::Memory,
    };

    let thread_count = match self_tier {
        Tier::Memory => config.threads.memory,
        Tier::Disk => config.threads.disk,
        _ => 1,
    };

    let public_ip = config.server.public_ip.clone();
    let private_ip = config.server.private_ip.clone();
    let seed_ip = config.server.seed_ip.clone();
    let routing_ips = config.server.routing.clone();
    let monitoring_ips = config.server.monitoring.clone();

    if self_tier == Tier::Memory {
        log::info!(
            "Starting anna-kvs-rs (memory tier, {} threads)",
            thread_count
        );
    } else {
        log::info!("Starting anna-kvs-rs (disk tier, {} threads)", thread_count);
    }

    signal::install_shutdown_handler();

    // Spawn worker threads 1..N as tokio tasks.
    let mut handles = Vec::new();
    for tid in 1..thread_count {
        let config = config.clone();
        let public_ip = public_ip.clone();
        let private_ip = private_ip.clone();
        let seed_ip = seed_ip.clone();
        let routing_ips = routing_ips.clone();
        let monitoring_ips = monitoring_ips.clone();
        handles.push(tokio::spawn(async move {
            kvs_server::run(
                tid,
                &config,
                &public_ip,
                &private_ip,
                &seed_ip,
                self_tier,
                thread_count,
                0,
                routing_ips,
                monitoring_ips,
            )
            .await;
        }));
    }

    // Thread 0 runs on the main task.
    kvs_server::run(
        0,
        &config,
        &public_ip,
        &private_ip,
        &seed_ip,
        self_tier,
        thread_count,
        0,
        routing_ips,
        monitoring_ips,
    )
    .await;

    // Wait for worker threads.
    for handle in handles {
        let _ = handle.await;
    }
}
