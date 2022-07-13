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
use clap::{App, Arg, ArgMatches, SubCommand};
use log::{debug, error, info, warn};
use rustyline::Editor;
use simplog::SimpleLogger;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const ANNA_HISTORY_FILENAME: &str = ".anna_history";
const DEFAULT_CONFIG_FILENAME: &str = "default-config.yml";

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
}

pub use errors::*;

fn main() {
    match run() {
        Err(ref e) => {
            println!("error: {}", e);

            for e in e.iter().skip(1) {
                println!("caused by: {}", e);
            }

            // The backtrace is generated if env var `RUST_BACKTRACE` is set to `1` or `full`
            if let Some(backtrace) = e.backtrace() {
                println!("backtrace: {:?}", backtrace);
            }
            exit(1);
        }
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{}", msg);
            }
            exit(0)
        }
    }
}

fn get_config_path(args: &ArgMatches) -> Result<PathBuf> {
    match args.value_of("config") {
        Some(config_file) => PathBuf::from(config_file)
            .canonicalize()
            .chain_err(|| "Could not canonicalize config file path"),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(DEFAULT_CONFIG_FILENAME)
            .canonicalize()
            .chain_err(|| "Could not canonicalize config file path"),
    }
}

/*
    run the cli using clap to interpret commands and options
*/
fn run() -> Result<String> {
    debug!(
        "'{}' CLI version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    debug!("'anna' library version {}", info::version());

    let app = get_app();
    let app_clone = app.clone();
    let matches = app.get_matches();

    // Initialize the logger with the level of verbosity requested via option (or the default)
    SimpleLogger::init_prefix(matches.value_of("verbosity"), false);

    let config_file_path = get_config_path(&matches)?;
    info!("Using config file: {}", config_file_path.display());

    let config = Config::read(&config_file_path).chain_err(|| "Could not load config from file")?;
    let kvs_client = KVSClient::new(&config, None);

    match matches
        .subcommand()
        .ok_or("Could not find valid subcommand")?
    {
        ("help", _) => help(app_clone),
        ("start", _) => Ok(format!(
            "{} anna processes were started",
            start(&config_file_path)?
        )),
        ("stop", _) => Ok(format!("{} anna processes were terminated", stop()?)),
        ("cli", arg_matches) => Ok(cli(kvs_client, arg_matches, config_file_path)?.into()),
        (_, _) => Ok("No command executed".into()),
    }
}

fn execute_command(client: &KVSClient, line: &str, config_file_path: &PathBuf) -> Result<()> {
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
        "START" => println!("{} anna processes were started", start(&config_file_path)?),
        "STOP" => println!("{} anna processes were terminated", stop()?),
        "STATUS" => println!("{}", status()?),
        "HELP" => println!("{}", usage()),
        "EXIT" => exit(0),
        _ => bail!("Invalid anna command line: '{}'\n{}", line, usage()),
    }

    Ok(())
}

fn usage() -> String {
    let mut usage = "Valid commands are:\n\tget {{key}}\n\tput {{key}} {{value}}".into();

    #[cfg(feature = "causal")]
    {
        usage = format!(
            "{}\n\tget_causal {{key}}\n\tput_causal {{key}} {{value}}",
            usage
        );
    }

    #[cfg(feature = "set")]
    {
        usage = format!("{}\n\tget_set {{key}}\n\tput_set {{key}} {{set}}", usage);
    }

    usage = format!(
        "{}\n\tstart - start anna processes\n\tstop - stop running anna processes\
        \n\tstatus - print the status of anna processes\n\thelp\n\texit",
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
    match args.value_of("command_file") {
        None => cli_loop_interactive(client, config_file_path),
        Some(filename) => cli_loop_file(client, filename, config_file_path),
    }
}

/*
    The 'help' command
*/
fn help(mut app: App) -> Result<String> {
    app.print_long_help()?;
    Ok("".into())
}

/*
    Create the clap app with the desired options and sub commands
*/
fn get_app() -> App<'static> {
    App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::with_name("verbosity")
                .short('v')
                .long("verbosity")
                .takes_value(true)
                .value_name("VERBOSITY_LEVEL")
                .help("Set verbosity level for output (trace, debug, info, warn, error (default))"),
        )
        .arg(
            Arg::with_name("config")
                .short('c')
                .long("config")
                .takes_value(true)
                .value_name("CONFIG_FILE")
                .help("Specify the config file to be used"),
        )
        .subcommand(
            SubCommand::with_name("cli")
                .about("Start an interactive anna CLI session")
                .arg(
                    Arg::with_name("command_file")
                        .index(1)
                        .help("A file where anna commands are read from"),
                ),
        )
        .subcommand(
            SubCommand::with_name("start")
                .about("Start anna processes (monitor, route and kvs) in background"),
        )
        .subcommand(
            SubCommand::with_name("stop")
                .about("Stop running instances of anna (monitor, route and kvs)"),
        )
}
