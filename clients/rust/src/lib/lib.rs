#![warn(clippy::unwrap_used)]
#![deny(missing_docs)]

//! The `annalib` crate provides a Rust client for the Anna key-value store.
//!
//! # Quick Start
//!
//! ```rust
//! # #[tokio::main]
//! # async fn main() {
//! use annalib::config::Config;
//! use annalib::kvs_client::KVSClient;
//!
//! let config = Config::default();
//! let client = KVSClient::new(&config, Some(106)).await;
//! // Use client.get("key") and client.put("key", "value") with a running server
//! # }
//! ```
//!
//! # Process Management
//!
//! ```rust
//! // Check status (no server needed)
//! let status = annalib::status()?;
//! for (name, pids) in &status {
//!     if pids.is_empty() {
//!         println!("{} is not running", name);
//!     }
//! }
//! # Ok::<(), annalib::Error>(())
//! ```

#[cfg(unix)]
use nix::sys::signal::kill;
#[cfg(unix)]
use nix::unistd::Pid;
use std::path::Path;
use std::process::Command;
use sysinfo::System;

/// Cache subscription client for receiving gossip-pushed key updates.
pub mod cache_client;
/// Tab-completion for the anna CLI.
pub mod completer;
/// `config` of anna - read from config file or created via API calls.
pub mod config;
/// put all error types and methods into an `errors` module
mod errors;
/// `info` module provides additional information about this anna client and server components running
pub mod info;
/// `kvs_client` connects to key-value-store server to perform operations
pub mod kvs_client;
/// `proto` module holds definition of protobufs for communication between client and server
pub mod proto;
/// `threads` provides helper methods related to anna threads
pub mod threads;
/// Types used by KVS
pub mod types;

// Pending them being defined elsewhere in a build script or similar
const ANNA_MONITOR_PROCESS_NAME: &str = "anna-monitor";
const ANNA_ROUTE_PROCESS_NAME: &str = "anna-route";
const ANNA_KVS_PROCESS_NAME: &str = "anna-kvs";
const PROCESS_LIST: [&str; 3] = [
    ANNA_MONITOR_PROCESS_NAME,
    ANNA_ROUTE_PROCESS_NAME,
    ANNA_KVS_PROCESS_NAME,
];

pub use errors::{Error, Result};

/*
   Gather a list of pids that are running for a process using the process name
*/
fn pids_from_name(name: &str) -> Vec<i32> {
    let s = System::new_all();
    s.processes_by_name(name.as_ref())
        .map(|process| process.pid().as_u32() as i32)
        .collect()
}

/// Start the anna server processes (`anna-monitor`, `anna-route`, `anna-kvs`).
///
/// Returns the number of processes started. Fails if any process is already running.
///
/// ```rust
/// use std::path::Path;
/// // Requires anna-monitor, anna-route, anna-kvs binaries in PATH
/// if let Ok(count) = annalib::start(Path::new("anna-config.yml")) {
///     println!("{} processes started", count);
///     annalib::stop().ok();
/// }
/// ```
pub fn start(config_file_path: &Path) -> Result<usize> {
    let mut process_count = 0;
    for process_name in PROCESS_LIST.iter() {
        let pids = pids_from_name(process_name);
        if !pids.is_empty() {
            return Err(Error::Process(format!(
                "Process '{}' is already running with pids = {:?}",
                process_name, pids
            )));
        }

        let config_str = config_file_path
            .to_str()
            .ok_or_else(|| Error::Process("Could not get config file path".into()))?;

        Command::new(process_name)
            .args(["--config", config_str])
            .spawn()
            .map_err(|e| Error::Process(format!("Failed to spawn '{}': {}", process_name, e)))?;

        process_count += 1;
    }

    Ok(process_count)
}

/// Get the running status of each anna server process.
///
/// Returns a list of `(process_name, pids)` tuples.
///
/// ```rust
/// let status = annalib::status()?;
/// for (name, pids) in &status {
///     if pids.is_empty() {
///         println!("{} is not running", name);
///     } else {
///         println!("{} running with pids {:?}", name, pids);
///     }
/// }
/// # Ok::<(), annalib::Error>(())
/// ```
pub fn status() -> Result<Vec<(String, Vec<i32>)>> {
    let mut status = vec![];

    for process_name in PROCESS_LIST.iter() {
        let pids = pids_from_name(process_name);
        status.push((process_name.to_string(), pids));
    }

    Ok(status)
}

/// Stop all running anna server processes via SIGTERM.
///
/// Returns the number of processes terminated.
///
/// ```rust
/// let count = annalib::stop()?;
/// assert_eq!(count, 0); // no anna processes running during test
/// # Ok::<(), annalib::Error>(())
/// ```
#[cfg(unix)]
pub fn stop() -> Result<usize> {
    let mut kill_count: usize = 0;
    for process_name in PROCESS_LIST.iter() {
        for pid in pids_from_name(process_name) {
            if kill(Pid::from_raw(pid), Some(nix::sys::signal::Signal::SIGTERM)).is_ok() {
                kill_count += 1;
            }
        }
    }

    Ok(kill_count)
}

/// `stop` is a no-op stub on Windows (process termination not yet implemented)
///
/// It always returns `Ok(0)`.
#[cfg(windows)]
pub fn stop() -> Result<usize> {
    Ok(0)
}

#[cfg(test)]
mod test {
    #[test]
    fn no_such_process_to_stop() {
        assert_eq!(super::stop().expect("stop failed"), 0);
    }

    #[test]
    fn status_with_nothing_running() {
        let status = super::status().expect("status failed");
        assert_eq!(status.len(), 3);
        for (name, pids) in &status {
            assert!(
                pids.is_empty(),
                "Expected no pids for '{}', got {:?}",
                name,
                pids
            );
        }
    }

    #[test]
    fn status_returns_process_names() {
        let status = super::status().expect("status failed");
        let names: Vec<&str> = status.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"anna-monitor"));
        assert!(names.contains(&"anna-route"));
        assert!(names.contains(&"anna-kvs"));
    }

    #[test]
    fn pids_from_name_nonexistent() {
        let pids = super::pids_from_name("nonexistent_process_xyz_12345");
        assert!(pids.is_empty());
    }
}
