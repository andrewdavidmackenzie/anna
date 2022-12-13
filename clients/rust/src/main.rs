#![warn(clippy::unwrap_used)]
//! `anna` is a command line tool for working with the `anna` key-value store
//!
//! Execute `anna` or `anna --help` or `anna -h` at the comment line for a
//! description of the command line options.

#[macro_use]
extern crate error_chain;

use std::env;
use std::process::exit;

use annalib::{config::Config, info, kvs_client::KVSClient, start, status, stop};
use clap::{Arg, ArgMatches, Command};
use log::{debug, error, info, warn};
use rustyline::Editor;
use simplog::SimpleLogger;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const ANNA_HISTORY_FILENAME: &str = ".anna_history";
const DEFAULT_CONFIG_FILENAME: &str = "default-config.yml";

/// `anna` CLI Error codes
/// 0 - Success
/// 1 - Config file error
/// 2 - Command line arguments error (from clap)
const SUCCESS: i32 = 0;

// We'll put our errors in an `errors` module, and other modules in this crate will
// `use crate::errors::*;` to get access to everything `error_chain!` creates.
#[doc(hidden)]
pub mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Clap(clap::Error);
        Anna(annalib::Error);
        RustyLine(rustyline::error::ReadlineError);
    }

    errors {
        ConfigFileError(p: String, m: String) {
            description("Config file error")
            display("Problem loading config from file: '{}'\n{}", p, m)
        }
    }
}

use crate::ErrorKind::ConfigFileError;
pub use errors::*;

