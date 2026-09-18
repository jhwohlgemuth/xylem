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
