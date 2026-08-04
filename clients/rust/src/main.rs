#![warn(clippy::unwrap_used)]
//! `anna` is a command line tool for working with the `anna` key-value store
//!
//! Execute `anna` or `anna --help` or `anna -h` at the comment line for a
//! description of the command line options.

use std::process::exit;
use std::time::Duration;

use annalib::{
    client_config::ClientConfig, completer::AnnaCompleter, info, kvs_client::KVSClient, start,
    status, stop, Component, COMPONENT_NAMES,
};
use clap::{Arg, ArgMatches, Command};
use log::{debug, error};
use rustyline::Editor;
use simplog::SimpleLogger;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const ANNA_HISTORY_FILENAME: &str = ".anna_history";

/// `anna` CLI Error codes
/// 0 - Success
/// 1 - Config file error
/// 2 - Command line arguments error (from clap)
const SUCCESS: i32 = 0;

/// CLI-specific errors wrapping library and external errors.
#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Clap(#[from] clap::Error),
    #[error("{0}")]
    Anna(#[from] annalib::Error),
    #[error("{0}")]
    RustyLine(#[from] rustyline::error::ReadlineError),
    #[error("{0}")]
    Other(String),
}

type Result<T> = std::result::Result<T, CliError>;

#[tokio::main]
async fn main() {
    match run().await {
        Err(ref e) => {
            eprintln!("error: {}", e);
            exit(1);
        }
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{}", msg);
            }
            exit(SUCCESS)
        }
    }
}

fn get_client_config(args: &ArgMatches) -> ClientConfig {
    let routing: Vec<String> = args
        .get_many::<String>("routing")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_else(|| vec!["tcp://127.0.0.1:6450".to_string()]);
    let client_ip = args
        .get_one::<String>("client-ip")
        .cloned()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    ClientConfig {
        routing_addresses: routing,
        client_ip,
    }
}

fn get_server_config_path(args: &ArgMatches) -> Result<PathBuf> {
    let path = args
        .get_one::<String>("server-config")
        .ok_or_else(|| CliError::Other("--server-config is required for start/stop".into()))?;
    PathBuf::from(path)
        .canonicalize()
        .map_err(|e| CliError::Other(format!("Could not resolve server config '{}': {}", path, e)))
}

/// Parse the optional component name from the subcommand arguments.
///
/// Returns an empty vec if no component was specified (meaning "all"),
/// or a single-element vec with the parsed [`Component`].
fn parse_components(sub_matches: &ArgMatches) -> Result<Vec<Component>> {
    match sub_matches.get_one::<String>("component") {
        None => Ok(vec![]),
        Some(name) => Component::from_name(name).map(|c| vec![c]).ok_or_else(|| {
            CliError::Other(format!(
                "Unknown component '{}'. Valid components: {}",
                name,
                COMPONENT_NAMES.join(", ")
            ))
        }),
    }
}

/// Parse an optional component name from the interactive CLI input tokens.
///
/// `split[0]` is the command (START/STOP/STATUS); `split[1]` (if present) is the
/// component name.
fn parse_component_from_split(split: &[&str]) -> Result<Vec<Component>> {
    if split.len() > 2 {
        return Err(CliError::Other(
            "Expected at most one component argument".into(),
        ));
    }
    if split.len() <= 1 {
        return Ok(vec![]);
    }
    let name = split[1];
    Component::from_name(name).map(|c| vec![c]).ok_or_else(|| {
        CliError::Other(format!(
            "Unknown component '{}'. Valid components: {}",
            name,
            COMPONENT_NAMES.join(", ")
        ))
    })
}

