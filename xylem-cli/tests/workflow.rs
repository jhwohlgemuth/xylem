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
fn test_dry_run_plans_managed_container_without_starting_runtime() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-managed-plan-{}", std::process::id()));
    let _ = remove_dir_all(&test_root);
    create_dir_all(&test_root).expect("create managed plan fixture");
    let config = test_root.join("config.json");
    let document = serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "managed",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": format!("file:{}", test_root.display()),
            "template": {
                "command": "echo",
                "arguments": ["hello"],
                "container": {
                    "name": "worker",
                    "create": true,
                    "image": "alpine:3.22",
                    "mounts": [{"source": ".", "target": "/workspace"}]
                }
            }
        }]
    });
    write(&config, serde_json::to_vec_pretty(&document).expect("serialize fixture")).expect("write managed plan fixture");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .current_dir(workspace)
        .args([
            "run",
            "--config",
            config.to_str().expect("UTF-8 configuration path"),
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("run managed dry-run plan");
    assert!(output.status.success(), "dry run failed: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid dry-run plan");
    assert_eq!(plan["containers"][0]["action"], "would_create");
    assert_eq!(plan["containers"][0]["name"], "worker");
    assert_eq!(plan["containers"][0]["mounts"][0]["source"], test_root.display().to_string());
    remove_dir_all(test_root).expect("remove managed plan fixture");
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
    assert_eq!(plan["schema_version"], 3);
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
#[cfg(not(target_os = "windows"))]
#[test]
fn test_partial_multi_input_failure_preserves_completed_and_unstarted_inputs() {
    let test_root = std::env::temp_dir().join(format!("xylem-partial-input-failure-{}", std::process::id()));
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    create_dir_all(&module).expect("create partial failure module");
    create_dir_all(&reports).expect("create partial failure reports");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "input": ["one", "two", "three"],
        "modules": [{
            "name": "Selective Failure",
            "programmingLanguage": "generic",
            "template": {
                "arguments": ["-c", "if [ \"$_XYLEM_INPUT\" = two ]; then exit 7; fi"],
                "command": "sh"
            },
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "options": { "batch": 1 },
        "requirements": ["sh"]
    });
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize partial failure config"),
    )
    .expect("write partial failure config");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command
        .args([
            "run",
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--output",
            reports.to_str().expect("UTF-8 reports path"),
        ])
        .assert()
        .failure()
        .code(exitcode::SOFTWARE);
    let journal = read_dir(&reports)
        .expect("read partial failure reports")
        .map(|entry| entry.expect("report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .expect("partial failure journal");
    let summary = xylem_lib::journal::RunJournal::read(journal).expect("read partial failure journal");
    assert_eq!(
        summary.inputs.iter().map(|input| input.state).collect::<Vec<_>>(),
        [
            xylem_lib::journal::ExecutionState::Succeeded,
            xylem_lib::journal::ExecutionState::Failed,
            xylem_lib::journal::ExecutionState::NotStarted
        ]
    );
    remove_dir_all(test_root).expect("remove partial failure directory");
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
#[test]
fn test_rejects_an_invalid_module_location_with_context() {
    let test_root = std::env::temp_dir().join(format!("xylem-invalid-location-{}", std::process::id()));
    create_dir_all(&test_root).expect("create invalid location directory");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "modules": [{
            "name": "Invalid Location",
            "programmingLanguage": "generic",
            "type": "script",
            "uri": "not a module URI"
        }],
        "requirements": []
    });
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize invalid location config"),
    )
    .expect("write invalid location config");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["run", "--config", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run invalid location workflow");
    assert_eq!(output.status.code(), Some(exitcode::DATAERR));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("modules[0].uri"), "{stderr}");
    remove_dir_all(test_root).expect("remove invalid location directory");
}
#[test]
fn test_resume_logs_when_a_module_has_no_completion_declaration() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .current_dir(workspace)
        .args(["-vvvv", "run", "--config", "tests/e2e/config.json", "--dry-run", "--resume"])
        .output()
        .expect("render resume plan");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), stderr);
    assert!(
        logs.contains("Resume plan will run module because no completion declaration is configured"),
        "{logs}"
    );
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
fn test_resume_skips_early_complete_module_and_runs_later_incomplete_module() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-resume-partial-{}", std::process::id()));
    let artifacts = test_root.join("artifacts");
    let first_counter = test_root.join("first-counter");
    let first_module = test_root.join("first-module");
    let reports = test_root.join("reports");
    let second_counter = test_root.join("second-counter");
    let second_module = test_root.join("second-module");
    create_dir_all(&first_module).expect("create first module directory");
    create_dir_all(&reports).expect("create report directory");
    create_dir_all(&second_module).expect("create second module directory");
    let first_command = format!(
        "printf 'run\\n' >> {}; mkdir -p {}; printf 'first\\n' > {}/first.txt",
        first_counter.display(),
        artifacts.display(),
        artifacts.display()
    );
    let second_command = format!(
        "printf 'run\\n' >> {}; mkdir -p {}; printf 'second\\n' > {}/second.txt",
        second_counter.display(),
        artifacts.display(),
        artifacts.display()
    );
    let module = |name: &str, uri: &std::path::Path, marker: &str, output: &str, command: String| {
        serde_json::json!({
            "completion": {
                "marker": format!("{{{{ OUTPUT_DIRECTORY }}}}/.xylem/{marker}.json"),
                "outputs": [format!("{{{{ OUTPUT_DIRECTORY }}}}/{output}.txt")]
            },
            "name": name,
            "programmingLanguage": "generic",
            "template": { "arguments": ["-c", command], "command": "sh" },
            "type": "script",
            "uri": format!("file:{}", uri.display()),
            "variables": { "OUTPUT_DIRECTORY": artifacts.display().to_string() }
        })
    };
    let config = serde_json::json!({
        "modules": [
            module("First Resume Probe", &first_module, "first", "first", first_command),
            module("Second Resume Probe", &second_module, "second", "second", second_command)
        ],
        "requirements": ["sh"],
        "version": "1.0.0"
    });
    let config_path = test_root.join("config.json");
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize partial resume fixture"),
    )
    .expect("write partial resume fixture");
    let run = |resume: bool| {
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
            .args(resume.then_some("--resume"))
            .assert()
            .success();
    };
    run(false);
    remove_file(artifacts.join("second.txt")).expect("remove later output");
    remove_dir_all(&first_module).expect("remove completed module source");
    run(true);
    assert_eq!(read_to_string(first_counter).expect("read first counter").lines().count(), 1);
    assert_eq!(read_to_string(second_counter).expect("read second counter").lines().count(), 2);
    let resumed = read_dir(&reports)
        .expect("read reports")
        .map(|entry| entry.expect("report entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .filter_map(|path| xylem_lib::journal::RunJournal::read(path).ok())
        .find(|summary| {
            summary.inputs[0]
                .modules
                .first()
                .is_some_and(|module| module.state == xylem_lib::journal::ExecutionState::SkippedExisting)
        })
        .expect("resumed summary");
    assert_eq!(resumed.inputs[0].state, xylem_lib::journal::ExecutionState::Succeeded);
    assert_eq!(
        resumed.inputs[0].modules.iter().map(|module| module.state).collect::<Vec<_>>(),
        [
            xylem_lib::journal::ExecutionState::SkippedExisting,
            xylem_lib::journal::ExecutionState::Succeeded
        ]
    );
    remove_dir_all(test_root).expect("remove partial resume fixture");
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
#[test]
fn test_spawn_failure_reports_command_and_working_directory() {
    let test_root = std::env::temp_dir().join(format!("xylem-spawn-failure-{}", std::process::id()));
    let module = test_root.join("module");
    create_dir_all(&module).expect("create spawn failure module");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "modules": [{
            "name": "Spawn Failure",
            "programmingLanguage": "generic",
            "template": { "arguments": [], "command": "xylem-command-that-cannot-exist" },
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "requirements": []
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize spawn failure config")).expect("write spawn failure config");
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command
        .args(["run", "--config", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run spawn failure workflow");
    assert_eq!(output.status.code(), Some(exitcode::SOFTWARE));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("xylem-command-that-cannot-exist"), "{stderr}");
    assert!(stderr.contains(&module.display().to_string()), "{stderr}");
    remove_dir_all(test_root).expect("remove spawn failure directory");
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
