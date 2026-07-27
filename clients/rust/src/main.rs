#![warn(clippy::unwrap_used)]
//! `anna` is a command line tool for working with the `anna` key-value store
//!
//! Execute `anna` or `anna --help` or `anna -h` at the comment line for a
//! description of the command line options.

use std::process::exit;
use std::time::{Duration, Instant};

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
                None => cli_loop_interactive(client, server_config_path).await?,
                Some(filename) => cli_loop_file(client, filename, server_config_path).await?,
            }
            .into())
        }
        ("bench", sub_matches) => {
            let config = get_client_config(&matches);
            let mut client = KVSClient::new(&config, None).await;
            run_bench(&mut client, sub_matches).await?;
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

/// Returns `true` if the user requested exit.
async fn execute_command(
    client: &mut KVSClient,
    line: &str,
    config_file_path: &Path,
) -> Result<bool> {
    let split = line.trim().split(' ').collect::<Vec<&str>>();

    match split[0].to_ascii_uppercase().as_str() {
        "GET" if split.len() == 2 => println!("{}", client.get(split[1]).await?),
        "DELETE" if split.len() == 2 => client.delete(split[1]).await?,
        "PUT" if split.len() == 3 => client.put(split[1], split[2]).await?,
        #[cfg(feature = "causal")]
        "GET_CAUSAL" if split.len() == 2 => {
            let (vc, deps, value) = client.get_causal(split[1]).await?;
            let mut sorted_vc: Vec<_> = vc.iter().collect();
            sorted_vc.sort_by_key(|(k, _)| k.to_string());
            for (k, v) in &sorted_vc {
                println!("{{{} : {}}}", k, v);
            }
            let mut sorted_deps = deps.clone();
            sorted_deps.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (dep_key, dep_vc) in &sorted_deps {
                let mut sorted_dep_vc: Vec<_> = dep_vc.iter().collect();
                sorted_dep_vc.sort_by_key(|(k, _)| k.to_string());
                let vc_str: Vec<String> = sorted_dep_vc
                    .iter()
                    .map(|(k, v)| format!("{{{} : {}}}", k, v))
                    .collect();
                println!("{} : {}", dep_key, vc_str.join(" "));
            }
            println!("{}", value);
        }
        #[cfg(feature = "causal")]
        "PUT_CAUSAL" if split.len() == 3 => client.put_causal(split[1], split[2]).await?,
        #[cfg(feature = "set")]
        "GET_SET" if split.len() == 2 => {
            let values = client.get_set(split[1]).await?;
            println!("{{ {} }}", values.join(" "));
        }
        #[cfg(feature = "set")]
        "PUT_SET" if split.len() >= 3 => client.put_set(split[1], &split[2..]).await?,
        #[cfg(feature = "set")]
        "GET_ORDERED_SET" if split.len() == 2 => {
            let values = client.get_ordered_set(split[1]).await?;
            println!("[ {} ]", values.join(" "));
        }
        #[cfg(feature = "set")]
        "PUT_ORDERED_SET" if split.len() >= 3 => {
            client.put_ordered_set(split[1], &split[2..]).await?
        }
        #[cfg(feature = "causal")]
        "GET_SINGLE_CAUSAL" if split.len() == 2 => {
            let (vc, values) = client.get_single_causal(split[1]).await?;
            let mut sorted_vc: Vec<_> = vc.iter().collect();
            sorted_vc.sort_by_key(|(k, _)| k.to_string());
            for (k, v) in &sorted_vc {
                println!("{{{} : {}}}", k, v);
            }
            for v in &values {
                println!("{}", v);
            }
        }
        #[cfg(feature = "causal")]
        "PUT_SINGLE_CAUSAL" if split.len() == 3 => {
            client.put_single_causal(split[1], split[2]).await?
        }
        "GET_PRIORITY" if split.len() == 2 => {
            let (priority, value) = client.get_priority(split[1]).await?;
            println!("priority: {}", priority);
            println!("{}", value);
        }
        "PUT_PRIORITY" if split.len() == 4 => {
            let priority = split[2]
                .parse::<f64>()
                .map_err(|e| CliError::Other(format!("Invalid priority '{}': {}", split[2], e)))?;
            client.put_priority(split[1], priority, split[3]).await?
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
    let mut usage = "Valid commands are:\
    \n\tget {{key}} \t\t\t- get the value of entry with key = {{key}} from the KVS\
    \n\tput {{key}} {{value}} \t\t- set entry with key = {{key}} in the KVS to have value = {{value}}"
        .into();

    #[cfg(feature = "causal")]
    {
        usage = format!(
            "{}\n\tget_causal {{key}} \t\t- causal 'get' of value with key = {{key}} in the KVS\
            \n\tput_causal {{key}} {{value}} \t- causal set of value with key = {{key}} in the KVS",
            usage
        );
    }

    #[cfg(feature = "causal")]
    {
        usage = format!(
            "{}\n\tget_single_causal {{key}} \t- single-key causal 'get' of value with key = {{key}} in the KVS\
            \n\tput_single_causal {{key}} {{value}} \t- single-key causal set of value with key = {{key}} in the KVS",
            usage
        );
    }

    #[cfg(feature = "set")]
    {
        usage = format!(
            "{}\n\tget_set {{key}} \t\t\t- get the value of the set with key = {{key}} in the KVS\
        \n\tput_set {{key}} {{set}} \t\t- set the value of the set with key = {{key}} in the KVS\
        \n\tget_ordered_set {{key}} \t\t- get the ordered set with key = {{key}} in the KVS\
        \n\tput_ordered_set {{key}} {{set}} \t- set the ordered set with key = {{key}} in the KVS",
            usage
        );
    }

    usage = format!(
        "{}\n\tget_priority {{key}} \t\t- get the priority value with key = {{key}} in the KVS\
        \n\tput_priority {{key}} {{priority}} {{value}} - set value with priority for key = {{key}} in the KVS",
        usage
    );

    usage = format!(
        "{}\n\tdelete {{key}} \t\t\t- delete a key from the KVS\
        \n\tstart [component] \t\t- start anna processes (component: kvs, monitor, route; omit for all)\
        \n\tstop [component] \t\t- stop running anna processes (component: kvs, monitor, route; omit for all)\
        \n\tstatus [component] \t\t- print the status of anna processes (component: kvs, monitor, route; omit for all)\
        \n\thelp \t\t\t\t- print this usage message\
        \n\texit \t\t\t\t- exit the CLI (does not stop any anna processes)",
        usage
    );

    usage
}

async fn cli_loop_interactive(
    mut client: KVSClient,
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
        match execute_command(&mut client, &line, &config_file_path).await {
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
    config_file_path: PathBuf,
) -> Result<&'static str> {
    let file = File::open(filename).map_err(|e| {
        CliError::Other(format!("Could not open command file '{}': {}", filename, e))
    })?;
    let reader = BufReader::new(file);

    for line in reader.lines().map_while(|l| l.ok()) {
        match execute_command(&mut client, &line, &config_file_path).await {
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

/// Format a key index as a zero-padded 8-character string.
fn bench_key(index: u64) -> String {
    format!("{:08}", index)
}

/// Simple pseudo-random number generator state.
/// Uses a linear congruential generator to avoid needing the `rand` crate.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns a pseudo-random u64.
    fn next_u64(&mut self) -> u64 {
        // LCG parameters from Numerical Recipes
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }

    /// Returns a value in [0, bound).
    fn next_bounded(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// A single workload result for the summary table.
struct WorkloadResult {
    name: &'static str,
    total_ops: u64,
    elapsed: Duration,
}

impl WorkloadResult {
    fn ops_per_sec(&self) -> f64 {
        self.total_ops as f64 / self.elapsed.as_secs_f64()
    }

    fn us_per_op(&self) -> f64 {
        if self.total_ops == 0 {
            return 0.0;
        }
        self.elapsed.as_micros() as f64 / self.total_ops as f64
    }
}

async fn run_bench(client: &mut KVSClient, args: &ArgMatches) -> Result<()> {
    let num_keys = *args.get_one::<u64>("keys").expect("default set");
    let value_size = *args.get_one::<usize>("value-size").expect("default set");
    let duration_secs = *args.get_one::<u64>("duration").expect("default set");
    let report_secs = *args.get_one::<u64>("report").expect("default set");
    let workload = args
        .get_one::<String>("workload")
        .expect("default set")
        .clone();

    let value: String = "a".repeat(value_size);
    let duration = Duration::from_secs(duration_secs);
    let report_period = Duration::from_secs(report_secs);

    println!(
        "Benchmark (Rust): keys={}, value_size={}, duration={}s, report={}s, workload={}",
        num_keys, value_size, duration_secs, report_secs, workload
    );

    // Warmup: PUT all keys
    println!("Warming up {} keys...", num_keys);
    let warmup_start = Instant::now();
    for i in 0..num_keys {
        let key = bench_key(i);
        client.put(&key, &value).await?;
    }
    let warmup_elapsed = warmup_start.elapsed();
    println!(
        "Warmup complete: {} keys in {:.2}s",
        num_keys,
        warmup_elapsed.as_secs_f64()
    );

    let workloads: Vec<&str> = match workload.as_str() {
        "ALL" => vec!["GET", "PUT", "MIXED"],
        other => vec![other],
    };

    let mut results: Vec<WorkloadResult> = Vec::new();

    for wl in &workloads {
        let result = run_workload(client, wl, num_keys, &value, duration, report_period).await?;
        results.push(result);
    }

    // Print summary table
    println!();
    println!("=== Benchmark Summary (Rust) ===");
    println!(
        "{:<10} {:>12} {:>12} {:>12} {:>10}",
        "Workload", "ops/sec", "us/op", "total_ops", "elapsed"
    );
    println!("{}", "-".repeat(60));
    for r in &results {
        println!(
            "{:<10} {:>12.1} {:>12.1} {:>12} {:>9.2}s",
            r.name,
            r.ops_per_sec(),
            r.us_per_op(),
            r.total_ops,
            r.elapsed.as_secs_f64()
        );
    }

    Ok(())
}

async fn run_workload(
    client: &mut KVSClient,
    workload: &str,
    num_keys: u64,
    value: &str,
    duration: Duration,
    report_period: Duration,
) -> Result<WorkloadResult> {
    println!();
    println!("--- {} workload ---", workload);

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let mut rng = SimpleRng::new(seed);

    let start = Instant::now();
    let mut total_ops: u64 = 0;
    let mut epoch_ops: u64 = 0;
    let mut last_report = start;

    while start.elapsed() < duration {
        let key_index = rng.next_bounded(num_keys);
        let key = bench_key(key_index);

        match workload {
            "GET" => {
                client.get(&key).await?;
                total_ops += 1;
                epoch_ops += 1;
            }
            "PUT" => {
                client.put(&key, value).await?;
                total_ops += 1;
                epoch_ops += 1;
            }
            "MIXED" => {
                client.put(&key, value).await?;
                client.get(&key).await?;
                total_ops += 2;
                epoch_ops += 2;
            }
            _ => unreachable!("invalid workload validated by clap"),
        }

        let now = Instant::now();
        if now.duration_since(last_report) >= report_period {
            let epoch_elapsed = now.duration_since(last_report).as_secs_f64();
            let epoch_throughput = epoch_ops as f64 / epoch_elapsed;
            println!(
                "  [{:>6.1}s] {:>10.1} ops/sec  ({} ops in {:.2}s)",
                now.duration_since(start).as_secs_f64(),
                epoch_throughput,
                epoch_ops,
                epoch_elapsed,
            );
            epoch_ops = 0;
            last_report = now;
        }
    }

    // Print final partial epoch if any ops remain unreported
    if epoch_ops > 0 {
        let now = Instant::now();
        let epoch_elapsed = now.duration_since(last_report).as_secs_f64();
        if epoch_elapsed > 0.0 {
            let epoch_throughput = epoch_ops as f64 / epoch_elapsed;
            println!(
                "  [{:>6.1}s] {:>10.1} ops/sec  ({} ops in {:.2}s)",
                now.duration_since(start).as_secs_f64(),
                epoch_throughput,
                epoch_ops,
                epoch_elapsed,
            );
        }
    }

    let elapsed = start.elapsed();
    let name = match workload {
        "GET" => "GET",
        "PUT" => "PUT",
        "MIXED" => "MIXED",
        _ => unreachable!(),
    };

    println!(
        "{} complete: {} ops in {:.2}s ({:.1} ops/sec)",
        name,
        total_ops,
        elapsed.as_secs_f64(),
        total_ops as f64 / elapsed.as_secs_f64()
    );

    Ok(WorkloadResult {
        name,
        total_ops,
        elapsed,
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
}
