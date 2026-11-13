#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
//! End-to-end command-line workflow tests.
use acorn_core::time::UnitOfTime;
use acorn_schema::hardware::Memory;
use std::fs::copy;
#[cfg(unix)]
use std::fs::symlink_metadata;
use std::fs::{create_dir_all, read, read_dir, read_to_string, remove_dir_all, remove_file, write};
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const PROMPT_POLICY_REJECTION: &str = "Invalid prompt: your prompt was flagged as potentially violating our usage policy. Please try again with a different prompt: https://platform.openai.com/docs/guides/reasoning#advice-on-prompting";
#[cfg(not(target_os = "windows"))]
fn managed_keep_alive_config(module: &Path) -> serde_json::Value {
    serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "managed",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": format!("file:{}", module.display()),
            "template": {
                "command": "true",
                "arguments": [],
                "container": {
                    "cleanup": "always",
                    "create": true,
                    "image": "alpine:3.22",
                    "keep_alive": true,
                    "name": "worker",
                    "runtime": "docker",
                    "startup_timeout_seconds": 1
                }
            }
        }]
    })
}
fn native_lifecycle_config(modules: &Path, output: &Path, command: &str) -> serde_json::Value {
    serde_json::json!({
        "output": output.display().to_string(),
        "requirements": ["sh", "tar"],
        "modules": [{
            "name": "native-lifecycle",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": format!("file:{}", modules.display()),
            "template": { "arguments": ["-c", command], "command": "sh" }
        }],
        "sandbox": true
    })
}
fn native_report(path: &Path) -> xylem_lib::journal::RunSummary {
    let report = read_dir(path)
        .expect("read native report directory")
        .map(|entry| entry.expect("native report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("native JSON report");
    serde_json::from_slice(&read(report).expect("read native JSON report")).expect("valid native JSON report")
}
fn native_sandbox_policy(
    runtime: &str,
    workload: &str,
    artifacts: &Path,
    inputs: &Path,
    modules: &Path,
    retention: &str,
    stale_after_seconds: u64,
) -> serde_json::Value {
    serde_json::json!({
        "environment": {
            "allow": [
                "CONDA_OVERRIDE_CUDA",
                "ENV_NAME",
                "LANG",
                "LC_ALL",
                "MAMBA_EXE",
                "MAMBA_ROOT_PREFIX",
                "MAMBA_USER",
                "MAMBA_USER_GID",
                "MAMBA_USER_ID",
                "OUTPUT_DIRECTORY",
                "PATH",
                "PROJ_DATA",
                "PROJ_LIB",
                "PYTHONDONTWRITEBYTECODE",
                "_XYLEM_INPUT",
                "_XYLEM_MODULE_LIST",
                "_XYLEM_INPUT_BASENAME"
            ]
        },
        "lifecycle": { "retention": retention, "stale_after_seconds": stale_after_seconds },
        "minimum_isolation": "shared_kernel",
        "paths": { "artifact_roots": [artifacts], "input_roots": [inputs], "module_roots": [modules] },
        "preparation_network": "deny",
        "process_guards": [],
        "provider": { "runtime": runtime, "type": "oci" },
        "resources": {
            "artifact_bytes": 1048576,
            "artifact_files": 1000,
            "captured_output_bytes": 65536,
            "cpu_millis": 1000,
            "memory_bytes": 536870912,
            "pids": 128,
            "scratch_bytes": 134217728,
            "wall_time_seconds": 60
        },
        "version": 1,
        "workload": { "allowed": [workload], "default": workload }
    })
}
fn native_sandbox_resources(runtime: &str, policy_digest: &str) -> Vec<String> {
    let output = std::process::Command::new(runtime)
        .args([
            "ps",
            "--all",
            "--filter",
            "label=gov.ornl.xylem.sandbox=true",
            "--filter",
            &format!("label=gov.ornl.xylem.policy={policy_digest}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("list native sandbox resources");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut resources = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    resources.sort();
    resources
}
#[cfg(not(target_os = "windows"))]
fn write_fake_docker(root: &Path) -> (PathBuf, PathBuf) {
    let executable = root.join("docker");
    let log = root.join("docker.log");
    write(
        &executable,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$XYLEM_FAKE_DOCKER_LOG"
case "$1" in
  run)
    printf 'fake-container-id\n'
    ;;
  inspect)
    case "${XYLEM_FAKE_DOCKER_STATE:-ready}" in
      exited) printf '[{"State":{"Running":false}}]\n' ;;
      starting) printf '[{"State":{"Running":true,"Health":{"Status":"starting"}}}]\n' ;;
      *) printf '[{"State":{"Running":true}}]\n' ;;
    esac
    ;;
  exec|rm)
    ;;
  *)
    exit 64
    ;;
