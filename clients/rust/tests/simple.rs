use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, fs, io};

fn normalize_sets(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                let inner = &trimmed[1..trimmed.len() - 1];
                let elements: Vec<&str> = inner.split_whitespace().collect();
                if !elements.is_empty() && elements.iter().all(|e| e.parse::<i32>().is_ok()) {
                    let mut sorted: Vec<i32> =
                        elements.iter().filter_map(|e| e.parse().ok()).collect();
                    sorted.sort();
                    return format!(
                        "{{ {} }}",
                        sorted
                            .iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compare_and_fail(expected_path: PathBuf, actual_path: PathBuf) {
    if expected_path.exists() {
        let expected_content =
            fs::read_to_string(&expected_path).expect("Could not read expected file");
        let actual_content = fs::read_to_string(&actual_path).expect("Could not read actual file");

        let expected_norm = normalize_sets(&expected_content);
        let actual_norm = normalize_sets(&actual_content);

        if expected_norm == actual_norm {
            return;
        }

        let expected_tmp = expected_path.parent().unwrap().join("expected.norm");
        let actual_tmp = expected_path.parent().unwrap().join("actual.norm");
        fs::write(&expected_tmp, &expected_norm).expect("Could not write normalized expected");
        fs::write(&actual_tmp, &actual_norm).expect("Could not write normalized actual");

        let diff = Command::new("diff")
            .args(vec![&expected_tmp, &actual_tmp])
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .spawn()
            .expect("Could not get child process");
        let output = diff
            .wait_with_output()
            .expect("Could not get child process output");

        let _ = fs::remove_file(&expected_tmp);
        let _ = fs::remove_file(&actual_tmp);

        if !output.status.success() {
            panic!(
                "Contents of '{}' doesn't match the expected contents in '{}'",
                actual_path.display(),
                expected_path.display()
            );
        }
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

    let command_args = vec!["--config", config_file, "cli", input.to_str().unwrap()];

    Command::new("anna-cli")
        .args(command_args)
        .env("PATH", path)
        .current_dir(test_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error))
        .output()
}

fn get_cpp_paths() -> io::Result<String> {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap(); // project_root/clients
    root_dir = root_dir.parent().unwrap(); // project_root

    let path = format!(
        "{}:{}:{}",
        env::var("PATH").unwrap(),
        root_dir
            .join("server/cpp/build/target/kvs")
            .as_path()
            .to_string_lossy(),
        root_dir
            .join("clients/cpp/build/cli")
            .as_path()
            .to_string_lossy(),
    );

    Ok(path)
}

fn get_config_file() -> io::Result<String> {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap(); // project_root/clients
    root_dir = root_dir.parent().unwrap(); // project_root

    let config_file = root_dir.join("conf/anna-config.yml");
    println!("Using config file '{}'", config_file.to_string_lossy());

    Ok(config_file.to_string_lossy().to_string())
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

    let config_file = get_config_file()?;
    let path = get_cpp_paths()?;

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
}
