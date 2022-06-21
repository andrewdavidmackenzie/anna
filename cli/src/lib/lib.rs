#![warn(clippy::unwrap_used)]

//! This is the rust `anna` Library for working with the `anna` key-value store. It is linked into
//! the `anna` CLI binary but can also be used by others to create new binaries
#[macro_use]
extern crate error_chain;

use crate::config::Config;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{ProcessExt, System, SystemExt};

pub mod config;
pub mod info;
pub mod kvs_client;
mod proto;
mod threads;

// Pending them being defined elsewhere in a build script or similar
const ANNA_MONITOR_PROCESS_NAME: &str = "anna-monitor";
const ANNA_ROUTE_PROCESS_NAME: &str = "anna-route";
const ANNA_KVS_PROCESS_NAME: &str = "anna-kvs";
const PROCESS_LIST: [&str; 3] = [
    ANNA_MONITOR_PROCESS_NAME,
    ANNA_ROUTE_PROCESS_NAME,
    ANNA_KVS_PROCESS_NAME,
];
const BINARY_FOLDER: &str = "build/target/kvs";

// We'll put our errors in an `errors` module, and other modules in this crate will
// `use crate::errors::*;` to get access to everything `error_chain!` creates.
#[doc(hidden)]
pub mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

pub use errors::*;

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Serde(serde_yaml::Error);
    }
}

/*
   Gather a list of pids that are running for a process using the process name
*/
fn pids_from_name(name: &str) -> Vec<i32> {
    let s = System::new_all();
    s.get_process_by_name(name)
        .iter()
        .map(|p| p.pid())
        .collect()
}

fn project_root() -> Result<PathBuf> {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    Ok(root)
}

/// `start` function starts the processes `anna-kvs`, `anna-monitor` and `anna-route`
///
/// It returns a Result<usize> with the number of processes started
pub fn start(_config: &Config) -> Result<usize> {
    let bin_dir = project_root()?.join(BINARY_FOLDER);

    let mut process_count = 0;
    for process_name in PROCESS_LIST.iter() {
        if pids_from_name(process_name).is_empty()
            && Command::new(bin_dir.join(process_name)).spawn().is_ok()
        {
            process_count += 1;
        }
    }

    Ok(process_count)
}

/// `stop` function terminates the processes `anna-kvs`, `anna-monitor` and `anna-route`
///
/// It returns a Result<usize> with the number of processes terminated
pub fn stop() -> Result<usize> {
    let mut kill_count: usize = 0;
    for process_name in PROCESS_LIST.iter() {
        for pid in pids_from_name(process_name) {
            if kill(Pid::from_raw(pid), Some(Signal::SIGTERM)).is_ok() {
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
