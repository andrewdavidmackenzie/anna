#![warn(clippy::unwrap_used)]
#![deny(missing_docs)]

//! The `annalib` crate provides a Rust client for the Anna key-value store.
//!
//! # Quick Start
//!
//! ```rust
//! # #[tokio::main]
//! # async fn main() {
//! use annalib::client_config::ClientConfig;
//! use annalib::kvs_client::KVSClient;
//!
//! let config = ClientConfig::default();
//! let client = KVSClient::new(&config, Some(106)).await;
//! // Use client.get("key") and client.put("key", "value") with a running server
//! # }
//! ```
//!
//! # Process Management
//!
//! ```rust
//! // Check status of all components (no server needed)
//! let status = annalib::status(&[])?;
//! for (name, pids) in &status {
//!     if pids.is_empty() {
//!         println!("{} is not running", name);
//!     }
//! }
//!
//! // Check status of a single component
//! use annalib::Component;
//! let status = annalib::status(&[Component::Kvs])?;
//! # Ok::<(), annalib::Error>(())
//! ```

#[cfg(unix)]
use nix::sys::signal::kill;
#[cfg(unix)]
use nix::unistd::Pid;
use std::fmt;
use std::path::Path;
use std::process::Command;
use sysinfo::System;

/// Minimal client-side configuration for connecting to an anna cluster.
/// Benchmark infrastructure for measuring KVS throughput and latency.
pub mod bench;

/// Minimal client-side configuration for connecting to an anna cluster.
pub mod client_config;
/// Tab-completion for the anna CLI.
pub mod completer;
/// put all error types and methods into an `errors` module
mod errors;
/// `info` module provides additional information about this anna client and server components running
pub mod info;
/// `kvs_client` connects to key-value-store server to perform operations
pub mod kvs_client;
/// Reports client-observed latency to the monitor for SLO enforcement.
pub mod latency_reporter;
/// `proto` module holds definition of protobufs for communication between client and server
pub mod proto;
/// `threads` provides helper methods related to anna threads
pub mod threads;
/// Client-side transactions for Read Committed and Item Cut Isolation.
pub mod transaction;
/// Types used by KVS
pub mod types;
/// Subscribe to value changes for specific keys via the KVS gossip mechanism.
pub mod value_change_subscriber;

// Pending them being defined elsewhere in a build script or similar
const ANNA_MONITOR_PROCESS_NAME: &str = "anna-monitor";
const ANNA_ROUTE_PROCESS_NAME: &str = "anna-route";
const ANNA_KVS_PROCESS_NAME: &str = "anna-kvs";

pub use errors::{Error, Result};

/// An anna server component that can be started, stopped, or queried for status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// The monitoring daemon (`anna-monitor`).
    Monitor,
    /// The routing tier (`anna-route`).
    Route,
    /// The key-value store server (`anna-kvs`).
    Kvs,
}

/// All components in their canonical start order.
pub const ALL_COMPONENTS: [Component; 3] = [Component::Monitor, Component::Route, Component::Kvs];

/// The short names accepted on the command line (in the same order as [`ALL_COMPONENTS`]).
pub const COMPONENT_NAMES: [&str; 3] = ["monitor", "route", "kvs"];

impl Component {
    /// Return the binary/process name for this component.
    pub fn process_name(self) -> &'static str {
        match self {
            Component::Monitor => ANNA_MONITOR_PROCESS_NAME,
            Component::Route => ANNA_ROUTE_PROCESS_NAME,
            Component::Kvs => ANNA_KVS_PROCESS_NAME,
        }
    }

    /// Parse a short name (`kvs`, `monitor`, `route`) into a [`Component`].
    ///
    /// Returns `None` if the name is not recognized.
    pub fn from_name(name: &str) -> Option<Component> {
        match name.to_ascii_lowercase().as_str() {
            "monitor" => Some(Component::Monitor),
            "route" => Some(Component::Route),
            "kvs" => Some(Component::Kvs),
            _ => None,
        }
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Component::Monitor => write!(f, "monitor"),
            Component::Route => write!(f, "route"),
            Component::Kvs => write!(f, "kvs"),
        }
    }
}

/// Return a deduplicated, order-preserving list of components.
///
/// If `components` is empty, returns all components in canonical order.
fn resolve_targets(components: &[Component]) -> Vec<Component> {
    if components.is_empty() {
        return ALL_COMPONENTS.to_vec();
    }
    let mut seen = [false; 3]; // indexed by Component discriminant
    let mut targets = Vec::with_capacity(components.len());
    for &c in components {
        let idx = c as usize;
        if !seen[idx] {
            seen[idx] = true;
            targets.push(c);
        }
    }
    targets
}

/*
   Gather a list of pids that are running for a process using the process name
*/
fn pids_from_name(name: &str) -> Vec<i32> {
    let s = System::new_all();
    s.processes_by_name(name.as_ref())
        .map(|process| process.pid().as_u32() as i32)
        .collect()
}

