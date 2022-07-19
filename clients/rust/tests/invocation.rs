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
            "Found argument 'foo' which wasn't expected",
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
        .stderr(predicate::str::contains("error: Config file error"));

    Ok(())
}
