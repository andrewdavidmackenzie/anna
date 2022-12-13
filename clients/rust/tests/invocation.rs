use std::io;
use std::path::Path;
use std::process::Command;

use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions

fn config_path() -> io::Result<String> {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap(); // project_root/clients
    root_dir = root_dir.parent().unwrap(); // project_root

    Ok(root_dir
        .join("conf/anna-config.yml")
        .to_string_lossy()
        .to_string())
}

#[test]
fn invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("--config").arg(config_path()?).arg("foo");
    cmd.assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "The subcommand 'foo' wasn't recognized",
        ));

    Ok(())
}

#[test]
fn file_doesnt_exist() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("--config")
        .arg("test/file/doesnt/exist")
        .arg("start");
    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("config file path"));

    Ok(())
}

#[test]
fn default_config_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("help");
    cmd.assert().success();

    Ok(())
}

#[test]
fn help_contains_version() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "anna {}",
            env!("CARGO_PKG_VERSION")
        )));

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
fn help_contains_command() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("help");
    cmd.assert().stdout(predicate::str::contains("--verbosity"));
    cmd.assert().stdout(predicate::str::contains("--config"));
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

    cmd.arg("--config")
        .arg("test/file/doesnt/exist")
        .arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("anna-kvs"));
    cmd.assert()
        .stdout(predicate::str::contains("anna-monitor"));
    cmd.assert().stdout(predicate::str::contains("anna-route"));

    Ok(())
}

#[test]
fn stop_kills_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("anna")?;

    cmd.arg("--config")
        .arg("test/file/doesnt/exist")
        .arg("stop");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 anna processes"));

    Ok(())
}
