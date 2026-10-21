//! GitLab export and one-module execution contracts.
use std::fs::{create_dir_all, read_to_string, write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock").as_nanos();
    let root = std::env::temp_dir().join(format!("xylem-{name}-{}-{nonce}", std::process::id()));
    create_dir_all(&root).expect("create fixture root");
    root
}
fn xylem(root: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    command.current_dir(root).env("XYLEM_RUN_STORE", root.join("runs"));
    command
}

#[test]
fn export_gitlab_discovers_nested_modules_and_checks_source_changes() {
    let root = fixture_root("gitlab-export");
    create_dir_all(root.join("child/leaf")).expect("create nested module");
    create_dir_all(root.join("second")).expect("create second module");
    write(
        root.join(".xylem.json"),
        r#"{
            "output": "./artifacts",
            "input": ["first", "second"],
            "requirements": ["mamba", "pixi"],
            "modules": [
                {"name": "Group", "type": "configuration", "uri": "file:./child/config.json"},
                {"name": "Second", "type": "script", "uri": "file:./second", "template": {"command": "true", "arguments": []}}
            ]
        }"#,
    )
    .expect("write root configuration");
    let child = root.join("child/config.json");
    write(
        &child,
        r#"{
            "requirements": [],
            "modules": [{"name": "Leaf", "type": "script", "uri": "file:./leaf", "template": {"command": "true", "arguments": []}}]
        }"#,
    )
    .expect("write child configuration");
    let output = xylem(&root).args(["export", "--gitlab"]).output().expect("export GitLab CI");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let generated = read_to_string(root.join(".gitlab-ci.yml")).expect("read GitLab CI");
    let pipeline: serde_json::Value = serde_norway::from_str(&generated).expect("parse GitLab CI");
    assert_eq!(pipeline["workflow"]["rules"][0]["if"].as_str(), Some("$CI_PIPELINE_SOURCE == \"web\""));
    assert_eq!(pipeline["stages"].as_array().map(Vec::len), Some(2));
    let setup = pipeline["default"]["before_script"].as_array().expect("GitLab setup commands");
    assert!(setup
        .iter()
        .any(|line| line.as_str().is_some_and(|line| line.contains("281b0ac7d550802e"))));
    assert!(setup
        .iter()
        .any(|line| line.as_str().is_some_and(|line| line.contains("4383aed18b2d5569"))));
    let first_job = pipeline
        .as_object()
        .and_then(|fields| fields.keys().find(|key| key.starts_with("xylem-000-")))
        .expect("first job name");
    assert_eq!(pipeline["xylem-001-second"]["needs"][0]["job"].as_str(), Some(first_job.as_str()));
    assert_eq!(pipeline["xylem-001-second"]["needs"][0]["artifacts"].as_bool(), Some(true));
    assert_eq!(pipeline["xylem-001-second"]["artifacts"]["paths"][0].as_str(), Some("artifacts"));
    assert!(pipeline["xylem-001-second"]["script"]
        .as_array()
        .expect("second job script")
        .iter()
        .any(|line| line.as_str().is_some_and(|line| line.contains("--module-index 1"))));
    let check = xylem(&root).args(["export", "--gitlab", "--check"]).output().expect("check GitLab CI");
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let unchanged = xylem(&root).args(["export", "--gitlab"]).output().expect("repeat export");
    assert!(unchanged.status.success(), "{}", String::from_utf8_lossy(&unchanged.stderr));
    write(&child, format!("{}\n", read_to_string(&child).expect("read child"))).expect("change child source");
    let stale = xylem(&root)
        .args(["export", "--gitlab", "--check"])
        .output()
        .expect("check stale GitLab CI");
    assert!(!stale.status.success());
    let force = xylem(&root).args(["export", "--gitlab", "--force"]).output().expect("replace GitLab CI");
    assert!(force.status.success(), "{}", String::from_utf8_lossy(&force.stderr));
    assert_ne!(
        read_to_string(root.join(".gitlab-ci.yml")).expect("read regenerated GitLab CI"),
        generated
    );
}
#[cfg(unix)]
#[test]
fn run_module_index_executes_only_selected_module() {
    let root = fixture_root("module-index");
    for name in ["first", "second"] {
        create_dir_all(root.join(name)).expect("create module");
        write(
            root.join(name).join("run.sh"),
            format!("#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$1\")\"\nprintf '%s\\n' {name} > \"$1\"\n"),
        )
        .expect("write module script");
        let script = root.join(name).join("run.sh");
        let mut permissions = script.metadata().expect("read script permissions").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(script, permissions).expect("make script executable");
    }
    write(
        root.join(".xylem.json"),
        r#"{
            "output": "./artifacts",
            "requirements": [],
            "modules": [
                {"name": "First", "type": "script", "uri": "file:./first", "template": {"command": "sh", "arguments": ["run.sh", "{{ OUTPUT_DIRECTORY }}/first.txt"]}},
                {"name": "Second", "type": "binary", "uri": "file:./second/run.sh", "template": {"command": "module", "arguments": ["{{ OUTPUT_DIRECTORY }}/second.txt"]}}
            ]
        }"#,
    )
    .expect("write workflow");
    let output = xylem(&root)
        .args(["--no-progress", "run", "--module-index", "1"])
        .output()
        .expect("run selected module");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(read_to_string(root.join("artifacts/second.txt")).expect("second artifact"), "second\n");
    assert!(!root.join("artifacts/first.txt").exists());
    let invalid = xylem(&root)
        .args(["--no-progress", "run", "--module-index", "2"])
        .output()
        .expect("run invalid module");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("out of range"));
}
