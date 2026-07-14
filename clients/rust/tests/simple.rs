mod common;

use common::{anna_binary, config_file, server_path, start_servers, stop_servers, ServerGuard};
use std::fs::File;
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

        eprintln!("Expected:\n{}", expected_norm);
        eprintln!("Actual:\n{}", actual_norm);

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

        let non_profiling: String = contents
            .lines()
            .filter(|l| !l.starts_with("profiling:"))
            .collect::<Vec<_>>()
            .join("\n");

        if !non_profiling.trim().is_empty() {
            panic!(
                "Test {} produced output to STDERR:\n{}",
                test_dir.display(),
                non_profiling
            );
        }
    }

    compare_and_fail(test_dir.join("expected"), test_dir.join("test.output"));
}

fn run_test(test_dir: &Path, path: &str, config_file: &str) -> io::Result<Output> {
    let _ = fs::remove_file(test_dir.join("test.err"));
    let _ = fs::remove_file(test_dir.join("test.output"));

    let input = test_dir.join("input");
    let output = File::create(test_dir.join("test.output"))?;
    let error = File::create(test_dir.join("test.err"))?;

    let anna = anna_binary();

    Command::new(&anna)
        .args(["--config", config_file, "cli", input.to_str().unwrap()])
        .env("PATH", path)
        .current_dir(test_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error))
        .output()
}

fn test(name: &str) -> io::Result<()> {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);

    let config = config_file();
    let path = server_path();

    start_servers(&path, &config);
    let _guard = ServerGuard {
        path: path.clone(),
        config: config.clone(),
    };

    let test_run = run_test(&test_dir, &path, &config);
    test_run?;

    check_test_output(&test_dir);

    let _ = fs::remove_file(test_dir.join("test.err"));
    let _ = fs::remove_file(test_dir.join("test.output"));
    let _ = fs::remove_file(test_dir.join("client_log.txt"));
    let _ = fs::remove_file(test_dir.join("log.txt"));
    let _ = fs::remove_file(test_dir.join("log_0.txt"));

    Ok(())
}

#[test]
#[cfg(unix)]
fn simple_test() {
    test("simple").expect("simple_test failed");
}
