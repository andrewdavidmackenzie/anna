use std::env;
use std::path::PathBuf;
use std::process::Command;

fn anna_binary() -> PathBuf {
    let mut path = env::current_exe().expect("Could not get test executable path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("anna")
}

fn repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop(); // clients
    root.pop(); // repo root
    root
}

#[test]
#[cfg(unix)]
fn cli_golden_tests() {
    let shared_runner = repo_root()
        .join("tests")
        .join("shared")
        .join("cli")
        .join("run_smoke_test.py");
    let anna = anna_binary();

    let result = Command::new("python3")
        .arg(&shared_runner)
        .arg(anna.to_str().unwrap())
        .args(["--server-config", "{CONFIG}", "cli"])
        .output()
        .expect("Failed to run shared smoke test");

    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    if !result.status.success() {
        panic!(
            "Shared smoke test failed (exit {}):\nstdout: {}\nstderr: {}",
            result.status, stdout, stderr
        );
    }
}
