#![warn(clippy::unwrap_used)]
//! `anna` is a command line tool for working with the `anna` key-value store
//!
//! Execute `anna` or `anna --help` or `anna -h` at the comment line for a
//! description of the command line options.

use std::env;
use std::process::exit;

use annalib::{completer::AnnaCompleter, config::Config, info, kvs_client::KVSClient, start, status, stop};
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
    #[error("Problem loading config from file: '{path}'\n{detail}")]
    ConfigFile { path: String, detail: String },
    #[error("{0}")]
    Other(String),
}

type Result<T> = std::result::Result<T, CliError>;

fn main() {
    match run() {
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

fn get_config_path(args: &ArgMatches) -> Result<PathBuf> {
    match args.get_one::<String>("config").map(|s| s.as_str()) {
        Some(config_file) => PathBuf::from(config_file)
            .canonicalize()
            .map_err(|e| CliError::ConfigFile {
                path: config_file.into(),
                detail: format!("Could not canonicalize: {}", e),
            }),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(DEFAULT_CONFIG_FILENAME)
            .canonicalize()
            .map_err(|e| CliError::ConfigFile {
                path: DEFAULT_CONFIG_FILENAME.into(),
                detail: format!("Could not canonicalize default config: {}", e),
            }),
    }
}

fn run() -> Result<String> {
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
        ("start", _) => Ok(format!(
            "{} anna processes were started",
            start(&get_config_path(&matches)?)?
        )),
        ("status", _) => Ok(print_status(status()?)),
        ("stop", _) => Ok(format!("{} anna processes were terminated", stop()?)),
        ("cli", arg_matches) => {
            let config_path = get_config_path(&matches)?;
            let config = Config::read(&config_path)?;
            let client = KVSClient::new(&config, None);
            Ok(cli(client, arg_matches, config_path)?.into())
        }
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

fn execute_command(client: &mut KVSClient, line: &str, config_file_path: &Path) -> Result<()> {
    let split = line.trim().split(' ').collect::<Vec<&str>>();

    match split[0].to_ascii_uppercase().as_str() {
        "GET" if split.len() == 2 => println!("{}", client.get(split[1])?),
        "PUT" if split.len() == 3 => client.put(split[1], split[2])?,
        #[cfg(feature = "causal")]
        "GET_CAUSAL" if split.len() == 2 => println!("{}", client.get_causal(split[1])?),
        #[cfg(feature = "causal")]
        "PUT_CAUSAL" if split.len() == 3 => client.put_causal(split[1], split[2])?,
        #[cfg(feature = "set")]
        "GET_SET" if split.len() == 2 => {
            let values = client.get_set(split[1])?;
            println!("{{ {} }}", values.join(" "));
        }
        #[cfg(feature = "set")]
        "PUT_SET" if split.len() >= 3 => client.put_set(split[1], &split[2..])?,
        "START" => println!("{} anna processes were started", start(config_file_path)?),
        "STOP" => println!("{} anna processes were terminated", stop()?),
        "STATUS" => println!("{}", print_status(status()?)),
        "HELP" => println!("{}", cli_usage()),
        "EXIT" => exit(0),
        _ => return Err(CliError::Other(format!(
            "Invalid anna command line: '{}'\n{}",
            line,
            cli_usage()
        ))),
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

fn cli_loop_interactive(mut client: KVSClient, config_file_path: PathBuf) -> Result<&'static str> {
    let mut rl = Editor::new()?;
    rl.set_helper(Some(AnnaCompleter));
    if rl.load_history(ANNA_HISTORY_FILENAME).is_err() {
        println!(
            "No previous history. Saving new history in {}",
            ANNA_HISTORY_FILENAME
        );
    }

    while let Ok(line) = rl.readline("anna> ") {
        let _ = rl.add_history_entry(&line);
        if let Err(e) = execute_command(&mut client, &line, &config_file_path) {
            error!("{}", e);
        }
    }

    rl.save_history(ANNA_HISTORY_FILENAME)?;

    Ok("History saved. Exiting")
}

fn cli_loop_file(
    mut client: KVSClient,
    filename: &str,
    config_file_path: PathBuf,
) -> Result<&'static str> {
    let file = File::open(filename)
        .map_err(|e| CliError::Other(format!("Could not open command file '{}': {}", filename, e)))?;
    let reader = BufReader::new(file);

    for line in reader.lines().flatten() {
        if let Err(e) = execute_command(&mut client, &line, &config_file_path) {
            error!("Error while executing command line: '{}'\n{}", line, e);
        }
    }

    Ok("")
}

fn cli(client: KVSClient, args: &ArgMatches, config_file_path: PathBuf) -> Result<&'static str> {
    match args.get_one::<String>("command_file").map(|s| s.as_str()) {
        None => cli_loop_interactive(client, config_file_path),
        Some(filename) => cli_loop_file(client, filename, config_file_path),
    }
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
            Arg::new("config")
                .short('c')
                .long("config")
                .num_args(1)
                .value_name("CONFIG_FILE")
                .help("Specify a config file to use"),
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
        .subcommand(Command::new("start").about("Start the KVS server processes"))
        .subcommand(Command::new("stop").about("Stop the KVS server processes"))
        .subcommand(Command::new("status").about("Report status of KVS server processes"))
}