esac
"#,
    )
    .expect("write fake Docker runtime");
    let mut permissions = executable.metadata().expect("inspect fake Docker runtime").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions).expect("make fake Docker runtime executable");
    (executable, log)
}
#[cfg(not(target_os = "windows"))]
fn write_fake_nvidia_smi(root: &Path) -> PathBuf {
    let executable = root.join("nvidia-smi");
    write(&executable, "#!/bin/sh\nprintf '0, GPU-test, NVIDIA Test, 8192, 50, 1024\\n'\n").expect("write fake nvidia-smi");
    let mut permissions = executable.metadata().expect("inspect fake nvidia-smi").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions).expect("make fake nvidia-smi executable");
    executable
}
fn xylem_command() -> assert_cmd::Command {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command.env(
        xylem_lib::run::store::RUN_STORE_ENV,
        std::env::temp_dir().join(format!("xylem-cli-run-store-{}", std::process::id())),
    );
    command
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_cli_arguments_bind_and_preserve_usage_boundaries() {
    let root = std::env::temp_dir().join(format!("xylem-cli-arguments-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    create_dir_all(&root).expect("create arguments fixture");
    let result = root.join("result.txt");
    let config = root.join("workflow.json");
    write(
        &config,
        serde_json::json!({
            "arguments": ["DATASET", "MODE"],
            "requirements": [],
            "variables": {"DATASET": "default", "MODE": "workflow"},
            "modules": [{
                "name": "probe", "type": "script", "programmingLanguage": "generic",
                "uri": format!("file:{}", root.display()), "variables": {"MODE": "module"},
                "template": {"command": "sh", "arguments": [
                    "-c", "printf '%s|%s' \"$1\" \"$_XYLEM_MODE\" > \"$2\"",
                    "sh", "{{ DATASET }}", result.display().to_string()
                ]}
            }]
        })
        .to_string(),
    )
    .expect("write arguments config");
    let path = config.to_str().expect("config path");
    let plan = |arguments: &[&str]| {
        let output = xylem_command()
            .args(["run", path, "--dry-run", "--reporter", "json"])
            .args(arguments)
            .output()
            .expect("run plan");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("plan JSON")
    };
    let bound = plan(&["--var", "MODE=named", "--", "basin", "-literal"]);
    assert_eq!(bound["modules"][0]["variables"]["_XYLEM_DATASET"], "basin");
    assert_eq!(bound["modules"][0]["variables"]["_XYLEM_MODE"], "named");
    let omitted = plan(&["--", "basin"]);
    assert_eq!(omitted["modules"][0]["variables"]["_XYLEM_MODE"], "module");
    let empty = plan(&["--", ""]);
    assert_eq!(empty["modules"][0]["variables"]["_XYLEM_DATASET"], "");
    let hyphen = plan(&["--", "-literal"]);
    assert_eq!(hyphen["modules"][0]["variables"]["_XYLEM_DATASET"], "-literal");
    let output = xylem_command()
        .args(["run", path, "--var", "MODE=named", "--", "basin", "ignored"])
        .output()
        .expect("execute positional workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read_to_string(&result).expect("read positional result"), "basin|named");
    for arguments in [vec!["run", "--", "value"], vec!["run", path, "--", "one", "two", "three"]] {
        let output = xylem_command().args(arguments).output().expect("reject invocation");
        assert_eq!(output.status.code(), Some(64), "{}", String::from_utf8_lossy(&output.stderr));
    }
    let absent = root.join("absent.json");
    write(&absent, r#"{"requirements":[],"modules":[]}"#).expect("write absent declarations");
    let output = xylem_command()
        .args(["run", absent.to_str().expect("absent path"), "--", "value"])
        .output()
        .expect("reject missing declarations");
    assert_eq!(output.status.code(), Some(64));
    let invalid = root.join("invalid.json");
    write(&invalid, r#"{"arguments":["INPUT"],"requirements":[],"modules":[]}"#).expect("write invalid declarations");
    let output = xylem_command()
        .args(["run", invalid.to_str().expect("invalid path"), "--", "value"])
        .output()
        .expect("reject invalid declarations");
    assert_eq!(output.status.code(), Some(65));
    remove_dir_all(root).expect("remove arguments fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_cli_variable_override_matches_plan_and_execution() {
    let root = std::env::temp_dir().join(format!("xylem-cli-variable-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    create_dir_all(&root).expect("create fixture");
    let result = root.join("result.txt");
    let external_result = root.join("external.txt");
    write(
        root.join("template.json"),
        serde_json::json!({
            "command": "sh", "arguments": [
                "-c", "printf '%s|%s' \"$1\" \"$_XYLEM_DATASET\" > \"$2\"",
                "sh", "{{ DATASET }}", external_result.display().to_string()
            ]
        })
        .to_string(),
    )
    .expect("write external template");
    let config = root.join("workflow.json");
    write(
        &config,
        serde_json::json!({
            "requirements": [],
            "variables": {"DATASET": "root", "COMPOSED": "{{ DATASET }}:suffix"},
            "modules": [{
                "name": "probe", "type": "script", "programmingLanguage": "generic",
                "uri": format!("file:{}", root.display()), "variables": {"DATASET": "module"},
                "template": {"command": "sh", "arguments": [
                    "-c", "printf '%s|%s|%s' \"$1\" \"$_XYLEM_DATASET\" \"$_XYLEM_COMPOSED\" > \"$2\"",
                    "sh", "{{ DATASET }}", result.display().to_string()
                ]}
            }, {
                "name": "external", "type": "script", "programmingLanguage": "generic",
                "uri": format!("file:{}", root.display()), "template": "template.json"
            }]
        })
        .to_string(),
    )
    .expect("write fixture");
    let output = xylem_command()
        .args([
            "run",
            config.to_str().expect("config path"),
            "--dry-run",
            "--reporter",
            "json",
            "--var",
            "DATASET=first",
            "--var",
            "DATASET=final",
        ])
        .output()
        .expect("dry run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(plan["modules"][0]["variables"]["_XYLEM_DATASET"], "final");
    assert_eq!(plan["modules"][0]["variables"]["_XYLEM_COMPOSED"], "final:suffix");
    assert!(!result.exists());
    let output = xylem_command()
        .args(["run", config.to_str().expect("config path"), "--var", "DATASET=final"])
        .output()
        .expect("run workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read_to_string(&result).expect("read result"), "final|final|final:suffix");
    assert_eq!(read_to_string(&external_result).expect("read external result"), "final|final");
    let output = xylem_command()
        .args(["run", config.to_str().expect("config path"), "--var", "INPUT=forbidden"])
        .output()
        .expect("reject variable");
    assert_eq!(output.status.code(), Some(64));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("forbidden"));
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_cli_variable_override_reaches_nested_leaf_and_selection_roots() {
    let root = std::env::temp_dir().join(format!("xylem-cli-variable-nested-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let selected = root.join("selected");
    create_dir_all(&selected).expect("create selected input root");
    write(selected.join("item.tif"), "input").expect("write selected input");
    let child = root.join("child.json");
    write(
        &child,
        serde_json::json!({
            "arguments": ["CHILD"], "requirements": [], "variables": {"DATASET": "child"},
            "modules": [{"name": "leaf", "type": "script", "programmingLanguage": "generic",
                "uri": "file:.", "variables": {"DATASET": "leaf"},
                "template": {"command": "true", "arguments": ["{{ DATASET }}"]}}]
        })
        .to_string(),
    )
    .expect("write child");
    let parent = root.join("parent.json");
    write(
        &parent,
        serde_json::json!({
            "arguments": ["ROOT", "DATASET"],
            "requirements": [], "variables": {"ROOT": "missing", "DATASET": "parent"},
            "input": {"select": {"engine": "cel", "options": {"roots": ["{{ ROOT }}"], "recursive": false},
                "script": "items.filter(item, item.extension == '.tif').map(item, item.path)"}},
            "modules": [{"name": "group", "type": "configuration", "uri": "file:./child.json",
                "variables": {"DATASET": "wrapper"}}]
        })
        .to_string(),
    )
    .expect("write parent");
    let output = xylem_command()
        .args([
            "run",
            parent.to_str().expect("parent path"),
            "--dry-run",
            "--reporter",
            "json",
            "--var",
            "DATASET=invocation",
            "--",
            selected.to_str().expect("selection root"),
            "positional",
        ])
        .output()
        .expect("plan nested workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(plan["inputs"]["source"]["kind"], "selection");
    assert_eq!(plan["inputs"]["source"]["candidate_count"], 1);
    assert_eq!(plan["modules"][0]["variables"]["_XYLEM_DATASET"], "invocation");
    remove_dir_all(root).expect("remove nested fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_configuration_module_executes_and_reports_qualified_leaf_identity() {
    let test_root = std::env::temp_dir().join(format!("xylem-configuration-cli-{}", std::process::id()));
    let child_directory = test_root.join("child");
    let module_directory = child_directory.join("module");
    let report_directory = test_root.join("reports");
    create_dir_all(&module_directory).expect("create nested module directory");
    let root_config = test_root.join("config.json");
    let child_config = child_directory.join("config.json");
    write(
        &root_config,
        r#"{
            "requirements": [],
            "modules": [{ "name": "Group", "type": "configuration", "uri": "file:./child/config.json" }]
        }"#,
    )
    .expect("write root configuration");
    write(
        &child_config,
        r#"{
            "output": "child-output",
            "requirements": [],
            "modules": [{
                "name": "Leaf",
                "type": "script",
                "uri": "file:./module",
                "variables": { "OUTPUT_DIRECTORY": "leaf-output" },
                "template": { "command": "true", "arguments": [] }
            }]
        }"#,
    )
    .expect("write child configuration");
    let artifact_output = test_root.canonicalize().expect("canonical test root").join("artifacts");
    let mut dry_run = xylem_command();
    let dry_run = dry_run
        .current_dir(&test_root)
        .args([
            "run",
            root_config.to_str().expect("UTF-8 configuration path"),
            "--dry-run",
            "--reporter",
            "json",
            "--artifact-output",
            "artifacts",
        ])
        .output()
        .expect("run nested dry run");
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&dry_run.stdout).expect("valid nested execution plan");
    assert_eq!(plan["modules"][0]["module_path"], serde_json::json!(["Group", "Leaf"]));
    assert_eq!(
        plan["modules"][0]["variables"]["_XYLEM_OUTPUT_DIRECTORY"],
        artifact_output.to_string_lossy().as_ref()
    );
    let mut command = xylem_command();
    command
        .args([
            "run",
            root_config.to_str().expect("UTF-8 configuration path"),
            "--reporter",
            "json",
            "--output",
            report_directory.to_str().expect("UTF-8 report path"),
        ])
        .assert()
        .success();
    let report_path = read_dir(&report_directory)
        .expect("read report directory")
        .map(|entry| entry.expect("report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("JSON report");
    let report: xylem_lib::journal::RunSummary = serde_json::from_slice(&read(report_path).expect("read JSON report")).expect("valid JSON report");
    assert_eq!(report.inputs[0].modules[0].name, "Leaf");
    assert_eq!(report.inputs[0].modules[0].module_path, vec!["Group", "Leaf"]);
    remove_dir_all(test_root).expect("remove configuration CLI test directory");
}
#[test]
fn test_configuration_read_failures_use_distinct_exit_codes() {
    let test_root = std::env::temp_dir().join(format!("xylem-config-errors-{}", std::process::id()));
    create_dir_all(&test_root).expect("create configuration error directory");
    let invalid = test_root.join("invalid.yaml");
    let missing = test_root.join("missing.yaml");
    write(&invalid, "requirements: [\n").expect("write invalid YAML configuration");

    let mut invalid_command = xylem_command();
    invalid_command
        .args(["run", invalid.to_str().expect("UTF-8 invalid path")])
        .assert()
        .failure()
        .code(exitcode::DATAERR);

    let mut missing_command = xylem_command();
    missing_command
        .args(["run", missing.to_str().expect("UTF-8 missing path")])
        .assert()
        .failure()
        .code(exitcode::UNAVAILABLE);

    remove_file(invalid).expect("remove invalid YAML configuration");
    remove_dir_all(test_root).expect("remove configuration error directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_configured_batch_caps_active_inputs_in_the_compiled_cli() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
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
    let mut command = xylem_command();
    let output = command
        .current_dir(workspace)
        .args(["--threads", "10", "run", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run batched workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "batched workflow failed:\n{logs}");
    assert_eq!(read_to_string(state.join("peak")).expect("peak concurrency").trim(), "2");
    let mut completed = read_to_string(state.join("completed"))
        .expect("completed inputs")
        .lines()
        .map(|input| Path::new(input).file_name().expect("top-level input name").to_string_lossy().to_string())
        .collect::<Vec<_>>();
    completed.sort();
    assert_eq!(completed, ["four", "one", "three", "two"]);
    remove_dir_all(test_root).expect("remove batch fixture");
}
#[test]
fn test_docker_sandbox_example_has_valid_public_dry_run() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let example = workspace.join("examples/docker-sandbox-provider");
    let output = xylem_command()
        .current_dir(&example)
        .args([
            "--sandbox-policy",
            "sandbox-policy.yaml",
            "run",
            ".xylem.jsonc",
            "--dry-run",
            "--reporter",
            "json",
            "--var",
            "MODE=sandbox",
        ])
        .output()
        .expect("run Docker Sandbox example dry-run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Warning: Docker Sandbox support is experimental"));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid Docker Sandbox example plan");
    assert_eq!(plan["sandbox"]["provider"], "docker_sandbox");
    assert_eq!(plan["modules"][0]["variables"]["_XYLEM_MODE"], "sandbox");
    assert_eq!(plan["sandbox"]["runtime"], serde_json::Value::Null);
    assert_eq!(
        plan["sandbox"]["workload"],
        "docker.io/docker/sandbox-templates:shell@sha256:c66aa9c0212bb710089cc356501f8bde8989e02a7071bf1c6be0893be1c30adf"
    );
}
#[test]
#[ignore = "requires standalone sbx 0.45-0.47 and a running local Docker Sandbox daemon"]
fn test_docker_sandbox_example_native_end_to_end() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let source = workspace.join("examples/docker-sandbox-provider");
    let example = std::env::temp_dir().join(format!("xylem-docker-sandbox-example-{}", std::process::id()));
    create_dir_all(example.join("input")).expect("create Docker Sandbox input fixture");
    create_dir_all(example.join("module")).expect("create Docker Sandbox module fixture");
    copy(source.join("input/name.txt"), example.join("input/name.txt")).expect("copy Docker Sandbox input");
    copy(source.join("module/run.sh"), example.join("module/run.sh")).expect("copy Docker Sandbox module");
    copy(source.join("sandbox-policy.yaml"), example.join("sandbox-policy.yaml")).expect("copy Docker Sandbox policy");
    let config = read_to_string(source.join(".xylem.jsonc"))
        .expect("read Docker Sandbox workflow")
        .replace("\"offline\": true", "\"offline\": false");
    write(example.join(".xylem.jsonc"), config).expect("write Docker Sandbox workflow");
    let output = xylem_command()
        .current_dir(&example)
        .args(["--sandbox-policy", "sandbox-policy.yaml", "run", ".xylem.jsonc"])
        .output()
        .expect("run Docker Sandbox native example");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        read_to_string(example.join("output/greeting.txt")).expect("read greeting"),
        "Hello, Ada!\n"
    );
    remove_dir_all(example).expect("remove Docker Sandbox example fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_dry_run_plans_managed_container_without_starting_runtime() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
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
                    "keep_alive": true,
                    "mounts": [{"source": ".", "target": "/workspace"}]
                }
            }
        }]
    });
    write(&config, serde_json::to_vec_pretty(&document).expect("serialize fixture")).expect("write managed plan fixture");
    let mut command = xylem_command();
    let output = command
        .current_dir(workspace)
        .args([
            "run",
            config.to_str().expect("UTF-8 configuration path"),
            "--dry-run",
            "--reporter",
            "json",
            "--no-container-env",
            "--var",
            "MODE=container",
        ])
        .output()
        .expect("run managed dry-run plan");
    assert!(output.status.success(), "dry run failed: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid dry-run plan");
    assert_eq!(plan["containers"][0]["action"], "would_create");
    assert_eq!(plan["containers"][0]["name"], "worker");
    assert_eq!(plan["containers"][0]["mounts"][0]["source"], test_root.display().to_string());
    assert_eq!(plan["containers"][0]["startup"], "keep_alive");
    assert_eq!(plan["modules"][0]["variables"]["_XYLEM_MODE"], "container");
    remove_dir_all(test_root).expect("remove managed plan fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_dry_run_renders_a_plan_without_writing_files() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let output_directory = std::env::temp_dir().join(format!("xylem-write-free-dry-run-{}", std::process::id()));
    let _ = remove_dir_all(&output_directory);
    let mut command = xylem_command();
    let output = command
        .current_dir(workspace)
        .args([
            "run",
            "tests/e2e/config.json",
            "--artifact-output",
            output_directory.to_str().expect("UTF-8 artifact output"),
            "--dry-run",
            "--failure-policy",
            "continue",
            "--gpu-monitor",
            "--reporter",
            "json",
            "--threads",
            "2",
        ])
        .output()
        .expect("run dry-run plan");
    assert!(output.status.success(), "dry run failed: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid dry-run plan");
    assert_eq!(plan["schema_version"], 15);
    assert_eq!(plan["failure_policy"], "continue");
    assert_eq!(plan["admission"]["effective_ceiling"], 2);
    assert_eq!(plan["admission"]["input_indices"], serde_json::json!([0]));
    assert_eq!(plan["contexts"][0]["workspace"], "input_local");
    assert!(plan["memory"].is_null());
    assert_eq!(plan["sandbox"]["mode"], "off");
    assert_eq!(plan["gpu_monitoring"]["interval"], 1000);
    assert_eq!(plan["gpu_monitoring"]["source"], "nvidia-smi");
    assert_eq!(plan["configuration"]["output"], output_directory.display().to_string());
    assert_eq!(plan["modules"][0]["decision"], "would_run");
    assert_eq!(
        plan["modules"][0]["variables"]["_XYLEM_OUTPUT_DIRECTORY"],
        output_directory.display().to_string()
    );
    assert!(!output_directory.exists());
}
#[test]
fn test_dry_run_resolves_cel_selection_relative_to_configuration() {
    let test_root = std::env::temp_dir().join(format!("xylem-cel-dry-run-{}", std::process::id()));
    let empty_path = test_root.join("empty-path");
    let inputs = test_root.join("inputs");
    let output_directory = test_root.join("artifacts");
    create_dir_all(&empty_path).expect("create empty PATH");
    create_dir_all(&inputs).expect("create selection inputs");
    write(inputs.join("b.txt"), "b").expect("write b");
    write(inputs.join("a.tif"), "a").expect("write a");
    write(test_root.join("outside.tif"), "outside").expect("write outside candidate");
    let config_path = test_root.join("config.yaml");
    write(
        &config_path,
        r#"requirements: []
modules: []
output: artifacts
variables:
  BASE: inputs
  SCENES: "{{ BASE }}"
input:
  select:
    engine: cel
    options:
      roots: ["{{ SCENES }}"]
      recursive: false
    script: |-
      items
        .filter(
          item,
          item.extension == '.tif'
        )
        .map(item, item.path)
"#,
    )
    .expect("write selection configuration");
    let run = || {
        let mut command = xylem_command();
        command
            .env("PATH", &empty_path)
            .args(["run", config_path.to_str().expect("UTF-8 config path"), "--dry-run", "--reporter", "json"])
            .output()
            .expect("run selection dry run")
    };
    let output = run();
    assert!(
        output.status.success(),
        "selection dry run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let repeated = run();
    assert!(repeated.status.success(), "repeated selection dry run failed");
    assert_eq!(repeated.stdout, output.stdout);
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid selection plan");
    let inputs = std::fs::canonicalize(inputs).expect("canonical selection inputs");
    assert_eq!(plan["schema_version"], 15);
    assert!(plan["memory"].is_null());
    assert_eq!(plan["sandbox"]["mode"], "off");
    assert_eq!(plan["inputs"]["source"]["kind"], "selection");
    assert_eq!(plan["inputs"]["source"]["engine"], "cel");
    assert_eq!(plan["inputs"]["source"]["candidate_count"], 2);
    assert_eq!(plan["inputs"]["values"][0], inputs.join("a.tif").display().to_string());
    assert!(plan["inputs"]["source"]["script_digest"]
        .as_str()
        .is_some_and(|digest| digest.starts_with("sha256:")));
    assert!(!output_directory.exists());
    let mut entries = std::fs::read_dir(&test_root)
        .expect("read dry-run fixture")
        .map(|entry| entry.expect("read fixture entry").file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries, ["config.yaml", "empty-path", "inputs", "outside.tif"]);
    remove_dir_all(test_root).expect("remove selection fixture");
}
#[test]
fn test_explicit_cli_input_bypasses_cel_discovery_and_evaluation() {
    let test_root = std::env::temp_dir().join(format!("xylem-cel-cli-override-{}", std::process::id()));
    create_dir_all(&test_root).expect("create CLI override fixture");
    let config_path = test_root.join("config.yaml");
    write(
        &config_path,
        r#"requirements: []
modules: []
input:
  select:
    engine: cel
    options:
      roots: [missing]
    script: "["
"#,
    )
    .expect("write invalid unused selection");
    let mut command = xylem_command();
    let output = command
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--input",
            "explicit",
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("run CLI override dry run");
    assert!(
        output.status.success(),
        "CLI override failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid override plan");
    assert_eq!(plan["inputs"]["source"]["kind"], "cli");
    assert_eq!(plan["inputs"]["values"], serde_json::json!(["explicit"]));
    remove_dir_all(test_root).expect("remove CLI override fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_fail_fast_journal_distinguishes_failed_and_not_started_inputs() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
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
    let mut command = xylem_command();
    command
        .current_dir(workspace)
        .args([
            "run",
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
    assert_eq!(report.failure_policy, xylem_lib::config::FailurePolicy::FailFast);
    assert_eq!(report.stop_reason, Some(xylem_lib::journal::RunStopReason::Failure { input_index: 0 }));
    assert_eq!(report.inputs[0].state, xylem_lib::journal::ExecutionState::Failed);
    assert_eq!(report.inputs[1].state, xylem_lib::journal::ExecutionState::NotStarted);
    assert_eq!(report.inputs[2].state, xylem_lib::journal::ExecutionState::NotStarted);
    remove_dir_all(test_root).expect("remove durable failure directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_fail_fast_preserves_failure_and_cancels_active_sibling() {
    let test_root = std::env::temp_dir().join(format!("xylem-fail-fast-cancellation-{}", std::process::id()));
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    let state = test_root.join("state");
    create_dir_all(&module).expect("create fail-fast module");
    create_dir_all(&reports).expect("create fail-fast reports");
    create_dir_all(&state).expect("create fail-fast state");
    write(
        module.join("run.sh"),
        r#"#!/bin/sh
set -eu
state="$1"
case "$_XYLEM_INPUT" in
  slow)
    : > "$state/slow-started"
    while [ ! -f "$state/failure-started" ]; do sleep 0.01; done
    sleep 30
    ;;
  failure)
    while [ ! -f "$state/slow-started" ]; do sleep 0.01; done
    : > "$state/failure-started"
    exit 7
    ;;
  *)
    : > "$state/unexpected-later-input"
    ;;
esac
"#,
    )
    .expect("write fail-fast probe");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "input": ["slow", "failure", "later"],
        "modules": [{
            "name": "Fail Fast Probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["run.sh", state], "command": "sh" },
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "options": { "batch": 2 },
        "requirements": ["sh"]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize fail-fast config")).expect("write fail-fast config");
    let output = xylem_command()
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
            "--reporter",
            "json",
        ])
        .output()
        .expect("run fail-fast cancellation workflow");
    assert_eq!(
        output.status.code(),
        Some(exitcode::SOFTWARE),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = native_report(&reports);
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Failed);
    assert_eq!(report.failure_policy, xylem_lib::config::FailurePolicy::FailFast);
    assert_eq!(report.stop_reason, Some(xylem_lib::journal::RunStopReason::Failure { input_index: 1 }));
    assert_eq!(
        report.inputs.iter().map(|input| input.state).collect::<Vec<_>>(),
        [
            xylem_lib::journal::ExecutionState::Cancelled,
            xylem_lib::journal::ExecutionState::Failed,
            xylem_lib::journal::ExecutionState::NotStarted,
        ]
    );
    assert!(!state.join("unexpected-later-input").exists());
    remove_dir_all(test_root).expect("remove fail-fast cancellation directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_failure_policy_continue_admits_later_inputs() {
    let test_root = std::env::temp_dir().join(format!("xylem-continue-policy-{}", std::process::id()));
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    let state = test_root.join("state");
    create_dir_all(&module).expect("create continue module");
    create_dir_all(&reports).expect("create continue reports");
    create_dir_all(&state).expect("create continue state");
    write(
        module.join("run.sh"),
        r#"#!/bin/sh
set -eu
case "$_XYLEM_INPUT" in
  failure) exit 7 ;;
  later) : > "$1/later-ran" ;;
esac
"#,
    )
    .expect("write continue probe");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "input": ["failure", "later"],
        "modules": [{
            "name": "Continue Probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["run.sh", state], "command": "sh" },
            "timeout_seconds": 30,
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "options": { "batch": 1 },
        "requirements": ["sh"]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize continue config")).expect("write continue config");
    let output = xylem_command()
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--failure-policy",
            "continue",
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
            "--reporter",
            "json",
        ])
        .output()
        .expect("run continue workflow");
    assert_eq!(
        output.status.code(),
        Some(exitcode::SOFTWARE),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = native_report(&reports);
    assert_eq!(
        report.inputs.iter().map(|input| input.state).collect::<Vec<_>>(),
        [xylem_lib::journal::ExecutionState::Failed, xylem_lib::journal::ExecutionState::Succeeded]
    );
    assert_eq!(report.failure_policy, xylem_lib::config::FailurePolicy::Continue);
    assert_eq!(report.workspace, xylem_lib::journal::WorkspaceIdentity::RunLocal);
    assert_eq!(report.contexts.len(), 2);
    assert_eq!(report.contexts[0].input_id, "input-000000");
    assert_eq!(report.contexts[1].input_id, "input-000001");
    assert_eq!(report.inputs[0].modules[0].timeout, Some(acorn_core::time::Seconds::new(30)));
    assert_eq!(report.stop_reason, None);
    assert!(state.join("later-ran").exists());
    remove_dir_all(test_root).expect("remove continue policy directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_gpu_monitoring_fails_before_execution_when_nvidia_smi_is_missing() {
    let test_root = std::env::temp_dir().join(format!("xylem-gpu-missing-{}", std::process::id()));
    let empty_path = test_root.join("empty-path");
    let reports = test_root.join("reports");
    create_dir_all(&empty_path).expect("create empty PATH");
    let config_path = test_root.join("config.json");
    write(&config_path, r#"{"requirements":[],"modules":[]}"#).expect("write configuration");
    let mut command = xylem_command();
    let output = command
        .env("PATH", &empty_path)
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--gpu-monitor",
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
        ])
        .output()
        .expect("run unavailable GPU monitoring workflow");
    assert_eq!(output.status.code(), Some(exitcode::UNAVAILABLE));
    assert!(String::from_utf8_lossy(&output.stderr).contains("GPU monitoring requires nvidia-smi"));
    let journal = read_dir(&reports)
        .expect("read report directory")
        .map(|entry| entry.expect("report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .expect("GPU monitoring journal");
    let summary = xylem_lib::journal::RunJournal::read(journal).expect("recover unavailable monitoring journal");
    assert_eq!(
        summary.gpu_monitoring.map(|monitoring| monitoring.status),
        Some(xylem_lib::journal::GpuMonitoringStatus::Unavailable)
    );
    remove_dir_all(test_root).expect("remove unavailable monitoring fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_gpu_monitoring_journals_samples_and_reports_aggregates() {
    let test_root = std::env::temp_dir().join(format!("xylem-gpu-success-{}", std::process::id()));
    let reports = test_root.join("reports");
    create_dir_all(&test_root).expect("create GPU monitoring root");
    write_fake_nvidia_smi(&test_root);
    let config_path = test_root.join("config.json");
    write(&config_path, r#"{"requirements":[],"modules":[],"options":{"gpu_monitor":true}}"#).expect("write configuration");
    let path = std::env::join_paths(core::iter::once(test_root.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())))
        .expect("construct fake nvidia-smi PATH");
    let mut command = xylem_command();
    command
        .env("PATH", path)
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--reporter",
            "json",
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
        ])
        .assert()
        .success();
    let files = read_dir(&reports)
        .expect("read report directory")
        .map(|entry| entry.expect("report entry").path())
        .collect::<Vec<_>>();
    let report = files
        .iter()
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("GPU monitoring report");
    let journal = files
        .iter()
        .find(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .expect("GPU monitoring journal");
    let summary: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(report).expect("read GPU monitoring report")).expect("valid GPU monitoring report");
    let monitoring = summary.gpu_monitoring.expect("GPU monitoring summary");
    assert_eq!(monitoring.status, xylem_lib::journal::GpuMonitoringStatus::Complete);
    assert!(monitoring.devices[0].samples >= 2);
    assert_eq!(monitoring.devices[0].utilization_percent.as_ref().map(|metric| metric.average), Some(50));
    assert!(read_to_string(journal).expect("read GPU journal").contains("\"event\":\"gpu_sample\""));
    remove_dir_all(test_root).expect("remove GPU monitoring fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_json_stdout_is_not_corrupted_by_diagnostics() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let mut command = xylem_command();
    let output = command
        .current_dir(workspace)
        .args(["run", "tests/e2e/config.json", "--reporter", "json"])
        .output()
        .expect("run JSON stdout workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: xylem_lib::journal::RunSummary = serde_json::from_slice(&output.stdout).expect("stdout should contain only the JSON report");
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Succeeded);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let journal_path = stderr
        .lines()
        .find_map(|line| line.strip_prefix("Journal: "))
        .map(PathBuf::from)
        .expect("stderr should identify the durable journal");
    let run_directory = journal_path.parent().expect("journal should have a parent");
    assert!(journal_path.is_file());
    remove_dir_all(run_directory).expect("remove default run directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_keep_alive_runtime_reports_early_exit_and_timeout() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-keep-alive-failures-{}", std::process::id()));
    let _ = remove_dir_all(&test_root);
    create_dir_all(&test_root).expect("create keep-alive failure fixture");
    let config = test_root.join("config.json");
    write(
        &config,
        serde_json::to_vec_pretty(&managed_keep_alive_config(&test_root)).expect("serialize keep-alive failure fixture"),
    )
    .expect("write keep-alive failure fixture");
    let (_, log) = write_fake_docker(&test_root);
    let path = std::env::join_paths(core::iter::once(test_root.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())))
        .expect("construct fake runtime PATH");
    let mut exited = xylem_command();
    let exited = exited
        .current_dir(workspace)
        .env("PATH", &path)
        .env("XYLEM_FAKE_DOCKER_LOG", &log)
        .env("XYLEM_FAKE_DOCKER_STATE", "exited")
        .args(["--no-progress", "run", config.to_str().expect("UTF-8 configuration path"), "--offline"])
        .output()
        .expect("run early-exit keep-alive workflow");
    assert!(!exited.status.success());
    let diagnostics = String::from_utf8_lossy(&exited.stderr);
    assert!(diagnostics.contains("startup strategy keep_alive"), "{diagnostics}");
    assert!(diagnostics.contains("sleep with infinity support"), "{diagnostics}");
    assert!(read_to_string(&log)
        .expect("read early-exit runtime log")
        .contains("rm --force fake-container-id"));
    write(&log, "").expect("reset fake runtime log");
    let mut starting = xylem_command();
    let starting = starting
        .current_dir(workspace)
        .env("PATH", &path)
        .env("XYLEM_FAKE_DOCKER_LOG", &log)
        .env("XYLEM_FAKE_DOCKER_STATE", "starting")
        .args(["--no-progress", "run", config.to_str().expect("UTF-8 configuration path"), "--offline"])
        .output()
        .expect("run readiness-timeout keep-alive workflow");
    assert!(!starting.status.success());
    let diagnostics = String::from_utf8_lossy(&starting.stderr);
    assert!(diagnostics.contains("did not become healthy within 1s"), "{diagnostics}");
    assert!(read_to_string(&log)
        .expect("read timeout runtime log")
        .contains("rm --force fake-container-id"));
    remove_dir_all(test_root).expect("remove keep-alive failure fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_keep_alive_runtime_uses_exact_arguments_and_cleanup_override() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!("xylem-keep-alive-success-{}", std::process::id()));
    let _ = remove_dir_all(&test_root);
    create_dir_all(&test_root).expect("create keep-alive success fixture");
    let config = test_root.join("config.json");
    write(
        &config,
        serde_json::to_vec_pretty(&managed_keep_alive_config(&test_root)).expect("serialize keep-alive success fixture"),
    )
    .expect("write keep-alive success fixture");
    let (_, log) = write_fake_docker(&test_root);
    let path = std::env::join_paths(core::iter::once(test_root.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())))
        .expect("construct fake runtime PATH");
    let mut success = xylem_command();
    let success = success
        .current_dir(workspace)
        .env("PATH", &path)
        .env("XYLEM_FAKE_DOCKER_LOG", &log)
        .args(["--no-progress", "run", config.to_str().expect("UTF-8 configuration path"), "--offline"])
        .output()
        .expect("run successful keep-alive workflow");
    assert!(success.status.success(), "{}", String::from_utf8_lossy(&success.stderr));
    let invocations = read_to_string(&log).expect("read successful runtime log");
    assert!(
        invocations.lines().any(|line| line.ends_with("--pull=never alpine:3.22 sleep infinity")),
        "{invocations}"
    );
    assert!(invocations.lines().any(|line| line.starts_with("exec ")), "{invocations}");
    assert!(invocations.contains("rm --force fake-container-id"), "{invocations}");
    write(&log, "").expect("reset fake runtime log");
    let mut retained = xylem_command();
    let retained = retained
        .current_dir(workspace)
        .env("PATH", &path)
        .env("XYLEM_FAKE_DOCKER_LOG", &log)
        .args([
            "--no-progress",
            "run",
            config.to_str().expect("UTF-8 configuration path"),
            "--container-cleanup",
            "never",
            "--offline",
        ])
        .output()
        .expect("run retained keep-alive workflow");
    assert!(retained.status.success(), "{}", String::from_utf8_lossy(&retained.stderr));
    let invocations = read_to_string(&log).expect("read retained runtime log");
    assert!(!invocations.contains("rm --force"), "{invocations}");
    remove_dir_all(test_root).expect("remove keep-alive success fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_loads_an_explicit_dotenv_without_overriding_process_values() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let env_path = std::env::temp_dir().join(format!("xylem-dotenv-{}.env", std::process::id()));
    write(&env_path, "XYLEM_DOTENV_TEST=from-file\n").expect("write dotenv fixture");
    let mut from_file = xylem_command();
    let output = from_file
        .current_dir(workspace)
        .args([
            "--env",
            env_path.to_str().expect("UTF-8 path"),
            "-vvvv",
            "run",
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
    let mut from_process = xylem_command();
    let output = from_process
        .current_dir(workspace)
        .env("XYLEM_DOTENV_TEST", "from-process")
        .args([
            "run",
            "--env",
            env_path.to_str().expect("UTF-8 path"),
            "-vvvv",
            "tests/e2e/dotenv-config.json",
        ])
        .output()
        .expect("run dotenv precedence workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "dotenv precedence workflow failed:\n{logs}");
    assert!(logs.contains("XYLEM_DOTENV_TEST=from-process"), "{logs}");
    let _ = remove_file(env_path);
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_memory_aware_example_plans_reservations_and_fallback() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let example = workspace.join("examples/memory-aware");
    let main = xylem_command()
        .current_dir(&example)
        .args(["run", "--offline", "--dry-run", "--reporter", "json"])
        .output()
        .expect("render memory-aware example plan");
    assert!(main.status.success(), "{}", String::from_utf8_lossy(&main.stderr));
    let main: serde_json::Value = serde_json::from_slice(&main.stdout).expect("valid memory-aware example plan");
    assert_eq!(main["memory"]["budget"], "6KB");
    assert_eq!(main["memory"]["estimator"]["identity"], "memory-aware-example-v1");
    assert_eq!(main["memory"]["mode"], "memory_aware");
    assert_eq!(main["memory"]["inputs"][0]["input_bytes"], 1024);
    assert_eq!(main["memory"]["inputs"][0]["reservation"], "2.5KB");
    assert_eq!(main["memory"]["inputs"][1]["input_bytes"], 2048);
    assert_eq!(main["memory"]["inputs"][1]["reservation"], "3.75KB");
    let fallback = xylem_command()
        .current_dir(example)
        .args(["run", ".xylem-fallback.json", "--offline", "--dry-run", "--reporter", "json"])
        .output()
        .expect("render memory-aware fallback plan");
    assert!(fallback.status.success(), "{}", String::from_utf8_lossy(&fallback.stderr));
    let fallback: serde_json::Value = serde_json::from_slice(&fallback.stdout).expect("valid memory-aware fallback plan");
    assert_eq!(fallback["memory"]["mode"], "fixed_fallback");
    assert_eq!(fallback["memory"]["inputs"][0]["input_bytes"], 3072);
    assert_eq!(fallback["memory"]["inputs"][0]["reservation"], serde_json::Value::Null);
    assert_eq!(fallback["memory"]["inputs"][0]["fallback"]["kind"], "input_out_of_range");
    assert_eq!(fallback["memory"]["inputs"][0]["fallback"]["maximum_input_bytes"], 2048);
    assert_eq!(fallback["memory"]["inputs"][0]["fallback"]["minimum_input_bytes"], 1024);
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_memory_aware_policy_rejection_is_terminal_and_not_retried() {
    let test_root = std::env::temp_dir().join(format!("xylem-memory-policy-rejection-{}", std::process::id()));
    let attempts = test_root.join("attempts.log");
    let input_one = test_root.join("one.bin");
    let input_two = test_root.join("two.bin");
    let reports = test_root.join("reports");
    create_dir_all(&reports).expect("create policy rejection fixture");
    write(&input_one, vec![0_u8; 1024]).expect("write first policy rejection input");
    write(&input_two, vec![0_u8; 1024]).expect("write second policy rejection input");
    let diagnostic = format!("{} {}", PROMPT_POLICY_REJECTION, "x".repeat(xylem_lib::journal::MAX_ERROR_BYTES * 2));
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "input": [input_one, input_two],
        "requirements": ["sh"],
        "options": {
            "batch": 1,
            "memory": {
                "budget": "1KB",
                "estimator": {
                    "identity": "policy-rejection-test-v1",
                    "intercept": "1KB",
                    "maximum_input": "1KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 0
                }
            }
        },
        "modules": [{
            "name": "Policy Rejection",
            "programmingLanguage": "generic",
            "template": {
                "arguments": [
                    "-c",
                    "printf 'attempt\\n' >> \"$0\"; printf '%s\\n' \"$1\" >&2; exit 7",
                    attempts,
                    diagnostic
                ],
                "command": "sh"
            },
            "type": "script",
            "uri": format!("file:{}", test_root.display())
        }]
    });
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize policy rejection config"),
    )
    .expect("write policy rejection config");
    let output = xylem_command()
        .args([
            "run",
            config_path.to_str().expect("UTF-8 policy rejection config"),
            "--no-progress",
            "--report-output",
            reports.to_str().expect("UTF-8 policy rejection reports"),
            "--reporter",
            "json",
        ])
        .output()
        .expect("run policy rejection workflow");
    assert_eq!(output.status.code(), Some(exitcode::SOFTWARE));
    assert_eq!(read_to_string(&attempts).expect("read policy rejection attempts"), "attempt\n");
    let report_path = read_dir(&reports)
        .expect("read policy rejection reports")
        .map(|entry| entry.expect("policy rejection report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("policy rejection JSON report");
    let report: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(report_path).expect("read policy rejection report")).expect("valid policy rejection report");
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Failed);
    assert_eq!(report.inputs[0].state, xylem_lib::journal::ExecutionState::Failed);
    assert_eq!(report.inputs[1].state, xylem_lib::journal::ExecutionState::NotStarted);
    let error = report.inputs[0].error.as_deref().expect("policy rejection diagnostic");
    assert!(error.contains(PROMPT_POLICY_REJECTION), "{error}");
    assert!(error.len() <= xylem_lib::journal::MAX_ERROR_BYTES);
    assert!(report.memory.expect("policy rejection memory evidence").inputs[0].observed_peak.is_none());
    remove_dir_all(test_root).expect("remove policy rejection fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_memory_intent_plans_and_runs_with_a_fixed_batching_warning() {
    let test_root = std::env::temp_dir().join(format!("xylem-memory-intent-{}", std::process::id()));
    create_dir_all(&test_root).expect("create memory intent fixture");
    let modules = serde_json::json!([
        {
            "name": "First Module",
            "programmingLanguage": "generic",
            "template": { "arguments": ["--version"], "command": "rustc" },
            "type": "script",
            "uri": "file:."
        },
        {
            "name": "Second Module",
            "programmingLanguage": "generic",
            "template": { "arguments": [], "command": "true" },
            "type": "script",
            "uri": "file:."
        }
    ]);
    let memory_path = test_root.join("memory.json");
    write(
        &memory_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "requirements": [],
            "modules": modules,
            "options": {
                "memory": {
                    "budget": "64GB",
                    "estimator": {
                        "identity": "custom-modules-v1",
                        "intercept": "16GB",
                        "maximum_input": "1TB",
                        "minimum_input": "1KB",
                        "safety_millionths": 1_100_000,
                        "slope_millionths": 0
                    }
                }
            }
        }))
        .expect("serialize memory intent fixture"),
    )
    .expect("write memory intent fixture");
    let dry_run = xylem_command()
        .args([
            "run",
            memory_path.to_str().expect("UTF-8 memory configuration"),
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("render memory intent plan");
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&dry_run.stdout).expect("valid memory intent plan");
    assert_eq!(plan["schema_version"], 15);
    assert_eq!(plan["memory"]["budget"], "64GB");
    assert_eq!(plan["memory"]["estimator"]["identity"], "custom-modules-v1");
    assert_eq!(plan["memory"]["inputs"][0]["fallback"]["kind"], "no_input");
    assert_eq!(plan["memory"]["estimator"]["slope_millionths"], 0);
    assert_eq!(plan["memory"]["mode"], "fixed_fallback");
    assert!(!String::from_utf8_lossy(&dry_run.stderr).contains("fell back to fixed input batching"));
    let run = xylem_command()
        .args(["--no-progress", "run", memory_path.to_str().expect("UTF-8 memory configuration")])
        .output()
        .expect("run memory intent fixture");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(
        stderr
            .matches(
                "Memory-aware admission for estimator custom-modules-v1 fell back to fixed input batching: the workflow has no concrete input to size"
            )
            .count(),
        1
    );
    let fixed_path = test_root.join("fixed.json");
    write(
        &fixed_path,
        serde_json::to_vec_pretty(&serde_json::json!({"requirements": [], "modules": modules})).expect("serialize fixed batching fixture"),
    )
    .expect("write fixed batching fixture");
    let fixed = xylem_command()
        .args(["--no-progress", "run", fixed_path.to_str().expect("UTF-8 fixed configuration")])
        .output()
        .expect("run fixed batching fixture");
    assert!(fixed.status.success(), "{}", String::from_utf8_lossy(&fixed.stderr));
    assert!(!String::from_utf8_lossy(&fixed.stderr).contains("fell back to fixed input batching"));
    remove_dir_all(test_root).expect("remove memory intent fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_memory_runtime_admission_caps_the_compiled_cli() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!(
        "xylem-memory-admission-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos()
    ));
    let inputs = [("large-one", 3 * 1024), ("small", 2 * 1024), ("large-two", 3 * 1024)]
        .into_iter()
        .map(|(name, bytes)| {
            let input = test_root.join("inputs").join(name);
            create_dir_all(&input).expect("create memory-aware input");
            write(input.join("payload.bin"), vec![0_u8; bytes]).expect("write memory-aware input");
            input
        })
        .collect::<Vec<_>>();
    let state = test_root.join("state");
    create_dir_all(&state).expect("create memory-aware state");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "version": "1.0.0",
        "requirements": ["sh"],
        "input": inputs.iter().map(|input| input.display().to_string()).collect::<Vec<_>>(),
        "options": {
            "batch": 3,
            "memory": {
                "budget": "5KB",
                "estimator": {
                    "identity": "compiled-cli-v1",
                    "intercept": "0KB",
                    "maximum_input": "4KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 1_000_000
                }
            }
        },
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
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize memory-aware fixture")).expect("write memory-aware fixture");
    let dry_run = xylem_command()
        .current_dir(workspace)
        .args([
            "run",
            config_path.to_str().expect("UTF-8 memory-aware config"),
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("render memory-aware plan");
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&dry_run.stdout).expect("valid memory-aware plan");
    assert_eq!(plan["memory"]["mode"], "memory_aware");
    assert_eq!(plan["memory"]["inputs"][0]["input_bytes"], 3 * 1024);
    assert_eq!(plan["memory"]["inputs"][1]["reservation"], "2KB");
    let report_output = test_root.join("reports");
    let run = xylem_command()
        .current_dir(workspace)
        .args([
            "--threads",
            "10",
            "--no-progress",
            "run",
            config_path.to_str().expect("UTF-8 memory-aware config"),
            "--report-output",
            report_output.to_str().expect("UTF-8 report output"),
            "--reporter",
            "json",
        ])
        .output()
        .expect("run memory-aware workflow");
    let logs = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    assert!(run.status.success(), "memory-aware workflow failed:\n{logs}");
    assert!(!logs.contains("fell back to fixed input batching"));
    assert_eq!(read_to_string(state.join("peak")).expect("peak memory-aware concurrency").trim(), "2");
    let report_path = read_dir(&report_output)
        .expect("read memory report output")
        .map(|entry| entry.expect("memory report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("memory JSON report");
    let report: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(&report_path).expect("read memory JSON report")).expect("valid memory JSON report");
    let memory = report.memory.expect("durable memory evidence");
    assert_eq!(memory.estimator_identity, "compiled-cli-v1");
    assert_eq!(memory.budget.checked_bytes(), Some(5 * 1024));
    assert_eq!(memory.inputs[0].estimated.as_ref().and_then(Memory::checked_bytes), Some(3 * 1024));
    assert_eq!(memory.inputs[0].safety_reserve.as_ref().and_then(Memory::checked_bytes), Some(0));
    assert_eq!(memory.inputs[0].reservation.as_ref().and_then(Memory::checked_bytes), Some(3 * 1024));
    remove_dir_all(test_root).expect("remove memory-aware fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_memory_runtime_rejects_oversized_reservations_before_work_begins() {
    let test_root = std::env::temp_dir().join(format!("xylem-memory-oversized-{}", std::process::id()));
    create_dir_all(&test_root).expect("create oversized memory fixture");
    let input = test_root.join("input.bin");
    let marker = test_root.join("module-ran");
    write(&input, vec![0_u8; 2 * 1024]).expect("write oversized memory input");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "version": "1.0.0",
        "requirements": ["sh"],
        "input": [input.display().to_string()],
        "options": {
            "memory": {
                "budget": "2KB",
                "estimator": {
                    "identity": "oversized-cli-v1",
                    "intercept": "1KB",
                    "maximum_input": "3KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 1_000_000
                }
            }
        },
        "modules": [{
            "name": "Must Not Run",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": "file:.",
            "template": {
                "command": "sh",
                "arguments": ["-c", format!("touch {}", marker.display())]
            }
        }]
    });
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize oversized memory fixture"),
    )
    .expect("write oversized memory fixture");
    let run = xylem_command()
        .args(["--no-progress", "run", config_path.to_str().expect("UTF-8 oversized memory config")])
        .output()
        .expect("run oversized memory workflow");
    assert_eq!(run.status.code(), Some(exitcode::DATAERR));
    assert!(String::from_utf8_lossy(&run.stderr).contains("input 0 requires 3072 memory bytes, exceeding the 2048-byte budget"));
    assert!(!marker.exists(), "module work must not begin after an oversized reservation");
    remove_dir_all(test_root).expect("remove oversized memory fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_module_timeout_reaches_managed_execution() {
    let test_root = std::env::temp_dir().join(format!("xylem-module-timeout-{}", std::process::id()));
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    create_dir_all(&module).expect("create timeout module");
    create_dir_all(&reports).expect("create timeout reports");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "modules": [{
            "name": "Timeout Probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["-c", "sleep 5"], "command": "sh" },
            "timeout_seconds": 1,
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "requirements": ["sh"]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize timeout config")).expect("write timeout config");
    let started = std::time::Instant::now();
    let output = xylem_command()
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
            "--reporter",
            "json",
        ])
        .output()
        .expect("run module timeout workflow");
    assert_eq!(
        output.status.code(),
        Some(exitcode::SOFTWARE),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        started.elapsed() < core::time::Duration::from_secs(4),
        "configured timeout did not bound execution"
    );
    let report = native_report(&reports);
    assert_eq!(report.inputs[0].modules[0].timeout, Some(acorn_core::time::Seconds::new(1)));
    assert!(
        report.inputs[0].modules[0]
            .error
            .as_deref()
            .is_some_and(|error| error.to_ascii_lowercase().contains("timed out")),
        "{:?}",
        report.inputs[0].modules[0].error
    );
    remove_dir_all(test_root).expect("remove timeout directory");
}
#[test]
#[ignore = "requires a native rootless OCI runtime and digest-pinned sandbox workload"]
fn test_native_rootless_sandbox_conformance() {
    let runtime = std::env::var("XYLEM_SANDBOX_RUNTIME").expect("XYLEM_SANDBOX_RUNTIME must select docker or podman");
    let workload = std::env::var("XYLEM_SANDBOX_WORKLOAD").expect("XYLEM_SANDBOX_WORKLOAD must be digest-pinned");
    let test_root = std::env::temp_dir().join(format!("xylem-native-sandbox-{}", std::process::id()));
    let artifacts = test_root.join("artifacts");
    let published = artifacts.join("published");
    let inputs = test_root.join("inputs");
    let modules = test_root.join("modules");
    let reports = test_root.join("reports");
    create_dir_all(&artifacts).expect("create sandbox artifact root");
    create_dir_all(&inputs).expect("create sandbox input root");
    create_dir_all(&modules).expect("create sandbox module root");
    let input = inputs.join("sample.txt");
    write(&input, vec![0_u8; 1024]).expect("write sandbox input");
    let config = test_root.join("config.json");
    write(
        &config,
        serde_json::to_vec_pretty(&serde_json::json!({
            "input": [input],
            "output": published.display().to_string(),
            "requirements": ["curl", "python", "sh", "tar"],
            "options": {
                "memory": {
                    "budget": "1GB",
                    "estimator": {
                        "identity": "native-sandbox-conformance-v1",
                        "intercept": "128MB",
                        "maximum_input": "1MB",
                        "minimum_input": "1KB",
                        "safety_millionths": 1_000_000,
                        "slope_millionths": 0
                    }
                }
            },
            "modules": [{
                "name": "native-conformance",
                "type": "script",
                "programmingLanguage": "generic",
                "uri": format!("file:{}", modules.display()),
                "completion": {
                    "marker": "{{OUTPUT_DIRECTORY}}/.complete.json",
                    "outputs": ["{{OUTPUT_DIRECTORY}}/result.txt"]
                },
                "template": {
                    "arguments": ["-c", "test -r \"$_XYLEM_INPUT\" && test ! -w \"$_XYLEM_INPUT\" && test -d /xylem/prepared && test -w /xylem/tmp && test -w /xylem/output && ! touch /xylem/prepared/forbidden && ! curl --connect-timeout 1 https://example.invalid && printf 'published\\n' > /xylem/output/result.txt"],
                    "command": "sh"
                }
            }],
            "sandbox": true
        }))
        .expect("serialize native sandbox config"),
    )
    .expect("write native sandbox config");
    let policy = test_root.join("policy.json");
    write(
        &policy,
        serde_json::to_vec_pretty(&native_sandbox_policy(&runtime, &workload, &artifacts, &inputs, &modules, "delete", 3600))
            .expect("serialize native sandbox policy"),
    )
    .expect("write native sandbox policy");
    let output = xylem_command()
        .args([
            "run",
            config.to_str().expect("UTF-8 config path"),
            "--no-progress",
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
            "--reporter",
            "json",
            "--sandbox-policy",
            policy.to_str().expect("UTF-8 policy path"),
        ])
        .output()
        .expect("run native sandbox conformance workflow");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        read_to_string(published.join("result.txt")).expect("read published sandbox artifact"),
        "published\n"
    );
    assert!(published.join(".complete.json").is_file());
    #[cfg(target_os = "linux")]
    {
        let report_path = read_dir(&reports)
            .expect("read sandbox report directory")
            .map(|entry| entry.expect("sandbox report entry").path())
            .find(|path| path.extension().is_some_and(|extension| extension == "json"))
            .expect("sandbox JSON report");
        let report: xylem_lib::journal::RunSummary =
            serde_json::from_slice(&read(&report_path).expect("read sandbox JSON report")).expect("valid sandbox JSON report");
        assert_eq!(
            report.artifacts.as_ref().expect("sandbox artifact evidence").status,
            xylem_lib::sandbox::ArtifactPublicationStatus::Published
        );
        let lifecycle = report.sandbox.lifecycle.as_ref().expect("sandbox lifecycle evidence");
        assert!(lifecycle.finalized);
        assert_eq!(lifecycle.resources.len(), 2);
        assert!(lifecycle
            .resources
            .iter()
            .all(|resource| resource.status == xylem_lib::sandbox::SandboxResourceDisposition::Removed));
        assert!(native_sandbox_resources(&runtime, report.sandbox.policy_digest.as_deref().expect("sandbox policy digest")).is_empty());
        assert!(
            report.memory.as_ref().expect("sandbox memory evidence").inputs[0]
                .observed_peak
                .as_ref()
                .and_then(Memory::checked_bytes)
                .is_some_and(|bytes| bytes > 0),
            "Linux native sandbox run should record a positive cgroup v2 peak"
        );
        if let Some(evidence) = std::env::var_os("XYLEM_SANDBOX_EVIDENCE_DIR").map(PathBuf::from) {
            let journal_path = reports.join(format!("{}.journal.jsonl", report.run_id));
            create_dir_all(&evidence).expect("create sandbox evidence directory");
            copy(&journal_path, evidence.join(format!("{runtime}-journal.jsonl"))).expect("retain sandbox journal evidence");
            copy(&report_path, evidence.join(format!("{runtime}-summary.json"))).expect("retain sandbox summary evidence");
        }
    }
    let rejected = artifacts.join("rejected");
    let rejected_reports = test_root.join("rejected-reports");
    write(
        &config,
        serde_json::to_vec_pretty(&serde_json::json!({
            "input": [input],
            "output": rejected.display().to_string(),
            "requirements": ["curl", "python", "sh", "tar"],
            "options": {
                "memory": {
                    "budget": "1GB",
                    "estimator": {
                        "identity": "native-sandbox-conformance-v1",
                        "intercept": "128MB",
                        "maximum_input": "1MB",
                        "minimum_input": "1KB",
                        "safety_millionths": 1_000_000,
                        "slope_millionths": 0
                    }
                }
            },
            "modules": [{
                "name": "policy-rejection",
                "type": "script",
                "programmingLanguage": "generic",
                "uri": format!("file:{}", modules.display()),
                "completion": {
                    "marker": "{{OUTPUT_DIRECTORY}}/.complete.json",
                    "outputs": ["{{OUTPUT_DIRECTORY}}/rejected.txt"]
                },
                "template": {
                    "arguments": [
                        "-c",
                        "printf 'unpublished\\n' > /xylem/output/rejected.txt; printf '%s\\n' \"$0\" >&2; exit 7",
                        PROMPT_POLICY_REJECTION
                    ],
                    "command": "sh"
                }
            }],
            "sandbox": true
        }))
        .expect("serialize native policy rejection config"),
    )
    .expect("write native policy rejection config");
    let rejected_output = xylem_command()
        .args([
            "run",
            config.to_str().expect("UTF-8 policy rejection config path"),
            "--no-progress",
            "--report-output",
            rejected_reports.to_str().expect("UTF-8 policy rejection report path"),
            "--reporter",
            "json",
            "--sandbox-policy",
            policy.to_str().expect("UTF-8 policy path"),
        ])
        .output()
        .expect("run native sandbox policy rejection workflow");
    assert_eq!(rejected_output.status.code(), Some(exitcode::SOFTWARE));
    assert!(!rejected.exists(), "policy-rejected artifacts must remain unpublished");
    let rejected_report_path = read_dir(&rejected_reports)
        .expect("read policy rejection report directory")
        .map(|entry| entry.expect("policy rejection report entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("policy rejection JSON report");
    let rejected_report: xylem_lib::journal::RunSummary =
        serde_json::from_slice(&read(&rejected_report_path).expect("read policy rejection JSON report")).expect("valid policy rejection JSON report");
    assert_eq!(rejected_report.status, xylem_lib::journal::RunStatus::Failed);
    assert_eq!(rejected_report.inputs[0].state, xylem_lib::journal::ExecutionState::Failed);
    assert_eq!(
        rejected_report.artifacts.as_ref().expect("policy rejection artifact evidence").status,
        xylem_lib::sandbox::ArtifactPublicationStatus::Discarded
    );
    let lifecycle = rejected_report.sandbox.lifecycle.as_ref().expect("policy rejection lifecycle evidence");
    assert!(lifecycle.finalized);
    assert_eq!(lifecycle.resources.len(), 2);
    assert!(lifecycle
        .resources
        .iter()
        .all(|resource| resource.status == xylem_lib::sandbox::SandboxResourceDisposition::Removed));
    assert!(native_sandbox_resources(
        &runtime,
        rejected_report.sandbox.policy_digest.as_deref().expect("policy rejection policy digest")
    )
    .is_empty());
    let error = rejected_report.inputs[0].error.as_deref().expect("policy rejection diagnostic");
    assert!(error.contains(PROMPT_POLICY_REJECTION), "{error}");
    assert!(error.len() <= xylem_lib::journal::MAX_ERROR_BYTES);
    assert!(rejected_report.memory.as_ref().expect("policy rejection memory evidence").inputs[0]
        .observed_peak
        .is_none());
    let rejected_journal_path = rejected_reports.join(format!("{}.journal.jsonl", rejected_report.run_id));
    assert!(!read_to_string(&rejected_journal_path)
        .expect("read policy rejection journal")
        .contains("\"event\":\"input_memory_observed\""));
    if let Some(evidence) = std::env::var_os("XYLEM_SANDBOX_EVIDENCE_DIR").map(PathBuf::from) {
        create_dir_all(&evidence).expect("create sandbox evidence directory");
        std::fs::copy(&rejected_journal_path, evidence.join(format!("{runtime}-policy-rejection-journal.jsonl")))
            .expect("retain policy rejection journal evidence");
        std::fs::copy(&rejected_report_path, evidence.join(format!("{runtime}-policy-rejection-summary.json")))
            .expect("retain policy rejection summary evidence");
    }
    remove_dir_all(test_root).expect("remove native sandbox fixture");
}
#[test]
#[ignore = "requires a native rootless OCI runtime and digest-pinned sandbox workload"]
fn test_native_sandbox_controller_death_and_retention_recovery() {
    let runtime = std::env::var("XYLEM_SANDBOX_RUNTIME").expect("XYLEM_SANDBOX_RUNTIME must select docker or podman");
    let workload = std::env::var("XYLEM_SANDBOX_WORKLOAD").expect("XYLEM_SANDBOX_WORKLOAD must be digest-pinned");
    let test_root = std::env::temp_dir().join(format!("xylem-native-sandbox-recovery-{}", std::process::id()));
    let artifacts = test_root.join("artifacts");
    let inputs = test_root.join("inputs");
    let modules = test_root.join("modules");
    let store = test_root.join("store");
    create_dir_all(&artifacts).expect("create recovery artifact root");
    create_dir_all(&inputs).expect("create recovery input root");
    create_dir_all(&modules).expect("create recovery module root");
    let policy_path = test_root.join("policy.json");
    write(
        &policy_path,
        serde_json::to_vec_pretty(&native_sandbox_policy(&runtime, &workload, &artifacts, &inputs, &modules, "delete", 1))
            .expect("serialize recovery policy"),
    )
    .expect("write recovery policy");
    let policy_digest = xylem_lib::sandbox::EffectiveSandboxPolicy::read(&policy_path)
        .expect("read recovery policy")
        .digest;
    let config = test_root.join("config.json");
    let published = artifacts.join("controller-published");
    write(
        &config,
        serde_json::to_vec_pretty(&native_lifecycle_config(
            &modules,
            &published,
            "sleep 30; printf 'unexpected\\n' > /xylem/output/result.txt",
        ))
        .expect("serialize controller-death config"),
    )
    .expect("write controller-death config");
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_xylem"));
    command.env(xylem_lib::run::store::RUN_STORE_ENV, &store).args([
        "run",
        config.to_str().expect("UTF-8 recovery config path"),
        "--no-progress",
        "--sandbox-policy",
        policy_path.to_str().expect("UTF-8 recovery policy path"),
    ]);
    let mut child = command.spawn().expect("spawn controller-death workflow");
    (0..300)
        .find_map(|_| {
            let resources = native_sandbox_resources(&runtime, &policy_digest);
            match resources.iter().any(|name| name.ends_with("-exec")) {
                | true => Some(()),
                | false => {
                    std::thread::sleep(core::time::Duration::from_millis(100));
                    None
                }
            }
        })
        .expect("controller-death workflow should create a labeled resource");
    child.kill().expect("kill sandbox controller");
    child.wait().expect("reap killed sandbox controller");
    let abandoned = native_sandbox_resources(&runtime, &policy_digest);
    assert!(!abandoned.is_empty());
    let manifest = read_dir(&store)
        .expect("read recovery run store")
        .map(|entry| entry.expect("recovery run entry").path().join("manifest.json"))
        .find_map(|path| {
            read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<xylem_lib::run::store::Manifest>(&bytes).ok())
        })
        .expect("controller-death manifest");
    let abandoned_workspace = std::env::temp_dir()
        .join(xylem_lib::script::EnvironmentValue::PREFIX)
        .join(&manifest.run_id);
    let abandoned_artifacts = artifacts.join(format!(".xylem-{}-artifacts", manifest.run_id));
    assert!(abandoned_workspace.exists());
    assert!(abandoned_artifacts.exists());
    std::thread::sleep(core::time::Duration::from_secs(2));
    write(
        &config,
        serde_json::to_vec_pretty(&native_lifecycle_config(
            &modules,
            &published,
            "printf 'recovered\\n' > /xylem/output/result.txt",
        ))
        .expect("serialize recovery config"),
    )
    .expect("write recovery config");
    let recovery_reports = test_root.join("controller-reports");
    let recovery = xylem_command()
        .env(xylem_lib::run::store::RUN_STORE_ENV, &store)
        .args([
            "run",
            config.to_str().expect("UTF-8 recovery config path"),
            "--no-progress",
            "--report-output",
            recovery_reports.to_str().expect("UTF-8 recovery reports path"),
            "--reporter",
            "json",
            "--sandbox-policy",
            policy_path.to_str().expect("UTF-8 recovery policy path"),
        ])
        .output()
        .expect("run controller-death recovery workflow");
    assert!(recovery.status.success(), "{}", String::from_utf8_lossy(&recovery.stderr));
    let recovery_report = native_report(&recovery_reports);
    let reconciled = &recovery_report
        .sandbox
        .lifecycle
        .as_ref()
        .expect("recovery lifecycle evidence")
        .reconciled;
    assert!(abandoned
        .iter()
        .all(|name| reconciled.iter().any(|resource| resource.identifier == *name)));
    assert!(!abandoned_workspace.exists());
    assert!(!abandoned_artifacts.exists());
    assert!(native_sandbox_resources(&runtime, &policy_digest).is_empty());

    write(
        &policy_path,
        serde_json::to_vec_pretty(&native_sandbox_policy(
            &runtime,
            &workload,
            &artifacts,
            &inputs,
            &modules,
            "retain_on_failure",
            1,
        ))
        .expect("serialize retention policy"),
    )
    .expect("write retention policy");
    let retention_digest = xylem_lib::sandbox::EffectiveSandboxPolicy::read(&policy_path)
        .expect("read retention policy")
        .digest;
    let retained_output = artifacts.join("retained-published");
    write(
        &config,
        serde_json::to_vec_pretty(&native_lifecycle_config(
            &modules,
            &retained_output,
            "printf 'discarded\\n' > /xylem/output/result.txt; exit 7",
        ))
        .expect("serialize retained failure config"),
    )
    .expect("write retained failure config");
    let retained_reports = test_root.join("retained-reports");
    let retained = xylem_command()
        .env(xylem_lib::run::store::RUN_STORE_ENV, &store)
        .args([
            "run",
            config.to_str().expect("UTF-8 retained config path"),
            "--no-progress",
            "--report-output",
            retained_reports.to_str().expect("UTF-8 retained reports path"),
            "--reporter",
            "json",
            "--sandbox-policy",
            policy_path.to_str().expect("UTF-8 retention policy path"),
        ])
        .output()
        .expect("run retained failure workflow");
    assert!(!retained.status.success());
    let retained_report = native_report(&retained_reports);
    let retained_ids = retained_report
        .sandbox
        .lifecycle
        .as_ref()
        .expect("retained lifecycle evidence")
        .resources
        .iter()
        .filter(|resource| resource.status == xylem_lib::sandbox::SandboxResourceDisposition::Retained)
        .map(|resource| resource.identifier.clone())
        .collect::<Vec<_>>();
    assert!(!retained_ids.is_empty());
    assert!(retained_ids
        .iter()
        .all(|identifier| native_sandbox_resources(&runtime, &retention_digest).contains(identifier)));
    assert!(!retained_output.exists());
    std::thread::sleep(core::time::Duration::from_secs(2));
    write(
        &config,
        serde_json::to_vec_pretty(&native_lifecycle_config(
            &modules,
            &retained_output,
            "printf 'published\\n' > /xylem/output/result.txt",
        ))
        .expect("serialize retention recovery config"),
    )
    .expect("write retention recovery config");
    let retention_recovery_reports = test_root.join("retention-recovery-reports");
    let retention_recovery = xylem_command()
        .env(xylem_lib::run::store::RUN_STORE_ENV, &store)
        .args([
            "run",
            config.to_str().expect("UTF-8 retention recovery config path"),
            "--no-progress",
            "--report-output",
            retention_recovery_reports.to_str().expect("UTF-8 retention recovery reports path"),
            "--reporter",
            "json",
            "--sandbox-policy",
            policy_path.to_str().expect("UTF-8 retention policy path"),
        ])
        .output()
        .expect("run retention recovery workflow");
    assert!(
        retention_recovery.status.success(),
        "{}",
        String::from_utf8_lossy(&retention_recovery.stderr)
    );
    let retention_recovery_report = native_report(&retention_recovery_reports);
    let reconciled = &retention_recovery_report
        .sandbox
        .lifecycle
        .as_ref()
        .expect("retention recovery lifecycle evidence")
        .reconciled;
    assert!(retained_ids
        .iter()
        .all(|identifier| reconciled.iter().any(|resource| resource.identifier == *identifier)));
    assert!(native_sandbox_resources(&runtime, &retention_digest).is_empty());
    remove_dir_all(test_root).expect("remove native sandbox recovery fixture");
}
#[test]
fn test_offline_dry_run_is_request_free_but_still_enforces_allowlist() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let config = "tests/e2e/offline-remote-config.json";
    let mut allowed = xylem_command();
    allowed
        .current_dir(workspace)
        .args(["run", config, "--offline", "--dry-run", "--allowed", "example.com"])
        .assert()
        .success();
    let mut rejected = xylem_command();
    rejected
        .current_dir(workspace)
        .args(["run", config, "--offline", "--dry-run", "--allowed", "other.example"])
        .assert()
        .failure()
        .code(exitcode::UNAVAILABLE);
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_offline_smoke_example_runs_from_a_clean_directory() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let test_root = std::env::temp_dir().join(format!(
        "xylem-offline-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos()
    ));
    let source = workspace.join("examples/offline-smoke");
    let example = test_root.join("examples/offline-smoke");
    create_dir_all(example.join("module")).expect("create clean smoke example");
    write(
        example.join(".xylem.json"),
        read(source.join(".xylem.json")).expect("read smoke configuration"),
    )
    .expect("copy smoke configuration");
    write(
        example.join("module/run.sh"),
        read(source.join("module/run.sh")).expect("read smoke module"),
    )
    .expect("copy smoke module");
    let mut dry_run = xylem_command();
    let dry_run = dry_run
        .current_dir(&example)
        .args(["run", "--offline", "--dry-run", "--reporter", "json"])
        .output()
        .expect("run smoke dry run");
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    serde_json::from_slice::<serde_json::Value>(&dry_run.stdout).expect("valid smoke execution plan");
    assert!(!example.join("output").exists());
    let mut run = xylem_command();
    let run = run
        .current_dir(&example)
        .args(["-vvvv", "run", "--offline", "--no-progress"])
        .output()
        .expect("run smoke workflow");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let diagnostics = String::from_utf8_lossy(&run.stderr);
    assert!(diagnostics.contains("greeting-Ada.txt"), "{diagnostics}");
    assert!(diagnostics.contains("greeting-Grace.txt"), "{diagnostics}");
    assert_eq!(
        read_to_string(example.join("output/greeting-Ada.txt")).expect("Ada greeting"),
        "Hello, Ada!\n"
    );
    assert_eq!(
        read_to_string(example.join("output/greeting-Grace.txt")).expect("Grace greeting"),
        "Hello, Grace!\n"
    );
    remove_dir_all(test_root).expect("remove smoke example");
}
#[test]
fn test_openshell_example_has_valid_public_dry_run() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let example = workspace.join("examples/openshell-provider");
    let output = xylem_command()
        .current_dir(&example)
        .args([
            "--sandbox-policy",
            "sandbox-policy.yaml",
            "run",
            ".xylem.jsonc",
            "--dry-run",
            "--reporter",
            "json",
        ])
        .output()
        .expect("run OpenShell example dry-run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Warning: OpenShell sandbox support is experimental"));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid OpenShell example plan");
    assert_eq!(plan["sandbox"]["provider"], "openshell");
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
    let mut command = xylem_command();
    command
        .args([
            "run",
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
    let mut command = xylem_command();
    command
        .args([
            "run",
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
    let mut command = xylem_command();
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
    let mut command = xylem_command();
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
    let mut command = xylem_command();
    let output = command
        .args(["run", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run invalid location workflow");
    assert_eq!(output.status.code(), Some(exitcode::DATAERR));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("modules[0].uri"), "{stderr}");
    remove_dir_all(test_root).expect("remove invalid location directory");
}
#[test]
fn test_resume_logs_when_a_module_has_no_completion_declaration() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let mut command = xylem_command();
    let output = command
        .current_dir(workspace)
        .args(["-vvvv", "run", "tests/e2e/config.json", "--dry-run", "--resume"])
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
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
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
        let mut command = xylem_command();
        command
            .current_dir(workspace)
            .args([
                "run",
                config_path.to_str().expect("UTF-8 config"),
                "--output",
                reports.to_str().expect("UTF-8 reports"),
                "--var",
                "MODE=first",
            ])
            .args(arguments)
            .assert()
            .success();
    }
    assert_eq!(read_to_string(&counter).expect("read invocation counter").lines().count(), 1);
    xylem_command()
        .current_dir(workspace)
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config"),
            "--output",
            reports.to_str().expect("UTF-8 reports"),
            "--resume",
            "--var",
            "MODE=second",
        ])
        .assert()
        .success();
    assert_eq!(read_to_string(&counter).expect("read invocation counter").lines().count(), 2);
    let reports = read_dir(&reports)
        .expect("read reports")
        .map(|entry| entry.expect("report entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 3);
    assert!(reports.iter().any(|path| {
        xylem_lib::journal::RunJournal::read(path).is_ok_and(|summary| summary.inputs[0].state == xylem_lib::journal::ExecutionState::SkippedExisting)
    }));
    remove_dir_all(test_root).expect("remove resume fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_resume_skips_early_complete_module_and_runs_later_incomplete_module() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
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
    let module = |name: &str, uri: &Path, marker: &str, output: &str, command: String| {
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
        let mut command = xylem_command();
        command
            .current_dir(workspace)
            .args([
                "run",
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
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let mut command = xylem_command();
    command
        .current_dir(workspace)
        .args(["-vv", "run", "tests/e2e/failing-config.json"])
        .assert()
        .failure()
        .code(exitcode::SOFTWARE);
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_runs_fixture_modules_in_order_with_interpolated_environment() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    for threads in ["1", "3"] {
        let mut command = xylem_command();
        let output = command
            .current_dir(workspace)
            .args(["--threads", threads, "-vvvv", "run", "tests/e2e/config.json"])
            .output()
            .expect("xylem fixture workflow should run");
        let logs = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "workflow failed with {threads} threads:\n{logs}");
        let first = logs.find("XYLEM_E2E_ONE_OK").expect("first fixture module should succeed");
        let second = logs.find("XYLEM_E2E_TWO_OK").expect("second fixture module should succeed");
        assert!(first < second, "fixture modules ran out of order with {threads} threads:\n{logs}");
    }
}
#[cfg(unix)]
#[test]
fn test_runs_input_basename_in_templates_and_environment() {
    let root = std::env::temp_dir().join(format!("xylem-input-basename-cli-{}", std::process::id()));
    let module = root.join("module");
    let artifacts = root.join("artifacts");
    let data = root.join("basin range");
    create_dir_all(&module).expect("create module");
    create_dir_all(&artifacts).expect("create artifacts");
    create_dir_all(&data).expect("create input directory");
    write(data.join("elevation.tif"), "input").expect("create file input");
    write(
        module.join("check.sh"),
        r#"set -eu
expected=$(basename "$_XYLEM_INPUT")
test "$1" = "$expected"
test "$_XYLEM_INPUT_BASENAME" = "$expected"
printf '%s\n' "$1" >> "$_XYLEM_OUTPUT_DIRECTORY/results.txt"
"#,
    )
    .expect("write input basename probe");
    let configuration = root.join("workflow.json");
    write(
        &configuration,
        serde_json::to_vec(&serde_json::json!({
            "input": { "select": {
                "engine": "cel", "options": { "roots": ["{{ DATA_PARENT }}"], "recursive": true },
                "script": "items.filter(item, item.name == 'basin range' || item.name == 'elevation.tif').map(item, item.path)"
            } },
            "variables": { "DATA_PARENT": root },
            "modules": [{
                "name": "probe",
                "programmingLanguage": "generic",
                "template": { "command": "sh", "arguments": ["check.sh", "{{ INPUT_BASENAME }}"] },
                "type": "script",
                "uri": format!("file:{}", module.display())
            }],
            "output": artifacts,
            "requirements": ["sh"]
        }))
        .expect("serialize configuration"),
    )
    .expect("write configuration");
    let preview = xylem_command()
        .arg("run")
        .arg(&configuration)
        .args(["--dry-run", "--reporter", "json"])
        .output()
        .expect("preview input basenamees");
    assert!(preview.status.success(), "{}", String::from_utf8_lossy(&preview.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&preview.stdout).expect("dry-run plan");
    for (index, expected) in ["basin range", "elevation.tif"].iter().enumerate() {
        let module = &plan["modules"][index];
        assert_eq!(module["variables"]["_XYLEM_INPUT_BASENAME"], *expected);
        assert_eq!(module["command"]["arguments"], serde_json::json!(["check.sh", expected]));
    }
    let output = xylem_command()
        .arg("run")
        .arg(&configuration)
        .args(["--threads", "1", "--no-progress", "--reporter", "json", "--report-output"])
        .arg(root.join("reports"))
        .output()
        .expect("run input basenamees");
    assert!(
        output.status.success(),
        "workflow failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        read_to_string(artifacts.join("results.txt")).expect("recorded basenames"),
        "basin range\nelevation.tif\n"
    );
    remove_dir_all(root).expect("remove input basename fixture");
}
#[cfg(unix)]
#[test]
fn test_runs_repository_root_python_with_installed_pixi_envs() {
    let root = std::env::temp_dir().join(format!("xylem-root-pixi-cli-{}", std::process::id()));
    let repository = root.join("repository");
    create_dir_all(repository.join(".pixi/envs/default/lib")).expect("installed environment");
    create_dir_all(repository.join("data")).expect("data directory");
    create_dir_all(repository.join("lib")).expect("source directory");
    for (path, contents) in [
        (".gitignore", "data/\n.pixi/\n"),
        (".pixi/config.toml", "run-post-link-scripts = 'insecure'\n"),
        (".pixi/envs/default/lib/installed", "installed package"),
        ("data/input.txt", "workflow data"),
        ("pixi.lock", "version: 6\n"),
        ("pixi.toml", "[workspace]\nname = 'probe'\n"),
        ("pyproject.toml", "[project]\nname = 'probe'\n"),
    ] {
        write(repository.join(path), contents).expect("repository file");
    }
    symlink("missing", repository.join(".pixi/envs/default/lib/broken")).expect("broken installed link");
    write(
        repository.join("lib/run.py"),
        r#"import os
from pathlib import Path
assert not Path('.pixi/envs').exists()
for name in ['pyproject.toml', 'pixi.toml', 'pixi.lock', '.pixi/config.toml']:
    assert Path(name).is_file(), name
data = Path('data/input.txt')
assert data.read_text() == 'workflow data'
data.write_text('isolated change')
output = Path(os.environ['_XYLEM_OUTPUT_DIRECTORY'])
output.mkdir(parents=True, exist_ok=True)
(output / 'result.txt').write_text('repository root succeeded')
"#,
    )
    .expect("Python module");
    let configuration = repository.join(".xylem.json");
    write(
        &configuration,
        serde_json::to_vec(&serde_json::json!({
            "modules": [{
                "name": "Validation",
                "programmingLanguage": "generic",
                "template": {"arguments": ["lib/run.py"], "command": "python3"},
                "type": "script",
                "uri": "file:."
            }],
            "requirements": ["python3"]
        }))
        .expect("serialize configuration"),
    )
    .expect("write configuration");
    let output = xylem_command()
        .current_dir(&repository)
        .args(["run", "--offline", "--no-progress", "--reporter", "json", "--artifact-output"])
        .arg(root.join("artifacts"))
        .arg("--report-output")
        .arg(root.join("reports"))
        .output()
        .expect("run repository-root workflow");
    assert!(
        output.status.success(),
        "workflow failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        read_to_string(root.join("artifacts/result.txt")).expect("workflow result"),
        "repository root succeeded"
    );
    assert_eq!(read_to_string(repository.join("data/input.txt")).expect("original data"), "workflow data");
    assert!(repository.join(".pixi/envs/default/lib/installed").is_file());
    assert!(symlink_metadata(repository.join(".pixi/envs/default/lib/broken"))
        .expect("original link")
        .file_type()
        .is_symlink());
    let report = native_report(&root.join("reports"));
    assert_eq!(report.inputs[0].state, xylem_lib::journal::ExecutionState::Succeeded);
    assert_eq!(report.inputs[0].modules[0].state, xylem_lib::journal::ExecutionState::Succeeded);
    remove_dir_all(root).expect("cleanup fixture");
}
#[test]
fn test_sandbox_activation_is_explicit_and_fail_closed() {
    let test_root = std::env::temp_dir().join(format!("xylem-sandbox-activation-{}", std::process::id()));
    create_dir_all(&test_root).expect("create sandbox activation fixture");
    let disabled = test_root.join("disabled.json");
    let enabled = test_root.join("enabled.json");
    let outside = test_root.join("outside.json");
    let openshell_policy = test_root.join("openshell-policy.json");
    let policy = test_root.join("policy.json");
    write(&disabled, r#"{"requirements":[],"modules":[],"sandbox":false}"#).expect("write disabled sandbox fixture");
    write(&enabled, r#"{"requirements":[],"modules":[],"sandbox":true}"#).expect("write enabled sandbox fixture");
    write(
        &outside,
        serde_json::to_vec_pretty(&serde_json::json!({
            "input": std::env::temp_dir(),
            "requirements": [],
            "modules": [],
            "sandbox": true
        }))
        .expect("serialize outside-root fixture"),
    )
    .expect("write outside-root fixture");
    write(
        &policy,
        serde_json::to_vec_pretty(&serde_json::json!({
            "environment": { "allow": ["PATH"] },
            "lifecycle": { "retention": "delete", "stale_after_seconds": 3600 },
            "minimum_isolation": "shared_kernel",
            "paths": { "artifact_roots": ["."], "input_roots": ["."], "module_roots": ["."] },
            "preparation_network": "deny",
            "process_guards": [],
            "provider": { "runtime": "docker", "type": "oci" },
            "resources": {
                "artifact_bytes": 1048576,
                "artifact_files": 100,
                "captured_output_bytes": 65536,
                "cpu_millis": 1000,
                "memory_bytes": 268435456,
                "pids": 64,
                "scratch_bytes": 1048576,
                "wall_time_seconds": 60
            },
            "version": 1,
            "workload": {
                "allowed": ["example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
                "default": "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }
        }))
        .expect("serialize sandbox policy"),
    )
    .expect("write sandbox policy");
    let disabled_output = xylem_command()
        .args(["run", disabled.to_str().expect("UTF-8 path"), "--dry-run", "--reporter", "json"])
        .output()
        .expect("run disabled sandbox dry-run");
    assert!(
        disabled_output.status.success(),
        "disabled sandbox changed host behavior: {}",
        String::from_utf8_lossy(&disabled_output.stderr)
    );
    let missing_policy = xylem_command()
        .args(["run", enabled.to_str().expect("UTF-8 path"), "--dry-run"])
        .output()
        .expect("run missing-policy fixture");
    assert!(!missing_policy.status.success());
    assert!(String::from_utf8_lossy(&missing_policy.stderr).contains("requires --sandbox-policy"));
    let enabled_output = xylem_command()
        .args([
            "run",
            enabled.to_str().expect("UTF-8 path"),
            "--dry-run",
            "--reporter",
            "json",
            "--sandbox-policy",
            policy.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run enabled sandbox dry-run");
    assert!(enabled_output.status.success(), "{}", String::from_utf8_lossy(&enabled_output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&enabled_output.stdout).expect("valid sandbox plan");
    assert_eq!(plan["sandbox"]["mode"], "required");
    assert_eq!(plan["sandbox"]["provider"], "oci");
    assert_eq!(plan["sandbox"]["runtime"], "docker");
    let mut openshell_policy_value: serde_json::Value =
        serde_json::from_slice(&read(&policy).expect("read sandbox policy")).expect("parse sandbox policy for OpenShell fixture");
    openshell_policy_value["provider"] = serde_json::json!({ "driver": "podman", "type": "openshell" });
    write(
        &openshell_policy,
        serde_json::to_vec_pretty(&openshell_policy_value).expect("serialize OpenShell policy"),
    )
    .expect("write OpenShell policy");
    let openshell_output = xylem_command()
        .args([
            "run",
            enabled.to_str().expect("UTF-8 path"),
            "--dry-run",
            "--reporter",
            "json",
            "--sandbox-policy",
            openshell_policy.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run OpenShell sandbox dry-run");
    assert!(openshell_output.status.success(), "{}", String::from_utf8_lossy(&openshell_output.stderr));
    assert!(String::from_utf8_lossy(&openshell_output.stderr).contains("Warning: OpenShell sandbox support is experimental"));
    let plan: serde_json::Value = serde_json::from_slice(&openshell_output.stdout).expect("valid OpenShell sandbox plan");
    assert_eq!(plan["sandbox"]["provider"], "openshell");
    assert_eq!(plan["sandbox"]["runtime"], "podman");
    let outside_output = xylem_command()
        .args([
            "run",
            outside.to_str().expect("UTF-8 path"),
            "--dry-run",
            "--sandbox-policy",
            policy.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run outside-root sandbox dry-run");
    assert!(!outside_output.status.success());
    assert!(String::from_utf8_lossy(&outside_output.stderr).contains("outside the operator policy roots"));
    let unused_policy = xylem_command()
        .args([
            "run",
            disabled.to_str().expect("UTF-8 path"),
            "--dry-run",
            "--sandbox-policy",
            test_root.join("missing-policy.json").to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run unused-policy fixture");
    assert!(!unused_policy.status.success());
    assert!(String::from_utf8_lossy(&unused_policy.stderr).contains("requires workflow configuration sandbox: true"));
    remove_dir_all(test_root).expect("remove sandbox activation fixture");
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
    let mut command = xylem_command();
    let output = command
        .args(["run", config_path.to_str().expect("UTF-8 config path")])
        .output()
        .expect("run spawn failure workflow");
    assert_eq!(output.status.code(), Some(exitcode::SOFTWARE));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("xylem-command-that-cannot-exist"), "{stderr}");
    assert!(
        stderr.contains("/xylem/workspaces/") || stderr.contains("\\xylem\\workspaces\\"),
        "{stderr}"
    );
    remove_dir_all(test_root).expect("remove spawn failure directory");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_termination_signal_cancels_inputs_flushes_journal_and_removes_workspaces() {
    let test_root = std::env::temp_dir().join(format!("xylem-signal-cancellation-{}", std::process::id()));
    let module = test_root.join("module");
    let reports = test_root.join("reports");
    let state = test_root.join("state");
    create_dir_all(&module).expect("create signal module");
    create_dir_all(&reports).expect("create signal reports");
    create_dir_all(&state).expect("create signal state");
    write(
        module.join("run.sh"),
        r#"#!/bin/sh
set -eu
state="$1"
printf '%s\n' "$_XYLEM_WORK_DIRECTORY" > "$state/${_XYLEM_INPUT}.workspace"
: > "$state/${_XYLEM_INPUT}.started"
sleep 30
"#,
    )
    .expect("write signal probe");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "input": ["one", "two", "three"],
        "modules": [{
            "name": "Signal Probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["run.sh", state], "command": "sh" },
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "options": { "batch": 2 },
        "requirements": ["sh"]
    });
    write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize signal config")).expect("write signal config");
    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin!("xylem"));
    command
        .env(
            xylem_lib::run::store::RUN_STORE_ENV,
            std::env::temp_dir().join(format!("xylem-cli-run-store-{}", std::process::id())),
        )
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--report-output",
            reports.to_str().expect("UTF-8 report path"),
            "--reporter",
            "json",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn().expect("spawn signal workflow");
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(10);
    while !(state.join("one.started").exists() && state.join("two.started").exists()) && std::time::Instant::now() < deadline {
        std::thread::sleep(core::time::Duration::from_millis(10));
    }
    assert!(state.join("one.started").exists(), "first signal input should start");
    assert!(state.join("two.started").exists(), "second signal input should start");
    let signal = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("send termination signal");
    assert!(signal.success(), "termination signal should be delivered");
    let output = child.wait_with_output().expect("wait for cancelled workflow");
    assert_eq!(output.status.code(), Some(130), "{}", String::from_utf8_lossy(&output.stderr));
    let report = native_report(&reports);
    assert_eq!(report.status, xylem_lib::journal::RunStatus::Cancelled);
    assert_eq!(report.stop_reason, Some(xylem_lib::journal::RunStopReason::ExternalCancellation));
    assert!(report
        .inputs
        .iter()
        .all(|input| input.state == xylem_lib::journal::ExecutionState::Cancelled));
    for input in ["one", "two"] {
        let workspace = read_to_string(state.join(format!("{input}.workspace"))).expect("read input workspace");
        assert!(
            !Path::new(workspace.trim()).exists(),
            "cancelled workspace should be removed: {workspace}"
        );
    }
    remove_dir_all(test_root).expect("remove signal cancellation directory");
}
#[cfg(feature = "tui")]
#[test]
fn test_tui_rejects_a_missing_explicit_configuration() {
    let missing = std::env::temp_dir().join(format!("xylem-missing-tui-config-{}.json", std::process::id()));
    let _ = remove_file(&missing);
    let mut command = xylem_command();
    let output = command
        .args(["tui", missing.to_str().expect("UTF-8 missing path")])
        .output()
        .expect("run TUI with missing configuration");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(&missing.display().to_string()));
}
#[test]
fn test_workspace_cleanup_retention_and_dry_run_contract() {
    let test_root = std::env::temp_dir().join(format!("xylem-workspace-lifecycle-{}", std::process::id()));
    let temporary = test_root.join("temporary");
    let module = test_root.join("module");
    create_dir_all(&module).expect("create workspace lifecycle module");
    create_dir_all(&temporary).expect("create injected temporary directory");
    let config_path = test_root.join("config.json");
    let config = serde_json::json!({
        "modules": [{
            "name": "Workspace lifecycle",
            "programmingLanguage": "generic",
            "template": { "arguments": [], "command": "true" },
            "type": "script",
            "uri": format!("file:{}", module.display())
        }],
        "requirements": []
    });
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize workspace lifecycle config"),
    )
    .expect("write workspace lifecycle config");

    let normal = xylem_command()
        .env("TMPDIR", &temporary)
        .args(["run", config_path.to_str().expect("UTF-8 config path"), "--no-progress"])
        .output()
        .expect("run workspace cleanup workflow");
    assert!(normal.status.success(), "{}", String::from_utf8_lossy(&normal.stderr));
    let workspace_parent = temporary.join("xylem/workspaces");
    assert_eq!(read_dir(&workspace_parent).expect("read workspace parent").count(), 0);

    let dry_run = xylem_command()
        .env("TMPDIR", &temporary)
        .args(["run", config_path.to_str().expect("UTF-8 config path"), "--dry-run", "--retain-workspace"])
        .output()
        .expect("run workspace dry-run workflow");
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    assert_eq!(read_dir(&workspace_parent).expect("read workspace parent after dry-run").count(), 0);

    let mut failing_config = config.clone();
    failing_config["modules"][0]["template"]["command"] = serde_json::json!("xylem-command-that-cannot-exist");
    write(
        &config_path,
        serde_json::to_vec_pretty(&failing_config).expect("serialize failing workspace lifecycle config"),
    )
    .expect("write failing workspace lifecycle config");
    let failed = xylem_command()
        .env("TMPDIR", &temporary)
        .args(["run", config_path.to_str().expect("UTF-8 config path"), "--no-progress"])
        .output()
        .expect("run failing workspace cleanup workflow");
    assert!(!failed.status.success());
    assert_eq!(read_dir(&workspace_parent).expect("read workspace parent after failure").count(), 0);
    write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize restored workspace lifecycle config"),
    )
    .expect("restore workspace lifecycle config");

    let retained = xylem_command()
        .env("TMPDIR", &temporary)
        .args([
            "run",
            config_path.to_str().expect("UTF-8 config path"),
            "--no-progress",
            "--retain-workspace",
        ])
        .output()
        .expect("run retained workspace workflow");
    assert!(retained.status.success(), "{}", String::from_utf8_lossy(&retained.stderr));
    let stderr = String::from_utf8_lossy(&retained.stderr);
    let retained_path = stderr
        .lines()
        .find_map(|line| line.strip_prefix("Workspace: "))
        .map(PathBuf::from)
        .expect("retained workspace path");
    assert!(retained_path.starts_with(&workspace_parent));
    assert!(retained_path.join("configuration").is_dir());
    assert!(retained_path.join("inputs/input-000000/environments").is_dir());
    assert!(retained_path.join("inputs/input-000000/modules/module-000000").is_dir());
    assert!(retained_path.join("inputs/input-000000/scratch").is_dir());

    remove_dir_all(&retained_path).expect("remove retained workspace");
    remove_dir_all(test_root).expect("remove workspace lifecycle fixture");
}
#[cfg(not(target_os = "windows"))]
#[test]
fn test_writes_versioned_journal_and_json_report_from_the_same_summary() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate should be inside the workspace");
    let output_directory = std::env::temp_dir().join(format!("xylem-durable-success-{}", std::process::id()));
    create_dir_all(&output_directory).expect("create durable output directory");
    let mut command = xylem_command();
    command
        .current_dir(workspace)
        .args([
            "run",
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
    assert!(report.memory.is_none());
    assert!(report
        .inputs
        .iter()
        .all(|input| input.state == xylem_lib::journal::ExecutionState::Succeeded));
    assert!(report
        .inputs
        .iter()
        .flat_map(|input| &input.modules)
        .all(|module| !module.module_path.is_empty()));
    remove_dir_all(output_directory).expect("remove durable output directory");
}