fn main() {
    match run() {
        Err(ref e) => {
            eprintln!("error: {}", e);

            for e in e.iter().skip(1) {
                eprintln!("caused by: {}", e);
            }

            // The backtrace is generated if env var `RUST_BACKTRACE` is set to `1` or `full`
            if let Some(backtrace) = e.backtrace() {
                eprintln!("backtrace: {:?}", backtrace);
            }

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

fn get_config_path(args: &ArgMatches) -> Result<PathBuf> {
    match args.get_one::<String>("config").map(|s| s.as_str()) {
        Some(config_file) => PathBuf::from(config_file).canonicalize().chain_err(|| {
            ConfigFileError(
                config_file.into(),
                "Could not canonicalize config file path".into(),
            )
        }),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(DEFAULT_CONFIG_FILENAME)
            .canonicalize()
            .chain_err(|| {
                ConfigFileError(
                    DEFAULT_CONFIG_FILENAME.into(),
                    "Could not canonicalize default config file path".into(),
                )
            }),
    }
}

fn get_client(matches: &ArgMatches) -> Result<KVSClient> {
    let config_file_path = get_config_path(matches)?;
    let config = Config::read(&config_file_path)?;
    info!("Using config file: {}", config_file_path.display());
    Ok(KVSClient::new(&config, None))
}

/*
    run the cli using clap to interpret commands and options
*/
fn run() -> Result<String> {
    let app = get_app();
    let app_clone = app.clone();
    let matches = app.get_matches();

    // Initialize the logger with the level of verbosity requested via option (or the default)
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
        .ok_or("Could not find valid subcommand")?
    {
        ("start", _) => Ok(format!(
            "{} anna processes were started",
            start(&get_config_path(&matches)?)?
        )),
        ("status", _) => Ok(print_status(status()?)),
        ("stop", _) => Ok(format!("{} anna processes were terminated", stop()?)),
        ("cli", arg_matches) => Ok(cli(
            get_client(arg_matches)?,
            arg_matches,
            get_config_path(&matches)?,
        )?
        .into()),
        (_, _) => Ok("No command executed".into()),
    }
}

fn print_status(status: Vec<(String, Vec<i32>)>) -> String {
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

fn execute_command(client: &KVSClient, line: &str, config_file_path: &Path) -> Result<()> {
    let split = line.trim().split(' ').collect::<Vec<&str>>();

    match split[0].to_ascii_uppercase().as_str() {
        "GET" if split.len() == 2 => println!("{}", client.get(split[1])?),
        "PUT" if split.len() == 3 => client.put(split[1], split[2])?,
        #[cfg(feature = "causal")]
        "GET_CAUSAL" if split.len() == 2 => println!("{}", client.get_causal(split[1])?),
        #[cfg(feature = "causal")]
        "PUT_CAUSAL" if split.len() == 3 => client.put_causal(split[1], split[1])?,
        #[cfg(feature = "set")]
        "GET_SET" if split.len() == 2 => println!("{}", client.get_set(split[1])?),
        #[cfg(feature = "set")]
        "PUT_SET" if split.len() >= 3 => client.put_set(split[1], &split[2..])?,
        "START" => println!("{} anna processes were started", start(config_file_path)?),
        "STOP" => println!("{} anna processes were terminated", stop()?),
        "STATUS" => println!("{}", print_status(status()?)),
        "HELP" => println!("{}", cli_usage()),
        "EXIT" => exit(0),
        _ => bail!("Invalid anna command line: '{}'\n{}", line, cli_usage()),
    }

    Ok(())
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

    #[cfg(feature = "set")]
    {
        usage = format!(
            "{}\n\tget_set {{key}} \t\t\t- get the value of the set with key = {{key}} in the KVS\
        \n\tput_set {{key}} {{set}} \t\t- set the value of the set with key = {{key}} in the KVS",
            usage
        );
    }

    usage = format!(
        "{}\n\tstart \t\t\t\t- start anna processes\
        \n\tstop \t\t\t\t- stop running anna processes\
        \n\tstatus \t\t\t\t- print the status of anna processes\
        \n\thelp \t\t\t\t- print this usage message\
        \n\texit \t\t\t\t- exit the CLI (does not stop any anna processes)",
        usage
    );

    usage
}

/*
    Enter a loop of command/response for the CLI and interact with the server processes for each
*/
fn cli_loop_interactive(client: KVSClient, config_file_path: PathBuf) -> Result<&'static str> {
    let mut rl = Editor::<()>::new(); // `()` can be used when no completer is required
    if rl.load_history(ANNA_HISTORY_FILENAME).is_err() {
        println!(
            "No previous history. Saving new history in {}",
            ANNA_HISTORY_FILENAME
        );
    }

    while let Ok(line) = rl.readline("anna> ") {
        rl.add_history_entry(&line);
        if let Err(e) = execute_command(&client, &line, &config_file_path) {
            error!("{}", e);
        }
    }

    rl.save_history(ANNA_HISTORY_FILENAME)?;

    Ok("History saved. Exiting")
}

/*
    Enter a loop of command/response for the CLI and interact with the server processes for each
*/
fn cli_loop_file(
    client: KVSClient,
    filename: &str,
    config_file_path: PathBuf,
) -> Result<&'static str> {
    let file = File::open(filename)
        .chain_err(|| format!("Could not open the command_file: {}", filename))?;
    let reader = BufReader::new(file);

    for line in reader.lines().flatten() {
        if let Err(e) = execute_command(&client, &line, &config_file_path) {
            error!("Error while executing command line: '{}'\n{}", line, e);
        }
    }

    Ok("")
}

/*
   Try to parse and then open a command_file of anna commands
*/
fn cli(client: KVSClient, args: &ArgMatches, config_file_path: PathBuf) -> Result<&'static str> {
    match args.get_one::<String>("command_file").map(|s| s.as_str()) {
        None => cli_loop_interactive(client, config_file_path),
        Some(filename) => cli_loop_file(client, filename, config_file_path),
    }
}

/*
    Create the clap app with the desired options and sub commands
*/
fn get_app() -> Command {
    let app = Command::new(env!("CARGO_PKG_NAME")).version(env!("CARGO_PKG_VERSION"));

    app.arg(
        Arg::new("verbosity")
            .short('v')
            .long("verbosity")
            .num_args(0..1)
            .number_of_values(1)
            .value_name("VERBOSITY_LEVEL")
            .help("Set verbosity level for output (trace, debug, info, warn, error (default))"),
    )
    .arg(
        Arg::new("config")
            .short('c')
            .long("config")
            .num_args(1)
            .number_of_values(1)
            .value_name("CONFIG_FILE")
            .help("Specify the config file to be used"),
    )
    .subcommand(
        Command::new("cli")
            .about("Start anna CLI (interactive or specify file to read commands from)")
            .arg(
                Arg::new("command_file")
                    .index(1)
                    .help("A file where anna commands are read from"),
            ),
    )
    .subcommand(
        Command::new("start").about("Start anna processes (monitor, route and kvs) in background"),
    )
    .subcommand(
        Command::new("stop").about("Stop any running anna processes (monitor, route and kvs)"),
    )
    .subcommand(
        Command::new("status").about("Show the status of anna processes (monitor, route and kvs)"),
    )
}
