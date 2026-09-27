use std::fs::{create_dir_all, read, read_dir, read_to_string, remove_dir_all, remove_file, write};

#[test]
fn test_configuration_read_failures_use_distinct_exit_codes() {
    let test_root = std::env::temp_dir().join(format!("xylem-config-errors-{}", std::process::id()));
    create_dir_all(&test_root).expect("create configuration error directory");
    let invalid = test_root.join("invalid.yaml");
    let missing = test_root.join("missing.yaml");
    write(&invalid, "requirements: [\n").expect("write invalid YAML configuration");

    let mut invalid_command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    invalid_command
        .args(["run", "--config", invalid.to_str().expect("UTF-8 invalid path")])
        .assert()
        .failure()
        .code(exitcode::DATAERR);

    let mut missing_command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    missing_command
        .args(["run", "--config", missing.to_str().expect("UTF-8 missing path")])
        .assert()
        .failure()
        .code(exitcode::UNAVAILABLE);

    remove_file(invalid).expect("remove invalid YAML configuration");
    remove_dir_all(test_root).expect("remove configuration error directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_configured_batch_caps_active_inputs_in_the_compiled_cli() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!(
        "xylem-batch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos()
    ));
    let state = test_root.join("state");
    create_dir_all(&state).expect("create batch test state");
    let inputs = test_root.join("imagery-inputs");
    for name in ["one", "two", "three", "four"] {
        create_dir_all(inputs.join(name)).expect("create imagery input directory");
    }
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "version": "1.0.0",
        "requirements": ["sh"],
        "input": inputs.to_string_lossy(),
        "options": { "batch": 2 },
        "modules": [{
            "name": "Concurrency Probe",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": "file:./tests/e2e/modules/concurrency-probe",
            "template": {
                "command": "sh",
                "arguments": ["run.sh", state.to_string_lossy()]
            }
        }]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize batch fixture")).expect("write batch fixture");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .current_dir(workspace)
        .args(["--threads", "10", "run", "--config", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run batched workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "batched workflow failed:\n{logs}");
    assert_eq!(read_to_string(state.join("peak")).expect("peak concurrency").trim(), "2");
    let mut completed = read_to_string(state.join("completed"))
        .expect("completed inputs")
        .lines()
        .map(|input| {
            std::path::Path::new(input)
                .file_name()
                .expect("top-level input name")
                .to_string_lossy()
                .to_string()
        })
        .collect::<Vec<_>>();
    completed.sort();
    assert_eq!(completed, ["four", "one", "three", "two"]);
    remove_dir_all(test_root).expect("remove batch fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_dry_run_renders_a_plan_without_writing_files() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let output_directory = std::env::temp_dir().join(format!("xylem-write-free-dry-run-{}", std::process::id()));
    let _ = remove_dir_all(&output_directory);
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .current_dir(workspace)
        .args([
            "run",
            "--config",
            "tests/e2e/config.json",
            "--artifact-output",
            output_directory.to_str().expect("UTF-8 artifact output"),
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("run dry-run plan");
    assert!(output.status.success(), "dry run failed: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid dry-run plan");
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["configuration"]["output"], output_directory.display().to_string());
    assert_eq!(plan["modules"][0]["decision"], "would_run");
    assert_eq!(
        plan["modules"][0]["variables"]["_XYLEM_OUTPUT_DIRECTORY"],
        output_directory.display().to_string()
    );
    assert!(!output_directory.exists());
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_fail_fast_journal_distinguishes_failed_and_not_started_inputs() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-durable-failure-{}", std::process::id()));
    let output_directory = test_root.join("reports");
    create_dir_all(&output_directory).expect("create durable failure directory");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "version": "1.0.0",
        "requirements": ["sh"],
        "input": ["one", "two", "three"],
        "options": { "batch": 1 },
        "modules": [{
            "name": "Failure Probe",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": "file:./tests/e2e/modules/failure-probe"
        }]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize failure config")).expect("write failure config");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command
        .current_dir(workspace)
        .args([
            "run",
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--reporter",
            "json",
            "--output",
            output_directory.to_str().expect("UTF-8 output path"),
        ])
        .assert()
        .failure()
        .code(exitcode::SOFTWARE);

    let report_path = read_dir(&output_directory)
        .expect("read output directory")
        .map(|entry| entry.expect("directory entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("failure JSON report");
    let report: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(report_path).expect("read failure report")).expect("valid failure report");
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Failed);
    assert_eq!(report.inputs[0].state, xylem_lib::journal::ExecutionState::Failed);
    assert_eq!(report.inputs[1].state, xylem_lib::journal::ExecutionState::NotStarted);
    assert_eq!(report.inputs[2].state, xylem_lib::journal::ExecutionState::NotStarted);
    remove_dir_all(test_root).expect("remove durable failure directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_json_stdout_is_not_corrupted_by_diagnostics() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .current_dir(workspace)
        .args(["run", "--config", "tests/e2e/config.json", "--reporter", "json"])
        .output()
        .expect("run JSON stdout workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: xylem_lib::journal::RunSummary = serde_json::from_slice(&output.stdout).expect("stdout should contain only the JSON report");
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Succeeded);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let journal_path = stderr
        .lines()
        .find_map(|line| line.strip_prefix("Journal: "))
        .map(std::path::PathBuf::from)
        .expect("stderr should identify the durable journal");
    let run_directory = journal_path.parent().expect("journal should have a parent");
    assert!(journal_path.is_file());
    remove_dir_all(run_directory).expect("remove default run directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_loads_an_explicit_dotenv_without_overriding_process_values() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let env_path = std::env::temp_dir().join(format!("xylem-dotenv-{}.env", std::process::id()));
    write(&env_path, "XYLEM_DOTENV_TEST=from-file\n").expect("write dotenv fixture");
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
    let _ = remove_file(env_path);
}
#[test]
fn test_offline_dry_run_skips_reachability_but_still_enforces_allowlist() {
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
#[test]
fn test_preparation_failure_is_finalized_and_reported() {
    let test_root = std::env::temp_dir().join(format!("xylem-durable-preparation-{}", std::process::id()));
    let output_directory = test_root.join("reports");
    create_dir_all(&output_directory).expect("create preparation failure directory");
    let config_path = test_root.join("config.json");
    let missing_requirement = format!("xylem-command-that-does-not-exist-{}", std::process::id());
    let config = serde_json::json!({
        "version": "1.0.0",
        "requirements": [missing_requirement],
        "modules": []
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize preparation config")).expect("write preparation config");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command
        .args([
            "run",
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--reporter",
            "json",
            "--output",
            output_directory.to_str().expect("UTF-8 output path"),
        ])
        .assert()
        .failure()
        .code(exitcode::UNAVAILABLE);
    let report_path = read_dir(&output_directory)
        .expect("read preparation output directory")
        .map(|entry| entry.expect("directory entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("preparation failure JSON report");
    let report: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(report_path).expect("read preparation report")).expect("valid preparation report");
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Failed);
    assert_eq!(report.inputs[0].state, xylem_lib::journal::ExecutionState::NotStarted);
    assert!(report
        .error
        .as_deref()
        .is_some_and(|error| error.contains("required command was not found")));
    remove_dir_all(test_root).expect("remove preparation failure directory");
}
#[test]
fn test_rejects_a_malformed_explicit_dotenv_file() {
    let env_path = std::env::temp_dir().join(format!("xylem-malformed-dotenv-{}.env", std::process::id()));
    write(&env_path, "=broken\n").expect("write malformed dotenv fixture");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["--env", env_path.to_str().expect("UTF-8 path")])
        .output()
        .expect("run with malformed dotenv");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not load dotenv file"));
    let _ = remove_file(env_path);
}
#[test]
fn test_rejects_a_missing_explicit_dotenv_file() {
    let missing = std::env::temp_dir().join(format!("xylem-missing-dotenv-{}.env", std::process::id()));
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["--env", missing.to_str().expect("UTF-8 path")])
        .output()
        .expect("run with missing dotenv");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not load dotenv file"));
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_resume_skips_a_complete_module_without_spawning_it() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-resume-{}", std::process::id()));
    let artifacts = test_root.join("artifacts");
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    create_dir_all(&module).expect("create module directory");
    create_dir_all(&reports).expect("create report directory");
    let counter = test_root.join("counter");
    let command = format!(
        "printf 'run\\n' >> {}; mkdir -p {}; printf 'complete\\n' > {}/result.txt",
        counter.display(),
        artifacts.display(),
        artifacts.display()
    );
    let config = serde_json::json!({
        "modules": [{
            "completion": {
                "marker": "{{ OUTPUT_DIRECTORY }}/.xylem/probe.json",
                "outputs": ["{{ OUTPUT_DIRECTORY }}/*.txt"]
            },
            "name": "Resume Probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["-c", command], "command": "sh" },
            "type": "script",
            "uri": format!("file:{}", module.display()),
            "variables": { "OUTPUT_DIRECTORY": artifacts.display().to_string() }
        }],
        "requirements": ["sh"],
        "version": "1.0.0"
    });
    let config_path = test_root.join("config.json");
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize resume fixture")).expect("write resume fixture");
    for arguments in [vec![], vec!["--resume"]] {
        let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
        command
            .current_dir(workspace)
            .args([
                "run",
                "--config",
                config_path.to_str().expect("UTF-8 config"),
                "--output",
                reports.to_str().expect("UTF-8 reports"),
            ])
            .args(arguments)
            .assert()
            .success();
    }
    assert_eq!(read_to_string(&counter).expect("read invocation counter").lines().count(), 1);
    let reports = read_dir(&reports)
        .expect("read reports")
        .map(|entry| entry.expect("report entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().any(|path| {
        xylem_lib::journal::RunJournal::read(path).is_ok_and(|summary| summary.inputs[0].state == xylem_lib::journal::ExecutionState::SkippedExisting)
    }));
    remove_dir_all(test_root).expect("remove resume fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_returns_failure_when_a_fixture_module_fails() {
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
fn test_runs_fixture_modules_in_order_with_interpolated_environment() {
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
fn test_writes_versioned_journal_and_json_report_from_the_same_summary() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let output_directory = std::env::temp_dir().join(format!("xylem-durable-success-{}", std::process::id()));
    create_dir_all(&output_directory).expect("create durable output directory");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command
        .current_dir(workspace)
        .args([
            "run",
            "--config",
            "tests/e2e/config.json",
            "--reporter",
            "json,json",
            "--output",
            output_directory.to_str().expect("UTF-8 output path"),
        ])
        .assert()
        .success();
    let files = read_dir(&output_directory)
        .expect("read output directory")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    let report_path = files
        .iter()
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("JSON report");
    let journal_path = files
        .iter()
        .find(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .expect("JSONL journal");
    let report: xylem_lib::journal::RunSummary = serde_json::from_slice(&read(report_path).expect("read JSON report")).expect("valid JSON report");
    let recovered = xylem_lib::journal::RunJournal::read(journal_path).expect("recover journal");
    assert_eq!(report, recovered);
    assert_eq!(report.schema_version, xylem_lib::journal::JOURNAL_SCHEMA_VERSION);
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Succeeded);
    assert!(report
        .inputs
        .iter()
        .all(|input| input.state == xylem_lib::journal::ExecutionState::Succeeded));
    remove_dir_all(output_directory).expect("remove durable output directory");
}