async fn run() -> Result<String> {
    let app = get_app();
    let matches = app.get_matches();

    let verbosity = matches.get_one::<String>("verbosity").map(|s| s.as_str());
    SimpleLogger::init_prefix(verbosity, false);

    debug!(
        "'{}' CLI version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    debug!("'anna' library version {}", info::version());

    match matches
        .subcommand()
        .ok_or_else(|| CliError::Other("Could not find valid subcommand".into()))?
    {
        ("start", sub_matches) => {
            let components = parse_components(sub_matches)?;
            Ok(format!(
                "{} anna processes were started",
                start(&get_server_config_path(&matches)?, &components)?
            ))
        }
        ("status", sub_matches) => {
            let components = parse_components(sub_matches)?;
            Ok(format_status(status(&components)?))
        }
        ("stop", sub_matches) => {
            let components = parse_components(sub_matches)?;
            Ok(format!(
                "{} anna processes were terminated",
                stop(&components)?
            ))
        }
        ("cli", arg_matches) => {
            let config = get_client_config(&matches);
            let server_config_path = matches
                .get_one::<String>("server-config")
                .map(PathBuf::from)
                .unwrap_or_default();
            let client = KVSClient::new(&config, None).await;
            Ok(match arg_matches
                .get_one::<String>("command_file")
                .map(|s| s.as_str())
            {
                None => cli_loop_interactive(client, config, server_config_path).await?,
                Some(filename) => {
                    cli_loop_file(client, filename, config, server_config_path).await?
                }
            }
            .into())
        }
        ("bench", sub_matches) => {
            let config = get_client_config(&matches);
            let mut client = KVSClient::new(&config, None).await;
            {
                let config = bench_config_from_clap(sub_matches);
                annalib::bench::run_bench(&mut client, &config).await?;
            }
            Ok(String::new())
        }
        (_, _) => Ok("No command executed".into()),
    }
}

fn format_status(status: Vec<(String, Vec<i32>)>) -> String {
    let mut status_string = String::new();
    for (process_name, pids) in status {
        if pids.is_empty() {
            status_string = format!(
                "{} Process '{}' is not running\n",
                status_string, process_name
            );
        } else {
            status_string = format!(
                "{}{}' is running with pids = {:?}\n",
                status_string, process_name, pids
            );
        }
    }
    status_string
}

/// Build a [`Value`](annalib::value::Value) from CLI arguments for a PUT command.
///
/// The first argument after PUT may be a lattice type name (e.g., `set`,
/// `priority`, `causal`). If not recognized as a type, it is treated as
/// the key and LWW is assumed.
fn parse_put_args(split: &[&str]) -> Result<(String, annalib::value::Value)> {
    use annalib::value::{parse_type_name, Value};

    if split.len() < 3 {
        return Err(CliError::Other(
            "PUT requires at least a key and value".into(),
        ));
    }

    // Exactly 3 tokens: always LWW (preserves keys named "set", "priority", etc.)
    if split.len() == 3 {
        return Ok((split[1].to_string(), Value::Lww(split[2].into())));
    }

    // 4+ tokens: check if the first arg after PUT is a type name.
    if let Some(lt) = parse_type_name(split[1]) {
        use annalib::proto::kvs::LatticeType;
        if split.len() < 4 {
            return Err(CliError::Other(format!(
                "PUT {} requires a key and value(s)",
                split[1]
            )));
        }
        let key = split[2].to_string();
        let value = match lt {
            LatticeType::Lww => Value::Lww(split[3].into()),
            LatticeType::Set => Value::Set(split[3..].iter().map(|s| s.to_string()).collect()),
            LatticeType::OrderedSet => {
                Value::OrderedSet(split[3..].iter().map(|s| s.to_string()).collect())
            }
            LatticeType::LwwSet => {
                Value::LwwSet(split[3..].iter().map(|s| s.to_string()).collect())
            }
            LatticeType::LwwOrderedSet => {
                Value::LwwOrderedSet(split[3..].iter().map(|s| s.to_string()).collect())
            }
            LatticeType::UnionScalar => Value::UnionScalar(split[3].into()),
            LatticeType::PrioritySet => {
                if split.len() < 5 {
                    return Err(CliError::Other(
                        "PUT priority_set requires key, priority, and values".into(),
                    ));
                }
                let priority = split[3].parse::<f64>().map_err(|e| {
                    CliError::Other(format!("Invalid priority '{}': {}", split[3], e))
                })?;
                Value::PrioritySet {
                    priority,
                    values: split[4..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::PriorityOrderedSet => {
                if split.len() < 5 {
                    return Err(CliError::Other(
                        "PUT priority_ordered_set requires key, priority, and values".into(),
                    ));
                }
                let priority = split[3].parse::<f64>().map_err(|e| {
                    CliError::Other(format!("Invalid priority '{}': {}", split[3], e))
                })?;
                Value::PriorityOrderedSet {
                    priority,
                    values: split[4..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::CausalSet => {
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                Value::CausalSet {
                    vector_clock: vc,
                    values: split[3..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::CausalOrderedSet => {
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                Value::CausalOrderedSet {
                    vector_clock: vc,
                    values: split[3..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::MultiCausalSet => {
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                let mut dep_vc = std::collections::HashMap::new();
                dep_vc.insert("test1".to_string(), 1u32);
                Value::MultiCausalSet {
                    vector_clock: vc,
                    dependencies: vec![("dep1".into(), dep_vc)],
                    values: split[3..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::MultiCausalOrderedSet => {
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                let mut dep_vc = std::collections::HashMap::new();
                dep_vc.insert("test1".to_string(), 1u32);
                Value::MultiCausalOrderedSet {
                    vector_clock: vc,
                    dependencies: vec![("dep1".into(), dep_vc)],
                    values: split[3..].iter().map(|s| s.to_string()).collect(),
                }
            }
            LatticeType::Priority => {
                if split.len() < 5 {
                    return Err(CliError::Other(
                        "PUT priority requires key, priority, and value".into(),
                    ));
                }
                let priority = split[3].parse::<f64>().map_err(|e| {
                    CliError::Other(format!("Invalid priority '{}': {}", split[3], e))
                })?;
                Value::Priority {
                    priority,
                    value: split[4].into(),
                }
            }
            LatticeType::SingleCausal => {
                // Placeholder vector clock for CLI testing. A production
                // client would derive this from its causal context.
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                Value::SingleCausal {
                    vector_clock: vc,
                    values: vec![split[3].into()],
                }
            }
            LatticeType::MultiCausal => {
                // Placeholder vector clock and dependency for CLI testing.
                // A production client would derive these from its causal
                // context and tracked cross-key dependencies.
                let mut vc = std::collections::HashMap::new();
                vc.insert("test".to_string(), 1u32);
                let mut dep_vc = std::collections::HashMap::new();
                dep_vc.insert("test1".to_string(), 1u32);
                Value::MultiCausal {
                    vector_clock: vc,
                    dependencies: vec![("dep1".into(), dep_vc)],
                    values: vec![split[3].into()],
                }
            }
            _ => return Err(CliError::Other(format!("Unsupported type: {}", split[1]))),
        };
        Ok((key, value))
    } else {
        Err(CliError::Other(format!(
            "Unknown type '{}'. Valid types: lww, set, ordered_set, lww_set, lww_ordered_set, union, priority, causal, single_causal",
            split[1]
        )))
    }
}

/// Returns `true` if the user requested exit.
async fn execute_command(
    client: &mut KVSClient,
    line: &str,
    client_config: &ClientConfig,
    config_file_path: &Path,
) -> Result<bool> {
    let split = line.trim().split(' ').collect::<Vec<&str>>();

    match split[0].to_ascii_uppercase().as_str() {
        "GET" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "PUT" if split.len() >= 3 => {
            let (key, value) = parse_put_args(&split)?;
            client.put_value(&key, &value).await?;
        }
        "DEL" | "DELETE" if split.len() == 2 => client.delete(split[1]).await?,
        "SADD" if split.len() >= 3 => {
            for element in &split[2..] {
                client.or_set_add(split[1], element).await?;
            }
        }
        "SREM" if split.len() >= 3 => {
            for element in &split[2..] {
                client.or_set_remove(split[1], element).await?;
            }
        }
        "SMEMBERS" if split.len() == 2 => {
            let vals = client.get_or_set(split[1]).await?;
            println!("{}", vals.join(", "));
        }
        "MGET" if split.len() >= 2 => {
            let keys: Vec<&str> = split[1..].to_vec();
            let results = client.get_multi(&keys).await?;
            for (key, val) in results {
                println!("{}: {}", key, val);
            }
        }
        "MSET" if split.len() >= 3 && (split.len() - 1) % 2 == 0 => {
            let pairs: Vec<(&str, &str)> = split[1..].chunks(2).map(|c| (c[0], c[1])).collect();
            client.put_multi(&pairs).await?;
        }
        "INCR" if split.len() == 2 => {
            client.increment(split[1]).await?;
        }
        "INCR" if split.len() == 3 => {
            let amount: u64 = split[2]
                .parse()
                .map_err(|_| annalib::Error::Kvs("amount must be a positive integer".into()))?;
            client.increment_by(split[1], amount).await?;
        }
        "DECR" if split.len() == 2 => {
            client.decrement(split[1]).await?;
        }
        "DECR" if split.len() == 3 => {
            let amount: u64 = split[2]
                .parse()
                .map_err(|_| annalib::Error::Kvs("amount must be a positive integer".into()))?;
            client.decrement_by(split[1], amount).await?;
        }
        "GET_COUNTER" if split.len() == 2 => {
            let val = client.get_counter(split[1]).await?;
            println!("{}", val);
        }
        "SUBSCRIBE" if split.len() >= 2 => {
            use annalib::value_change_subscriber::ValueChangeSubscriber;
            let keys: Vec<String> = split[1..].iter().map(|s| s.to_string()).collect();
            let mut sub = ValueChangeSubscriber::new(client_config, None).await?;
            sub.watch(&keys).await?;
            println!("Subscribed to: {}", keys.join(", "));
            println!("Waiting for updates (Ctrl+C to stop)...");
            loop {
                match sub.recv_update(std::time::Duration::from_secs(1)).await {
                    Ok(Some((key, payload))) => {
                        let display = match ValueChangeSubscriber::decode_lww_value(&payload) {
                            Ok(bytes) => String::from_utf8(bytes)
                                .unwrap_or_else(|_| format!("({} raw bytes)", payload.len())),
                            Err(_) => format!("({} raw bytes)", payload.len()),
                        };
                        println!("{}: {}", key, display);
                    }
                    Ok(None) => continue,
                    Err(e) => {
                        error!("SUBSCRIBE error: {}", e);
                        break;
                    }
                }
            }
        }
        // Legacy aliases for renamed commands.
        "SET_ADD" if split.len() == 3 => {
            client.or_set_add(split[1], split[2]).await?;
        }
        "SET_REMOVE" if split.len() == 3 => {
            client.or_set_remove(split[1], split[2]).await?;
        }
        "GET_OR_SET" if split.len() == 2 => {
            let vals = client.get_or_set(split[1]).await?;
            println!("{}", vals.join(", "));
        }
        "PUT_MULTI" if split.len() >= 3 && (split.len() - 1) % 2 == 0 => {
            let pairs: Vec<(&str, &str)> = split[1..].chunks(2).map(|c| (c[0], c[1])).collect();
            client.put_multi(&pairs).await?;
        }
        "INCREMENT" if split.len() == 2 => {
            client.increment(split[1]).await?;
        }
        "INCREMENT" if split.len() == 3 => {
            let amount: u64 = split[2]
                .parse()
                .map_err(|_| annalib::Error::Kvs("amount must be a positive integer".into()))?;
            client.increment_by(split[1], amount).await?;
        }
        "DECREMENT" if split.len() == 2 => {
            client.decrement(split[1]).await?;
        }
        "DECREMENT" if split.len() == 3 => {
            let amount: u64 = split[2]
                .parse()
                .map_err(|_| annalib::Error::Kvs("amount must be a positive integer".into()))?;
            client.decrement_by(split[1], amount).await?;
        }
        "SCAN" => {
            let prefix = if split.len() >= 2 { split[1] } else { "" };
            let entries = client.scan(prefix).await?;
            if entries.is_empty() {
                println!("(no keys found)");
            } else {
                for entry in &entries {
                    let type_name = annalib::proto::kvs::LatticeType::try_from(entry.lattice_type)
                        .map(|t| format!("{:?}", t))
                        .unwrap_or_else(|_| format!("?{}", entry.lattice_type));
                    if entry.expiry_epoch_s > 0 {
                        println!(
                            "  {} [type={}, size={}, expiry={}]",
                            entry.key, type_name, entry.size, entry.expiry_epoch_s
                        );
                    } else {
                        println!("  {} [type={}, size={}]", entry.key, type_name, entry.size);
                    }
                }
                println!("({} keys)", entries.len());
            }
        }
        "EXPIRE" | "PUT_TTL" if split.len() == 4 => {
            let key = split[1];
            let value = split[2];
            let ttl: u32 = split[3]
                .parse()
                .map_err(|_| annalib::Error::Kvs("TTL must be a non-negative integer".into()))?;
            client.put_with_ttl(key, value, ttl).await?;
        }
        // Legacy aliases — map old commands to the unified GET/PUT.
        "GET_SET" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "GET_ORDERED_SET" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "GET_CAUSAL" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "GET_SINGLE_CAUSAL" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "GET_PRIORITY" if split.len() == 2 => {
            let value = client.get_value(split[1]).await?;
            println!("{}", value);
        }
        "PUT_SET" if split.len() >= 3 => {
            let mut args = vec!["PUT", "set"];
            args.extend_from_slice(&split[1..]);
            let (key, value) = parse_put_args(&args)?;
            client.put_value(&key, &value).await?;
        }
        "PUT_ORDERED_SET" if split.len() >= 3 => {
            let mut args = vec!["PUT", "ordered_set"];
            args.extend_from_slice(&split[1..]);
            let (key, value) = parse_put_args(&args)?;
            client.put_value(&key, &value).await?;
        }
        "PUT_CAUSAL" if split.len() == 3 => {
            let args = vec!["PUT", "causal", split[1], split[2]];
            let (key, value) = parse_put_args(&args)?;
            client.put_value(&key, &value).await?;
        }
        "PUT_SINGLE_CAUSAL" if split.len() == 3 => {
            let args = vec!["PUT", "single_causal", split[1], split[2]];
            let (key, value) = parse_put_args(&args)?;
            client.put_value(&key, &value).await?;
        }
        "PUT_PRIORITY" if split.len() == 4 => {
            let args = vec!["PUT", "priority", split[1], split[2], split[3]];
            let (key, value) = parse_put_args(&args)?;
            client.put_value(&key, &value).await?;
        }
        "BENCH" => {
            use annalib::bench::run_bench;
            let config = parse_bench_args(&split)?;
            run_bench(client, &config).await?;
        }
        "START" => {
            let components = parse_component_from_split(&split)?;
            println!(
                "{} anna processes were started",
                start(config_file_path, &components)?
            );
        }
        "STOP" => {
            let components = parse_component_from_split(&split)?;
            println!("{} anna processes were terminated", stop(&components)?);
        }
        "STATUS" => {
            let components = parse_component_from_split(&split)?;
            println!("{}", format_status(status(&components)?));
        }
        "HELP" => println!("{}", cli_usage()),
        "EXIT" => return Ok(true),
        _ => {
            return Err(CliError::Other(format!(
                "Invalid anna command line: '{}'\n{}",
                line,
                cli_usage()
            )))
        }
    }

    Ok(false)
}

fn cli_usage() -> String {
    "Redis-style commands:\
    \n\tget {key} \t\t\t- get the value of any key (auto-detects type)\
    \n\tput {key} {value} \t\t- store a value (LWW, default)\
    \n\tdel {key} \t\t\t- delete a key from the KVS\
    \n\tmget {key1} {key2} ... \t\t- get multiple keys at once\
    \n\tmset {k1} {v1} {k2} {v2} ... \t- batch PUT multiple keys\
    \n\tsadd {key} {m1} [m2 ...] \t- add member(s) to an OR-Set\
    \n\tsrem {key} {m1} [m2 ...] \t- remove member(s) from an OR-Set\
    \n\tsmembers {key} \t\t\t- get all members of an OR-Set\
    \n\tincr {key} [amount] \t\t- increment a counter (default +1)\
    \n\tdecr {key} [amount] \t\t- decrement a counter (default -1)\
    \n\tget_counter {key} \t\t- get counter value\
    \n\texpire {key} {value} {seconds} \t- store with TTL (auto-expires)\
    \n\tscan [prefix] \t\t\t- list keys matching prefix (all keys if omitted)\
    \n\tsubscribe {key1} [key2 ...] \t- watch keys for changes (Ctrl+C to stop)\
    \n\nAnna-specific PUT variants:\
    \n\tput set {key} {vals...} \t\t- store a set (union merge)\
    \n\tput ordered_set {key} {vals...} \t- store an ordered set\
    \n\tput lww_set {key} {vals...} \t- store a set (LWW, replaces on write)\
    \n\tput lww_ordered_set {key} {vals...} - store an ordered set (LWW)\
    \n\tput priority_set {key} {pri} {vals...} - store a set (lowest priority wins)\
    \n\tput priority_ordered_set {key} {pri} {vals...} - store ordered set (priority)\
    \n\tput causal_set {key} {vals...} \t- store a set (causal consistency)\
    \n\tput causal_ordered_set {key} {vals...} - store ordered set (causal)\
    \n\tput multi_causal_set {key} {vals...} - store a set (multi-key causal)\
    \n\tput multi_causal_ordered_set {key} {vals...} - ordered set (multi-key causal)\
    \n\tput union {key} {value} \t- append a value (accumulates via union)\
    \n\tput priority {key} {pri} {val} \t- store with priority (lowest wins)\
    \n\tput causal {key} {value} \t- store with multi-key causal consistency\
    \n\tput single_causal {key} {value} \t- store with single-key causal consistency\
    \n\nOther:\
    \n\tbench [keys] [value_size] [duration] [workload] - run a benchmark\
    \n\tstart [component] \t\t- start anna processes (component: kvs, monitor, route; omit for all)\
    \n\tstop [component] \t\t- stop running anna processes\
    \n\tstatus [component] \t\t- print the status of anna processes\
    \n\thelp \t\t\t\t- print this usage message\
    \n\texit \t\t\t\t- exit the CLI (does not stop any anna processes)"
        .to_string()
}

async fn cli_loop_interactive(
    mut client: KVSClient,
    client_config: ClientConfig,
    config_file_path: PathBuf,
) -> Result<&'static str> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

    std::thread::spawn(move || {
        let mut rl = Editor::new().expect("Failed to create editor");
        rl.set_helper(Some(AnnaCompleter));
        if rl.load_history(ANNA_HISTORY_FILENAME).is_err() {
            println!(
                "No previous history. Saving new history in {}",
                ANNA_HISTORY_FILENAME
            );
        }

        while let Ok(line) = rl.readline("anna> ") {
            let _ = rl.add_history_entry(&line);
            if tx.blocking_send(line).is_err() {
                break;
            }
        }

        let _ = rl.save_history(ANNA_HISTORY_FILENAME);
    });

    while let Some(line) = rx.recv().await {
        match execute_command(&mut client, &line, &client_config, &config_file_path).await {
            Ok(true) => break,
            Err(e) => error!("{}", e),
            _ => {}
        }
    }

    Ok("")
}

async fn cli_loop_file(
    mut client: KVSClient,
    filename: &str,
    client_config: ClientConfig,
    config_file_path: PathBuf,
) -> Result<&'static str> {
    let file = File::open(filename).map_err(|e| {
        CliError::Other(format!("Could not open command file '{}': {}", filename, e))
    })?;
    let reader = BufReader::new(file);

    for line in reader.lines().map_while(|l| l.ok()) {
        match execute_command(&mut client, &line, &client_config, &config_file_path).await {
            Ok(true) => break,
            Err(e) => error!("Error while executing command line: '{}'\n{}", line, e),
            _ => {}
        }
    }

    Ok("")
}

fn get_app() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("verbosity")
                .short('v')
                .long("verbosity")
                .num_args(1)
                .value_name("VERBOSITY_LEVEL")
                .help("Set verbosity level for output (trace, debug, info, warn, error (default))"),
        )
        .arg(
            Arg::new("routing")
                .short('r')
                .long("routing")
                .num_args(1..)
                .value_name("ROUTING_ADDRESS")
                .help("Routing tier address(es), e.g. tcp://10.0.0.1:6450"),
        )
        .arg(
            Arg::new("client-ip")
                .long("client-ip")
                .num_args(1)
                .value_name("IP")
                .help("IP address this client binds on (default: 127.0.0.1)"),
        )
        .arg(
            Arg::new("server-config")
                .long("server-config")
                .num_args(1)
                .value_name("CONFIG_FILE")
                .help("Server config file for start/stop commands"),
        )
        .subcommand(
            Command::new("cli")
                .about("Enter the CLI to interact with anna")
                .arg(
                    Arg::new("command_file")
                        .index(1)
                        .help("An optional file of commands to run"),
                ),
        )
        .subcommand(
            Command::new("start")
                .about("Start the KVS server processes")
                .arg(
                    Arg::new("component")
                        .index(1)
                        .value_parser(COMPONENT_NAMES.to_vec())
                        .help("Component to start (kvs, monitor, route). Omit to start all"),
                ),
        )
        .subcommand(
            Command::new("stop")
                .about("Stop the KVS server processes")
                .arg(
                    Arg::new("component")
                        .index(1)
                        .value_parser(COMPONENT_NAMES.to_vec())
                        .help("Component to stop (kvs, monitor, route). Omit to stop all"),
                ),
        )
        .subcommand(
            Command::new("status")
                .about("Report status of KVS server processes")
                .arg(
                    Arg::new("component")
                        .index(1)
                        .value_parser(COMPONENT_NAMES.to_vec())
                        .help("Component to check (kvs, monitor, route). Omit to check all"),
                ),
        )
        .subcommand(
            Command::new("bench")
                .about("Run a benchmark against the KVS")
                .arg(
                    Arg::new("keys")
                        .long("keys")
                        .num_args(1)
                        .default_value("1000")
                        .value_parser(clap::value_parser!(u64).range(1..))
                        .help("Number of keys to use in the benchmark"),
                )
                .arg(
                    Arg::new("value-size")
                        .long("value-size")
                        .num_args(1)
                        .default_value("256")
                        .value_parser(clap::value_parser!(usize))
                        .help("Size of values in bytes"),
                )
                .arg(
                    Arg::new("duration")
                        .long("duration")
                        .num_args(1)
                        .default_value("10")
                        .value_parser(clap::value_parser!(u64).range(1..))
                        .help("Duration of each workload in seconds"),
                )
                .arg(
                    Arg::new("report")
                        .long("report")
                        .num_args(1)
                        .default_value("2")
                        .value_parser(clap::value_parser!(u64).range(1..))
                        .help("Report interval in seconds"),
                )
                .arg(
                    Arg::new("workload")
                        .long("workload")
                        .num_args(1)
                        .default_value("ALL")
                        .value_parser(["GET", "PUT", "MIXED", "ALL"])
                        .help("Workload type: GET, PUT, MIXED, or ALL"),
                ),
        )
}

fn bench_config_from_clap(args: &ArgMatches) -> annalib::bench::BenchConfig {
    let num_keys = *args.get_one::<u64>("keys").expect("default set");
    let value_size = *args.get_one::<usize>("value-size").expect("default set");
    let duration_secs = *args.get_one::<u64>("duration").expect("default set");
    let report_secs = *args.get_one::<u64>("report").expect("default set");
    let workload = args
        .get_one::<String>("workload")
        .expect("default set")
        .clone();

    let workloads = match workload.as_str() {
        "ALL" => vec!["GET".into(), "PUT".into(), "MIXED".into()],
        other => vec![other.to_string()],
    };

    annalib::bench::BenchConfig {
        num_keys,
        value_size,
        duration: Duration::from_secs(duration_secs),
        report_period: Duration::from_secs(report_secs),
        workloads,
    }
}

fn parse_bench_args(split: &[&str]) -> Result<annalib::bench::BenchConfig> {
    let num_keys: u64 = match split.get(1) {
        Some(s) => s.parse().map_err(|_| {
            CliError::Other(format!(
                "Invalid keys value '{}'. Usage: BENCH [keys] [value_size] [duration] [workload]",
                s
            ))
        })?,
        None => 1000,
    };
    let value_size: usize = match split.get(2) {
        Some(s) => s.parse().map_err(|_| {
            CliError::Other(format!(
                "Invalid value_size '{}'. Usage: BENCH [keys] [value_size] [duration] [workload]",
                s
            ))
        })?,
        None => 256,
    };
    let dur: u64 = match split.get(3) {
        Some(s) => s.parse().map_err(|_| {
            CliError::Other(format!(
                "Invalid duration '{}'. Usage: BENCH [keys] [value_size] [duration] [workload]",
                s
            ))
        })?,
        None => 10,
    };
    let wl_arg = split
        .get(4)
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "ALL".into());
    let workloads = match wl_arg.as_str() {
        "ALL" => vec!["GET".into(), "PUT".into(), "MIXED".into()],
        "GET" | "PUT" | "MIXED" => vec![wl_arg.clone()],
        _ => {
            return Err(CliError::Other(format!(
                "Invalid workload '{}'. Must be GET, PUT, MIXED, or ALL",
                wl_arg
            )))
        }
    };
    Ok(annalib::bench::BenchConfig {
        num_keys,
        value_size,
        duration: Duration::from_secs(dur),
        report_period: Duration::from_secs(2),
        workloads,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format_status_no_processes() {
        let status = vec![
            ("anna-monitor".into(), vec![]),
            ("anna-route".into(), vec![]),
        ];
        let output = format_status(status);
        assert!(output.contains("anna-monitor"));
        assert!(output.contains("is not running"));
    }

    #[test]
    fn format_status_with_pids() {
        let status = vec![("anna-kvs".into(), vec![1234, 5678])];
        let output = format_status(status);
        assert!(output.contains("anna-kvs"));
        assert!(output.contains("1234"));
        assert!(output.contains("5678"));
    }

    #[test]
    fn cli_usage_contains_commands() {
        let usage = cli_usage();
        assert!(usage.contains("get"));
        assert!(usage.contains("put"));
        assert!(usage.contains("start"));
        assert!(usage.contains("stop"));
        assert!(usage.contains("status"));
        assert!(usage.contains("exit"));
    }

    #[test]
    fn parse_component_from_split_no_args() {
        let split = vec!["START"];
        let result = parse_component_from_split(&split).expect("should succeed");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_component_from_split_valid() {
        let split = vec!["START", "kvs"];
        let result = parse_component_from_split(&split).expect("should succeed");
        assert_eq!(result, vec![Component::Kvs]);
    }

    #[test]
    fn parse_component_from_split_invalid() {
        let split = vec!["START", "bogus"];
        assert!(parse_component_from_split(&split).is_err());
    }

    #[test]
    fn parse_component_from_split_surplus_args() {
        let split = vec!["STOP", "kvs", "extra"];
        let err = parse_component_from_split(&split).expect_err("should fail with surplus args");
        assert!(
            err.to_string().contains("at most one"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn bench_clap_rejects_zero_keys() {
        let app = get_app();
        let result = app.try_get_matches_from(vec![
            "anna",
            "--routing",
            "tcp://127.0.0.1:6450",
            "--client-ip",
            "127.0.0.1",
            "bench",
            "--keys",
            "0",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn bench_clap_rejects_zero_duration() {
        let app = get_app();
        let result = app.try_get_matches_from(vec![
            "anna",
            "--routing",
            "tcp://127.0.0.1:6450",
            "--client-ip",
            "127.0.0.1",
            "bench",
            "--duration",
            "0",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn bench_clap_rejects_invalid_workload() {
        let app = get_app();
        let result = app.try_get_matches_from(vec![
            "anna",
            "--routing",
            "tcp://127.0.0.1:6450",
            "--client-ip",
            "127.0.0.1",
            "bench",
            "--workload",
            "INVALID",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn bench_clap_accepts_valid_args() {
        let app = get_app();
        let matches = app
            .try_get_matches_from(vec![
                "anna",
                "--routing",
                "tcp://127.0.0.1:6450",
                "--client-ip",
                "127.0.0.1",
                "bench",
                "--keys",
                "500",
                "--value-size",
                "128",
                "--duration",
                "5",
                "--report",
                "1",
                "--workload",
                "PUT",
            ])
            .expect("should parse valid bench args");
        let (name, sub) = matches.subcommand().expect("should have subcommand");
        assert_eq!(name, "bench");
        assert_eq!(*sub.get_one::<u64>("keys").expect("keys"), 500);
        assert_eq!(
            *sub.get_one::<usize>("value-size").expect("value-size"),
            128
        );
        assert_eq!(*sub.get_one::<u64>("duration").expect("duration"), 5);
        assert_eq!(*sub.get_one::<u64>("report").expect("report"), 1);
        assert_eq!(sub.get_one::<String>("workload").expect("workload"), "PUT");
    }

    #[test]
    fn bench_clap_defaults() {
        let app = get_app();
        let matches = app
            .try_get_matches_from(vec![
                "anna",
                "--routing",
                "tcp://127.0.0.1:6450",
                "--client-ip",
                "127.0.0.1",
                "bench",
            ])
            .expect("should parse bench with defaults");
        let (_, sub) = matches.subcommand().expect("should have subcommand");
        let config = bench_config_from_clap(sub);
        assert_eq!(config.num_keys, 1000);
        assert_eq!(config.value_size, 256);
        assert_eq!(config.duration, Duration::from_secs(10));
        assert_eq!(config.report_period, Duration::from_secs(2));
        assert_eq!(config.workloads, vec!["GET", "PUT", "MIXED"]);
    }

    #[test]
    fn bench_config_from_clap_custom() {
        let app = get_app();
        let matches = app
            .try_get_matches_from(vec![
                "anna",
                "--routing",
                "tcp://127.0.0.1:6450",
                "--client-ip",
                "127.0.0.1",
                "bench",
                "--keys",
                "500",
                "--value-size",
                "128",
                "--duration",
                "5",
                "--report",
                "1",
                "--workload",
                "PUT",
            ])
            .expect("should parse");
        let (_, sub) = matches.subcommand().expect("should have subcommand");
        let config = bench_config_from_clap(sub);
        assert_eq!(config.num_keys, 500);
        assert_eq!(config.value_size, 128);
        assert_eq!(config.duration, Duration::from_secs(5));
        assert_eq!(config.report_period, Duration::from_secs(1));
        assert_eq!(config.workloads, vec!["PUT"]);
    }

    #[test]
    fn parse_bench_args_defaults() {
        let split = vec!["BENCH"];
        let config = parse_bench_args(&split).expect("should parse");
        assert_eq!(config.num_keys, 1000);
        assert_eq!(config.value_size, 256);
        assert_eq!(config.duration, Duration::from_secs(10));
        assert_eq!(config.workloads, vec!["GET", "PUT", "MIXED"]);
    }

    #[test]
    fn parse_bench_args_custom() {
        let split = vec!["BENCH", "500", "128", "5", "PUT"];
        let config = parse_bench_args(&split).expect("should parse");
        assert_eq!(config.num_keys, 500);
        assert_eq!(config.value_size, 128);
        assert_eq!(config.duration, Duration::from_secs(5));
        assert_eq!(config.workloads, vec!["PUT"]);
    }

    #[test]
    fn parse_bench_args_invalid_keys() {
        let split = vec!["BENCH", "nope"];
        assert!(parse_bench_args(&split).is_err());
    }

    #[test]
    fn parse_bench_args_invalid_duration() {
        let split = vec!["BENCH", "100", "256", "abc"];
        assert!(parse_bench_args(&split).is_err());
    }

    #[test]
    fn parse_bench_args_invalid_workload() {
        let split = vec!["BENCH", "100", "256", "10", "BOGUS"];
        assert!(parse_bench_args(&split).is_err());
    }

    #[test]
    fn parse_bench_args_all_workloads() {
        let split = vec!["BENCH", "100", "256", "10", "all"];
        let config = parse_bench_args(&split).expect("should parse");
        assert_eq!(config.workloads, vec!["GET", "PUT", "MIXED"]);
    }
}
