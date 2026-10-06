use super::*;
use crate::script::Command;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::process;

fn policy(format: &str) -> String {
    match format {
        | "json" | "jsonc" => r#"{
  "allowed_images": ["example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
  "backend": "docker",
  "default_image": "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "environment": { "allow": ["PATH", "LANG"] },
  "lifecycle": { "retention": "delete", "stale_after_seconds": 3600 },
  "paths": { "artifact_roots": ["artifacts"], "input_roots": ["inputs"], "module_roots": ["modules"] },
  "preparation_network": "deny",
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
  "version": 1
}"#
        .to_string(),
        | "yaml" | "yml" => r#"allowed_images:
  - example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
backend: docker
default_image: example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
environment:
  allow: [PATH, LANG]
lifecycle:
  retention: delete
  stale_after_seconds: 3600
paths:
  artifact_roots: [artifacts]
  input_roots: [inputs]
  module_roots: [modules]
preparation_network: deny
resources:
  artifact_bytes: 1048576
  artifact_files: 100
  captured_output_bytes: 65536
  cpu_millis: 1000
  memory_bytes: 268435456
  pids: 64
  scratch_bytes: 1048576
  wall_time_seconds: 60
version: 1
"#
        .to_string(),
        | _ => String::new(),
    }
}
fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-sandbox-policy-{label}-{}", process::id()))
}

#[test]
fn invocation_is_hardened_and_preserves_argument_boundaries() {
    let directory = root("invocation");
    let modules = directory.join("modules");
    let inputs = directory.join("inputs");
    create_dir_all(&modules).expect("create module fixture directory");
    create_dir_all(&inputs).expect("create input fixture directory");
    let input = inputs.join("sample data.txt");
    write(&input, "sample").expect("write input fixture");
    let policy_path = directory.join("policy.json");
    write(
        &policy_path,
        policy("json").replace("[\"PATH\", \"LANG\"]", "[\"PATH\", \"LANG\", \"_XYLEM_INPUT\"]"),
    )
    .expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&policy_path).expect("parse policy fixture");
    let script = Script::init()
        .command(Command::init().name("printf".to_string()).build())
        .arguments(["%s", input.to_str().expect("UTF-8 fixture path"), "; touch /host"])
        .current_dir(modules.to_str().expect("UTF-8 fixture path").to_string())
        .envs(std::collections::HashMap::from([(
            "_XYLEM_INPUT".to_string(),
            input.to_str().expect("UTF-8 fixture path").to_string(),
        )]))
        .build();
    let arguments = policy
        .invocation(&script, "xylem-run-0-0")
        .expect("build OCI invocation")
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.contains(&"--network=none".to_string()));
    assert!(arguments.contains(&"--read-only".to_string()));
    assert!(arguments.contains(&"--cap-drop=ALL".to_string()));
    assert!(arguments.contains(&"--security-opt=no-new-privileges".to_string()));
    assert!(arguments.contains(&"/xylem/input/current".to_string()), "{arguments:?}");
    assert!(arguments.contains(&"; touch /host".to_string()));
    assert_eq!(arguments.last().map(String::as_str), Some("; touch /host"));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[cfg(unix)]
#[test]
fn invocation_rejects_symlinks_inside_mounted_trees() {
    use std::os::unix::fs::symlink;

    let directory = root("symlink");
    let modules = directory.join("modules");
    create_dir_all(&modules).expect("create module fixture directory");
    symlink("/etc/passwd", modules.join("escape")).expect("create module symlink fixture");
    let policy_path = directory.join("policy.json");
    write(&policy_path, policy("json")).expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&policy_path).expect("parse policy fixture");
    let script = Script::init()
        .command(Command::init().name("true".to_string()).build())
        .current_dir(modules.to_str().expect("UTF-8 fixture path").to_string())
        .build();
    let error = policy.invocation(&script, "xylem-run-0-0").expect_err("symlinked mount must fail");
    assert!(error.to_string().contains("is a symlink"));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_formats_produce_the_same_canonical_digest() {
    let directory = root("formats");
    create_dir_all(&directory).expect("create policy fixture directory");
    let digests = ["json", "jsonc", "yaml", "yml"]
        .into_iter()
        .map(|extension| {
            let path = directory.join(format!("policy.{extension}"));
            write(&path, policy(extension)).expect("write policy fixture");
            EffectiveSandboxPolicy::read(&path).expect("parse policy fixture").digest
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(digests.len(), 1);
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_rejects_unknown_fields_and_mutable_images() {
    let directory = root("invalid");
    create_dir_all(&directory).expect("create policy fixture directory");
    let unknown = directory.join("unknown.json");
    write(
        &unknown,
        policy("json").replacen("\"version\": 1", "\"unknown\": true, \"version\": 1", 1),
    )
    .expect("write unknown-field fixture");
    assert!(EffectiveSandboxPolicy::read(&unknown).is_err());
    let mutable = directory.join("mutable.json");
    write(
        &mutable,
        policy("json").replace(
            "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "example.invalid/xylem:latest",
        ),
    )
    .expect("write mutable-image fixture");
    assert!(EffectiveSandboxPolicy::read(&mutable).is_err());
    remove_dir_all(directory).expect("remove policy fixture directory");
}
