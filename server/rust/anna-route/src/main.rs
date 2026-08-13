//! Anna routing server (Rust port).
//!
//! Usage: `anna-route-rs --config <config.yml>`

mod context;
mod handlers;
mod route_server;

use anna_server_common::config::Config;
use anna_server_common::signal;
use clap::Parser;

#[derive(Parser)]
#[command(name = "anna-route-rs", about = "Anna routing server (Rust)")]
struct Cli {
    #[arg(long)]
    config: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let mut config = Config::load(std::path::Path::new(&cli.config)).unwrap_or_else(|e| {
        eprintln!("Failed to load config {}: {}", cli.config, e);
        std::process::exit(1);
    });

    config.threads.resolve_auto();

    let thread_count = config.threads.routing;
    let ip = config.routing.ip.clone();
    let monitoring_ips = config.routing.monitoring.clone();

    log::info!("Starting anna-route-rs ({} threads)", thread_count);

    signal::install_shutdown_handler();

    let mut handles = Vec::new();
    for tid in 1..thread_count {
        let config = config.clone();
        let ip = ip.clone();
        let monitoring_ips = monitoring_ips.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = route_server::run(tid, &config, &ip, thread_count, monitoring_ips).await
            {
                log::error!("Worker thread {} failed: {}", tid, e);
                std::process::exit(1);
            }
        }));
    }

    if let Err(e) = route_server::run(0, &config, &ip, thread_count, monitoring_ips).await {
        log::error!("Thread 0 failed: {}", e);
        std::process::exit(1);
    }

    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("Worker task panicked: {}", e);
            std::process::exit(1);
        }
    }
}
