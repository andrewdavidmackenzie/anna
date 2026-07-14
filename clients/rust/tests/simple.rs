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

fn anna_binary() -> PathBuf {
    let mut path = env::current_exe()
        .expect("Could not get test executable path");
    // test binary is in target/debug/deps/ — anna binary is in target/debug/
    path.pop(); // remove binary name
    if path.ends_with("deps") {
        path.pop(); // remove deps/
    }
    path.join("anna")
}

fn server_path() -> String {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap(); // clients
    root_dir = root_dir.parent().unwrap(); // project root

    let server_dir = root_dir.join("server/cpp/build/target/kvs");
    format!(
        "{}:{}",
        env::var("PATH").unwrap(),
        server_dir.to_string_lossy(),
    )
}

fn get_config_file() -> String {
    let mut root_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    root_dir = root_dir.parent().unwrap();
    root_dir = root_dir.parent().unwrap();

    let config_file = root_dir.join("conf/anna-config.yml");
    config_file.to_string_lossy().to_string()
}

fn run_test(test_dir: &Path, path: &str, config_file: &str) -> io::Result<Output> {
    let _ = fs::remove_file(test_dir.join("test.err"));
    let _ = fs::remove_file(test_dir.join("test.output"));

    println!("Running test: {:?}", test_dir);

    let input = test_dir.join("input");
    let output = File::create(test_dir.join("test.output"))?;
    let error = File::create(test_dir.join("test.err"))?;

    let anna = anna_binary();
    println!("Using anna binary: {}", anna.display());

    Command::new(&anna)
        .args(["--config", config_file, "cli", input.to_str().unwrap()])
        .env("PATH", path)
        .current_dir(test_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error))
        .output()
}

fn ctl_anna_processes(anna_command: &str, path: &str, config_file: &str) -> Result<(), String> {
    println!("Controlling anna processes: '{}'", anna_command);

    let anna = anna_binary();
    let status = Command::new(&anna)
        .args(["--config", config_file, anna_command])
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run anna: {}", e))?;

    if !status.success() {
        return Err(format!("'anna {}' exited with {}", anna_command, status));
    }
    Ok(())
}

fn test(name: &str) -> io::Result<()> {
    println!("CWD = {}", env::current_dir()?.display());

    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);

    let config_file = get_config_file();
    let path = server_path();

    ctl_anna_processes("start", &path, &config_file)
        .expect("Could not start anna processes");

    // Wait for routing tier to be ready (port 6450)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if std::net::TcpStream::connect_timeout(
            &"127.0.0.1:6450".parse().unwrap(),
            std::time::Duration::from_secs(1),
        )
        .is_ok()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("Routing tier did not start within 30 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    // Brief settle time for hash ring registration
    std::thread::sleep(std::time::Duration::from_secs(1));

    let test_run = run_test(&test_dir, &path, &config_file);

    ctl_anna_processes("stop", &path, &config_file)
        .expect("Could not stop anna processes");

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
