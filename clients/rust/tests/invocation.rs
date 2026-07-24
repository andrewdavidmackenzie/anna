use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("foo");
    cmd.assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand 'foo'"));

    Ok(())
}

#[test]
fn start_without_server_config() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("start");
    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--server-config"));

    Ok(())
}

#[test]
fn help_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("help");
    cmd.assert().success();

    Ok(())
}

#[test]
fn debug_contains_lib_version() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("-v").arg("debug");
    cmd.assert().stdout(predicate::str::contains("CLI version"));

    Ok(())
}

#[test]
fn help_contains_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("help");
    cmd.assert().stdout(predicate::str::contains("--verbosity"));
    cmd.assert().stdout(predicate::str::contains("--routing"));
    cmd.assert()
        .stdout(predicate::str::contains("--server-config"));
    cmd.assert().stdout(predicate::str::contains("--help"));
    cmd.assert().stdout(predicate::str::contains("--version"));
    cmd.assert().stdout(predicate::str::contains("cli"));
    cmd.assert().stdout(predicate::str::contains("help"));
    cmd.assert().stdout(predicate::str::contains("start"));
    cmd.assert().stdout(predicate::str::contains("status"));
    cmd.assert().stdout(predicate::str::contains("stop"));

    Ok(())
}

#[test]
fn status_works() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("anna-kvs"));
    cmd.assert()
        .stdout(predicate::str::contains("anna-monitor"));
    cmd.assert().stdout(predicate::str::contains("anna-route"));

    Ok(())
}

#[test]
fn status_single_component() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.args(["status", "kvs"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("anna-kvs"));
    cmd.assert()
        .stdout(predicate::str::contains("anna-monitor").not());
    cmd.assert()
        .stdout(predicate::str::contains("anna-route").not());

    Ok(())
}

#[test]
fn status_invalid_component() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.args(["status", "bogus"]);
    cmd.assert().failure().code(2);

    Ok(())
}

#[test]
fn stop_kills_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("stop");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 anna processes"));

    Ok(())
}

#[test]
fn stop_single_component_kills_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.args(["stop", "kvs"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 anna processes"));

    Ok(())
}

#[test]
fn start_with_component_requires_server_config() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.args(["start", "kvs"]);
    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--server-config"));

    Ok(())
}
