#![warn(clippy::unwrap_used)]

//! This is the rust `anna` Library for working with the `anna` key-value store. It is linked into
//! the `anna` CLI binary but can also be used by others to create new binaries
//!
#[macro_use]
extern crate error_chain;

use nix::unistd::Pid;
use sysinfo::{ProcessExt, System, SystemExt};
use nix::sys::signal::{kill, Signal};
use std::path::PathBuf;
// use nix::sys::socket::bind;
use std::process::Command;
use crate::config::Config;

pub mod info;
mod proto;
mod kvs_client;
pub mod config;

// Pending them being defined elsewhere in a build script or similar
const ANNA_MONITOR_PROCESS_NAME: &str = "anna-monitor";
const ANNA_ROUTE_PROCESS_NAME: &str = "anna-route";
const ANNA_KVS_PROCESS_NAME: &str = "anna-kvs";
const PROCESS_LIST: [&str;3] = [ ANNA_MONITOR_PROCESS_NAME, ANNA_ROUTE_PROCESS_NAME, ANNA_KVS_PROCESS_NAME];
const BINARY_FOLDER: &str = "build/target/kvs";

// We'll put our errors in an `errors` module, and other modules in this crate will
// `use crate::errors::*;` to get access to everything `error_chain!` creates.
#[doc(hidden)]
pub mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

pub use errors::*;

#[doc(hidden)]
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
    s.get_process_by_name(name).iter().map(|p| p.pid()).collect()
}

fn project_root() -> Result<PathBuf> {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    Ok(root)
}

/// `start` the KVS processes in background
///
/// It returns a Result<usize> with the number of processes terminated
pub fn start(_config: &Config) -> Result<usize> {
    let bin_dir = project_root()?.join(BINARY_FOLDER);

    let mut process_count = 0;
    for process_name in PROCESS_LIST.iter() {
        if pids_from_name(process_name).is_empty() {
            if Command::new(bin_dir.join(process_name)).spawn().is_ok() {
                process_count += 1;
            }
        }
    }

    Ok(process_count)
}

/// `stop` function terminates the running processes for `anna-kvs`, `anna-monitor` and `anna-route`
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

pub fn get(tokens: &[&str]) {
    println!("GET: {:?}", tokens);
    //     client->get_async(v[1]);
//
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     if (responses.size() > 1) {
//       std::cout << "Error: received more than one response" << std::endl;
//     }
//
//     assert(responses[0].tuples(0).lattice_type() == LatticeType::LWW);
//
//     LWWPairLattice<string> lww_lattice =
//         deserialize_lww(responses[0].tuples(0).payload());
//     std::cout << lww_lattice.reveal().value << std::endl;
//   } else if (v[0] == "GET_CAUSAL") {
//     // currently this mode is only for testing purpose
//     client->get_async(v[1]);
//
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     if (responses.size() > 1) {
//       std::cout << "Error: received more than one response" << std::endl;
//     }
//
//     assert(responses[0].tuples(0).lattice_type() == LatticeType::MULTI_CAUSAL);
//
//     MultiKeyCausalLattice<SetLattice<string>> mkcl =
//         MultiKeyCausalLattice<SetLattice<string>>(to_multi_key_causal_payload(
//             deserialize_multi_key_causal(responses[0].tuples(0).payload())));
//
//     for (const auto &pair : mkcl.reveal().vector_clock.reveal()) {
//       std::cout << "{" << pair.first << " : "
//                 << std::to_string(pair.second.reveal()) << "}" << std::endl;
//     }
//
//     for (const auto &dep_key_vc_pair : mkcl.reveal().dependencies.reveal()) {
//       std::cout << dep_key_vc_pair.first << " : ";
//       for (const auto &vc_pair : dep_key_vc_pair.second.reveal()) {
//         std::cout << "{" << vc_pair.first << " : "
//                   << std::to_string(vc_pair.second.reveal()) << "}"
//                   << std::endl;
//       }
//     }
//
//     std::cout << *(mkcl.reveal().value.reveal().begin()) << std::endl;
}

pub fn get_causal(tokens: &[&str]) {
    println!("GET_CAUSAL: {:?}", tokens);
}

pub fn put(tokens: &[&str]) {
    println!("PUT: {:?}", tokens);
    //     Key key = v[1];
//     LWWPairLattice<string> val(
//         TimestampValuePair<string>(generate_timestamp(0), v[2]));
//
//     // Put async
//     string rid = client->put_async(key, serialize(val), LatticeType::LWW);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
}

pub fn put_causal(tokens: &[&str]) {
    println!("PUT_CAUSAL: {:?}", tokens);
    //     // currently this mode is only for testing purpose
//     Key key = v[1];
//
//     MultiKeyCausalPayload<SetLattice<string>> mkcp;
//     // construct a test client id - version pair
//     mkcp.vector_clock.insert("test", 1);
//
//     // construct one test dependencies
//     mkcp.dependencies.insert(
//         "dep1", VectorClock(map<string, MaxLattice<unsigned>>({{"test1", 1}})));
//
//     // populate the value
//     mkcp.value.insert(v[2]);
//
//     MultiKeyCausalLattice<SetLattice<string>> mkcl(mkcp);
//
//     // Put async
//     string rid = client->put_async(key, serialize(mkcl), LatticeType::MULTI_CAUSAL);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
}

pub fn put_set(tokens: &[&str]) {
    println!("PUT SET: {:?}", tokens);
    //     set<string> set;
//     for (int i = 2; i < v.size(); i++) {
//       set.insert(v[i]);
//     }
//
//     // Put async
//     string rid = client->put_async(v[1], serialize(SetLattice<string>(set)),
//                                    LatticeType::SET);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
}

pub fn get_set(tokens: &[&str]) {
    println!("GET SET: {:?}", tokens);
    //     // Get Async
//     client->get_async(v[1]);
//     string serialized;
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     SetLattice<string> latt = deserialize_set(responses[0].tuples(0).payload());
//     print_set(latt.reveal());
//   } else {
//     std::cout << "Unrecognized command " << v[0]
//               << ". Valid commands are GET, GET_SET, PUT, PUT_SET, PUT_CAUSAL, "
//               << "and GET_CAUSAL." << std::endl;
//     ;
//   }
}

#[cfg(test)]
mod test {
    #[test]
    fn no_such_process_to_stop() {
        assert_eq!(super::stop().expect("Expected zero processes killed"), 0);
    }
}