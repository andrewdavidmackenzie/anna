#![warn(clippy::unwrap_used)]
//! `anna` is a command line tool for working with the `anna` key-value store
//!
//! Execute `anna` or `anna --help` or `anna -h` at the comment line for a
//! description of the command line options.

#[macro_use]
extern crate error_chain;

use std::env;
use std::process::exit;

use annalib::{config::Config, info, kvs_client::KVSClient, start, stop};
use clap::{App, Arg, ArgMatches, SubCommand};
use log::{debug, info, warn};
use rustyline::Editor;
use simplog::simplog::SimpleLogger;
use std::fs::File;
use std::io::{BufRead, BufReader};

const ANNA_HISTORY_FILENAME: &str = ".anna_history";
const DEFAULT_CONFIG_FILENAME: &str = "conf/anna-config.yml";

// We'll put our errors in an `errors` module, and other modules in this crate will
// `use crate::errors::*;` to get access to everything `error_chain!` creates.
#[doc(hidden)]
pub mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

#[doc(hidden)]
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

    let config_file = matches
        .value_of("config")
        .unwrap_or(DEFAULT_CONFIG_FILENAME);
    info!("Using config file: {}", config_file);

    let config = Config::read(&config_file)
        .chain_err(|| format!("Could not read config file: {}", config_file))?;

    let kvs_client = KVSClient::new(&config, None, None);

    match matches.subcommand() {
        ("help", _) => help(app_clone),
        ("start", _) => Ok(format!("{} anna processes were started", start(&config)?)),
        ("stop", _) => Ok(format!("{} anna processes were terminated", stop()?)),
        ("cli", None) => Ok(cli_loop_interactive(kvs_client)?.into()),
        ("cli", Some(args)) => Ok(cli_loop(kvs_client, args)?.into()),
        (_, _) => Ok("No command executed".into()),
    }
}

fn execute_command(line: &str, client: &KVSClient) {
    let split = line.split(' ').collect::<Vec<&str>>();
    match (split[0].to_ascii_uppercase().as_str(), &split[1..]) {
        // ("GET", tokens) => println!("{}", client.get(tokens)), // TODO
        ("GET", tokens) => client.get(tokens),
        ("GET_CAUSAL", tokens) => client.get_causal(tokens),
        ("PUT", tokens) => client.put(tokens),
        ("PUT_CAUSAL", tokens) => client.put_causal(tokens),
        ("PUT_SET", tokens) => client.put_set(tokens),
        ("GET_SET", tokens) => client.get_set(tokens),
        (command, _) => eprintln!("Unrecognized anna command: {}. Was ignored.", command),
    }
}

/*
    Enter a loop of command/response for the CLI and interact with the server processes for each
*/
fn cli_loop_interactive(client: KVSClient) -> Result<&'static str> {
    let mut rl = Editor::<()>::new(); // `()` can be used when no completer is required
    if rl.load_history(ANNA_HISTORY_FILENAME).is_err() {
        println!(
            "No previous history. Saving new history in {}",
            ANNA_HISTORY_FILENAME
        );
    }

    loop {
        match rl.readline("anna> ") {
            Ok(line) => {
                rl.add_history_entry(&line);
                execute_command(&line, &client);
            }
            Err(_) => break, // Includes CONTROL-C and CONTROL-D exits
        }
    }

    rl.save_history(ANNA_HISTORY_FILENAME)?;

    Ok("History saved. Exiting")
}

/*
    Enter a loop of command/response for the CLI and interact with the server processes for each
*/
fn cli_loop_file(client: KVSClient, filename: &str) -> Result<&'static str> {
    let file = File::open(filename)
        .chain_err(|| format!("Could not open the command_file: {}", filename))?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        if let Ok(ref string) = line {
            execute_command(&string, &client);
        }
    }

    Ok("")
}

/*
   Try to parse and then open a command_file of anna commands
*/
fn cli_loop(client: KVSClient, args: &ArgMatches) -> Result<&'static str> {
    match args.value_of("command_file") {
        None => cli_loop_interactive(client),
        Some(filename) => cli_loop_file(client, filename),
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
fn get_app() -> App<'static, 'static> {
    App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .long("verbosity")
                .takes_value(true)
                .value_name("VERBOSITY_LEVEL")
                .help("Set verbosity level for output (trace, debug, info, warn, error (default))"),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
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
