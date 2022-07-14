#![warn(clippy::unwrap_used)]
#![deny(missing_docs)]

//! This is the rust `anna` Library for working with the `anna` key-value store. It is linked into
//! the `anna` CLI binary but can also be used by others to create new binaries

use nix::sys::signal::kill;
use nix::unistd::Pid;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{ProcessExt, System, SystemExt};

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

pub use errors::*;

/*
   Gather a list of pids that are running for a process using the process name
*/
fn pids_from_name(name: &str) -> Vec<i32> {
    let s = System::new_all();
    s.processes_by_name(name)
        .map(|process| process.pid().into())
        .collect()
}

/// `start` function starts the processes `anna-kvs`, `anna-monitor` and `anna-route`
///
/// It returns a Result<usize> with the number of processes started
pub fn start(config_file_path: &PathBuf) -> Result<usize> {
    let mut process_count = 0;
    for process_name in PROCESS_LIST.iter() {
        let pids = pids_from_name(process_name);
        if !pids.is_empty() {
            bail!(
                "Process '{}' is already running with pids = {:?}",
                process_count,
                pids
            )
        }

        Command::new(process_name)
            .args([
                "--config",
                config_file_path
                    .to_str()
                    .ok_or("Could not get config file path")?,
            ])
            .spawn()
            .chain_err(|| format!("Failed to spawn process '{}'", process_name))?;

        process_count += 1;
    }

    Ok(process_count)
}

/// Return a String representing the status of the anna processes
pub fn status() -> Result<Vec<(String, Vec<i32>)>> {
    let mut status = vec![];

    for process_name in PROCESS_LIST.iter() {
        let pids = pids_from_name(process_name);
        status.push((process_name.to_string(), pids));
    }

    Ok(status)
}

/// `stop` function terminates the processes `anna-kvs`, `anna-monitor` and `anna-route`
///
/// It returns a Result<usize> with the number of processes terminated
pub fn stop() -> Result<usize> {
    let mut kill_count: usize = 0;
    for process_name in PROCESS_LIST.iter() {
        for pid in pids_from_name(process_name) {
            if kill(Pid::from_raw(pid), Some(nix::sys::signal::SIGTERM)).is_ok() {
                kill_count += 1;
            }
        }
    }

    Ok(kill_count)
}

#[cfg(test)]
mod test {
    #[test]
    fn no_such_process_to_stop() {
        let _ = super::stop();

        assert_eq!(
            super::stop().expect("Expected zero processes killed"),
            0,
            "Expected zero processes killed"
        );
    }
}
