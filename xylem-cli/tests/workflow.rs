#[cfg(not(target_os = "windows"))]
#[test]
fn runs_fixture_modules_in_order_with_interpolated_environment() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    for threads in ["1", "3"] {
        let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
        let output = command
            .current_dir(workspace)
            .args(["--threads", threads, "-vvvv", "run", "--config", "tests/e2e/config.json"])
            .output()
            .expect("xylem fixture workflow should run");
        let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "workflow failed with {threads} threads:\n{logs}");
        let first = logs.find("XYLEM_E2E_ONE_OK").expect("first fixture module should succeed");
        let second = logs.find("XYLEM_E2E_TWO_OK").expect("second fixture module should succeed");
        assert!(first < second, "fixture modules ran out of order with {threads} threads:\n{logs}");
    }
}
#[cfg(not(target_os = "windows"))]
#[test]
fn returns_failure_when_a_fixture_module_fails() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command
        .current_dir(workspace)
        .args(["-vv", "run", "--config", "tests/e2e/failing-config.json"])
        .assert()
        .failure()
        .code(exitcode::SOFTWARE);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn loads_an_explicit_dotenv_without_overriding_process_values() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let env_path = std::env::temp_dir().join(format!("xylem-dotenv-{}.env", std::process::id()));
    std::fs::write(&env_path, "XYLEM_DOTENV_TEST=from-file\n").expect("write dotenv fixture");

    let mut from_file = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = from_file
        .current_dir(workspace)
        .args([
            "--env",
            env_path.to_str().expect("UTF-8 path"),
            "-vvvv",
            "run",
            "--config",
            "tests/e2e/dotenv-config.json",
        ])
        .output()
        .expect("run dotenv workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "dotenv workflow failed:\n{logs}");
    assert!(logs.contains("XYLEM_DOTENV_TEST=from-file"), "{logs}");
    assert!(
        !logs.contains("Preparing modules"),
        "non-TTY output should not contain progress rendering: {logs}"
    );

    let mut from_process = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = from_process
        .current_dir(workspace)
        .env("XYLEM_DOTENV_TEST", "from-process")
        .args([
            "run",
            "--env",
            env_path.to_str().expect("UTF-8 path"),
            "-vvvv",
            "--config",
            "tests/e2e/dotenv-config.json",
        ])
        .output()
        .expect("run dotenv precedence workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "dotenv precedence workflow failed:\n{logs}");
    assert!(logs.contains("XYLEM_DOTENV_TEST=from-process"), "{logs}");
    let _ = std::fs::remove_file(env_path);
}

#[test]
fn rejects_a_missing_explicit_dotenv_file() {
    let missing = std::env::temp_dir().join(format!("xylem-missing-dotenv-{}.env", std::process::id()));
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["--env", missing.to_str().expect("UTF-8 path")])
        .output()
        .expect("run with missing dotenv");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not load dotenv file"));
}

#[test]
fn rejects_a_malformed_explicit_dotenv_file() {
    let env_path = std::env::temp_dir().join(format!("xylem-malformed-dotenv-{}.env", std::process::id()));
    std::fs::write(&env_path, "=broken\n").expect("write malformed dotenv fixture");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["--env", env_path.to_str().expect("UTF-8 path")])
        .output()
        .expect("run with malformed dotenv");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not load dotenv file"));
    let _ = std::fs::remove_file(env_path);
}

#[test]
fn offline_dry_run_skips_reachability_but_still_enforces_allowlist() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let config = "tests/e2e/offline-remote-config.json";

    let mut allowed = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    allowed
        .current_dir(workspace)
        .args(["run", "--config", config, "--offline", "--dry-run", "--allowed", "example.com"])
        .assert()
        .success();

    let mut rejected = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    rejected
        .current_dir(workspace)
        .args(["run", "--config", config, "--offline", "--dry-run", "--allowed", "other.example"])
        .assert()
        .failure()
        .code(exitcode::UNAVAILABLE);
}
