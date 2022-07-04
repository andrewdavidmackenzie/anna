use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, fs, io};

fn compare_and_fail(expected_path: PathBuf, actual_path: PathBuf) {
    if expected_path.exists() {
        let diff = Command::new("diff")
            .args(vec![&expected_path, &actual_path])
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .spawn()
            .expect("Could not get child process");
        let output = diff
            .wait_with_output()
            .expect("Could not get child process output");
        if output.status.success() {
            return;
        }
        panic!(
            "Contents of '{}' doesn't match the expected contents in '{}'",
            actual_path.display(),
            expected_path.display()
        );
    }
}

fn check_test_output(test_dir: &Path) {
    let error_output = test_dir.join("test.err");
    if error_output.exists() {
        let contents =
            fs::read_to_string(&error_output).expect("Could not read from 'test.err' file");

        if !contents.is_empty() {
            panic!(
                "Test {} produced output to STDERR {}",
                test_dir.display(),
                contents
            );
        }
    }

    compare_and_fail(test_dir.join("expected"), test_dir.join("test.output"));
}

fn run_test(test_dir: &Path, path: &str, config_file: &str) -> io::Result<Output> {
    // Remove any previous output
    let _ = fs::remove_file(test_dir.join("test.err"));
    let _ = fs::remove_file(test_dir.join("test.output"));

    println!("Running test: {:?}", test_dir);

    let input = test_dir.join("input");
    let output = File::create(test_dir.join("test.output"))?;
    let error = File::create(test_dir.join("test.err"))?;

    let command_args = vec!["--config", config_file, input.to_str().unwrap()];

    Command::new("anna-cli")
        .args(command_args)
        .env("PATH", path)
        .current_dir(test_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error))
        .output()
}

fn get_paths() -> io::Result<(String, String)> {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap(); // project_root/client
    root_dir = root_dir.parent().unwrap(); // project_root

    let path = format!(
        "{}:{}:{}",
        env::var("PATH").unwrap(),
        root_dir
            .join("build/target/kvs")
            .as_path()
            .to_string_lossy(),
        root_dir.join("build/cli").as_path().to_string_lossy(),
    );

    let config_file = root_dir.join("conf/anna-config.yml");
    println!("Using config file '{}'", config_file.to_string_lossy());

    Ok((path, config_file.to_string_lossy().to_string()))
}

fn ctl_anna_processes(
    test_dir: &Path,
    anna_command: &str,
    path: &str,
    config_file: &str,
) -> Result<(), String> {
    println!(
        "Controlling anna background processes using rust cli: '{}'",
        anna_command
    );

    let cargo_args = vec![
        "run",
        "--quiet",
        "--",
        "--config",
        config_file,
        anna_command,
    ];

    let mut cargo = Command::new("cargo");
    cargo
        .args(cargo_args)
        .env("PATH", path)
        .current_dir(test_dir);

    match cargo.spawn() {
        Ok(_) => Ok(()),
        Err(e) => {
            match e.kind() {
                ErrorKind::NotFound => {
                    eprintln!(
                        "`cargo` was not found! Check your $PATH. {:?}",
                        cargo.get_envs()
                    )
                }
                _ => eprintln!("Unexpected error running `cargo`: {}", e),
            }
            Err(e.to_string())
        }
    }
}

fn test(name: &str) -> io::Result<()> {
    println!("CWD = {}", env::current_dir()?.display());

    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);

    println!("test_dir = {}", test_dir.display());

    let (path, config_file) = get_paths()?;

    ctl_anna_processes(&test_dir, "start", &path, &config_file)
        .expect("Could not start anna processes");

    let test_run = run_test(&test_dir, &path, &config_file);

    // always stop the anna processes we started in background
    ctl_anna_processes(&test_dir, "stop", &path, &config_file)
        .expect("Could not stop anna processes");

    test_run?;

    check_test_output(&test_dir);

    // if test passed, remove output
    let _ = fs::remove_file(test_dir.join("test.err"));
    let _ = fs::remove_file(test_dir.join("test.output"));
    // and remove misc log files generated
    let _ = fs::remove_file(test_dir.join("client_log.txt"));
    let _ = fs::remove_file(test_dir.join("log.txt"));
    let _ = fs::remove_file(test_dir.join("log_0.txt"));

    Ok(())
}

#[test]
fn simple_test() {
    test("simple").expect("simple_test failed");
    /*

    DIFF=$(diff tests/simple/output tests/simple/expected)

    if [ "$DIFF" != "" ]; then
    echo "Output did not match expected output (tests/simple/expected). Diff:"
    echo "$DIFF"
    exit 1
    else
    echo "Test succeeded!"
    fi

    # Cleanup
    rm tests/simple/output

    echo "Stopping local server..."
    PATH=$PATH:"./build/target/kvs" cargo run --quiet -- --config ./conf/anna-config.yml stop

    exit 0*/
}