/// Start the given anna server components (or all if the slice is empty).
///
/// Returns the number of processes started. Fails if any of the requested
/// processes is already running.
///
/// ```rust,no_run
/// use std::path::Path;
/// use annalib::Component;
/// // Requires anna-monitor, anna-route, anna-kvs binaries in PATH
/// if let Ok(count) = annalib::start(Path::new("anna-config.yml"), &[]) {
///     println!("{} processes started", count);
///     annalib::stop(&[]).ok();
/// }
/// ```
pub fn start(config_file_path: &Path, components: &[Component]) -> Result<usize> {
    let targets = resolve_targets(components);

    let mut process_count = 0;
    for component in &targets {
        let process_name = component.process_name();
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

/// Get the running status of the given anna server components (or all if the
/// slice is empty).
///
/// Returns a list of `(process_name, pids)` tuples.
///
/// ```rust
/// let status = annalib::status(&[])?;
/// for (name, pids) in &status {
///     if pids.is_empty() {
///         println!("{} is not running", name);
///     } else {
///         println!("{} running with pids {:?}", name, pids);
///     }
/// }
/// # Ok::<(), annalib::Error>(())
/// ```
pub fn status(components: &[Component]) -> Result<Vec<(String, Vec<i32>)>> {
    let targets = resolve_targets(components);

    let mut status = vec![];

    for component in &targets {
        let process_name = component.process_name();
        let pids = pids_from_name(process_name);
        status.push((process_name.to_string(), pids));
    }

    Ok(status)
}

/// Stop the given anna server components via SIGTERM (or all if the slice is
/// empty).
///
/// Returns the number of processes terminated.
///
/// ```rust
/// let count = annalib::stop(&[])?;
/// assert_eq!(count, 0); // no anna processes running during test
/// # Ok::<(), annalib::Error>(())
/// ```
#[cfg(unix)]
pub fn stop(components: &[Component]) -> Result<usize> {
    let targets = resolve_targets(components);

    let mut kill_count: usize = 0;
    for component in &targets {
        let process_name = component.process_name();
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
pub fn stop(_components: &[Component]) -> Result<usize> {
    Ok(0)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn no_such_process_to_stop() {
        assert_eq!(stop(&[]).expect("stop failed"), 0);
    }

    #[test]
    fn stop_single_component() {
        assert_eq!(stop(&[Component::Kvs]).expect("stop failed"), 0);
    }

    #[test]
    fn status_with_nothing_running() {
        let status = status(&[]).expect("status failed");
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
    fn status_single_component() {
        let status = status(&[Component::Kvs]).expect("status failed");
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0, "anna-kvs");
    }

    #[test]
    fn status_returns_process_names() {
        let status = status(&[]).expect("status failed");
        let names: Vec<&str> = status.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"anna-monitor"));
        assert!(names.contains(&"anna-route"));
        assert!(names.contains(&"anna-kvs"));
    }

    #[test]
    fn pids_from_name_nonexistent() {
        let pids = pids_from_name("nonexistent_process_xyz_12345");
        assert!(pids.is_empty());
    }

    #[test]
    fn component_from_name() {
        assert_eq!(Component::from_name("kvs"), Some(Component::Kvs));
        assert_eq!(Component::from_name("monitor"), Some(Component::Monitor));
        assert_eq!(Component::from_name("route"), Some(Component::Route));
        assert_eq!(Component::from_name("KVS"), Some(Component::Kvs));
        assert_eq!(Component::from_name("unknown"), None);
    }

    #[test]
    fn component_process_name() {
        assert_eq!(Component::Kvs.process_name(), "anna-kvs");
        assert_eq!(Component::Monitor.process_name(), "anna-monitor");
        assert_eq!(Component::Route.process_name(), "anna-route");
    }

    #[test]
    fn component_display() {
        assert_eq!(format!("{}", Component::Kvs), "kvs");
        assert_eq!(format!("{}", Component::Monitor), "monitor");
        assert_eq!(format!("{}", Component::Route), "route");
    }

    #[test]
    fn resolve_targets_empty_returns_all() {
        let targets = resolve_targets(&[]);
        assert_eq!(targets, ALL_COMPONENTS.to_vec());
    }

    #[test]
    fn resolve_targets_deduplicates() {
        let targets = resolve_targets(&[Component::Kvs, Component::Kvs]);
        assert_eq!(targets, vec![Component::Kvs]);
    }

    #[test]
    fn resolve_targets_preserves_order() {
        let targets = resolve_targets(&[Component::Kvs, Component::Monitor, Component::Kvs]);
        assert_eq!(targets, vec![Component::Kvs, Component::Monitor]);
    }

    #[test]
    fn status_deduplicates_components() {
        let status = status(&[Component::Kvs, Component::Kvs]).expect("status failed");
        assert_eq!(status.len(), 1);
    }

    #[test]
    fn stop_deduplicates_components() {
        assert_eq!(
            stop(&[Component::Kvs, Component::Kvs]).expect("stop failed"),
            0
        );
    }
}
