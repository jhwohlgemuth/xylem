#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
use super::*;
use crate::acquisition::AcquisitionCache;
use crate::config::{Config, ConfigOptions};
use crate::workspace::RunWorkspace;
use alloc::collections::BTreeSet;
use std::fs::remove_dir_all;
use std::path::PathBuf;

fn resources() -> SandboxResources {
    SandboxResources {
        artifact_bytes: 1_048_576,
        artifact_files: 100,
        captured_output_bytes: 1_048_576,
        cpu_millis: 1000,
        memory_bytes: 536_870_912,
        pids: 2048,
        scratch_bytes: 67_108_864,
        wall_time_seconds: 30,
    }
}

#[test]
fn test_create_arguments_are_mountless_and_policy_bounded() {
    let identity = DockerSandboxIdentity::new(
        "run_123456",
        7,
        SandboxResourcePhase::Execution,
        &format!("sha256:{}", "a".repeat(64)),
        1_700_000_000_000,
    )
    .expect("create identity");
    let request = DockerSandboxCreateRequest {
        acquisition: WorkloadAcquisition::LocalOnly,
        identity: &identity,
        network_deny: true,
        resources: &resources(),
        workload: &format!("docker.io/docker/sandbox-templates:shell@sha256:{}", "b".repeat(64)),
    };
    let arguments = request
        .arguments()
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(arguments.last().map(String::as_str), Some("shell"));
    assert!(arguments.windows(2).any(|pair| pair == ["--cpus", "1"]));
    assert!(arguments.windows(2).any(|pair| pair == ["--deny-network", "**"]));
    assert!(arguments.windows(2).any(|pair| pair == ["--memory", "512m"]));
    assert!(arguments.windows(2).any(|pair| pair == ["--pull", "never"]));
    assert!(arguments.windows(2).any(|pair| pair == ["--skills", "off"]));
    assert!(!arguments.iter().any(|argument| argument == "."));
}

#[test]
fn test_execution_arguments_clear_inherited_environment_and_preserve_argv() {
    let client = DockerSandboxClient::new();
    let environment = [("XYLEM_VALUE".to_string(), "value with spaces".to_string())].into_iter().collect();
    let command = ["printf".to_string(), "$(literal)".to_string()];
    let arguments = client
        .execution_arguments("sandbox", "/xylem/prepared", &environment, &command)
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.windows(2).any(|pair| pair == ["env", "-i"]));
    assert_eq!(arguments[arguments.len() - 2..], command);
    assert!(arguments.iter().any(|argument| argument == "XYLEM_VALUE=value with spaces"));
}

#[test]
fn test_identity_round_trips_all_ownership_fields() {
    let identity = DockerSandboxIdentity::new(
        "run_123456",
        35,
        SandboxResourcePhase::Preparation,
        &format!("sha256:{}", "c".repeat(64)),
        1_700_000_000_000,
    )
    .expect("create identity");
    let parsed = DockerSandboxIdentity::parse(&identity.name).expect("parse identity");
    assert_eq!(parsed, identity);
    assert!(identity.name.len() <= 63);
    assert!(DockerSandboxIdentity::parse(&identity.name.replace("72756e", "72756E")).is_err());
}

#[test]
fn test_list_parser_accepts_array_and_envelope() {
    let array = parse_sandboxes(br#"[{"name":"one","status":"running"}]"#).expect("parse array");
    let envelope = parse_sandboxes(br#"{"sandboxes":[{"name":"two","status":"stopped"}]}"#).expect("parse envelope");
    assert_eq!(array[0].name, "one");
    assert_eq!(envelope[0].name, "two");
}

#[test]
fn test_local_oci_archive_uses_input_local_source_and_offline_workload() {
    let root = std::env::temp_dir().join(format!("xylem-docker-sandbox-oci-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("create Docker Sandbox OCI workspace");
    let input = workspace.input(0).expect("create Docker Sandbox OCI input");
    let archive = std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle.tar"))
        .expect("canonical OCI archive fixture");
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let source = format!("oci-layout:{}@{digest}", archive.display());
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "archive",
            "type": "script",
            "uri": source,
            "template": { "command": "sh", "arguments": ["run.sh"] }
        }],
        "requirements": [],
        "sandbox": true
    }))
    .expect("Docker Sandbox OCI configuration");
    let options = ConfigOptions::init()
        .build()
        .with_acquisition_cache(Some(AcquisitionCache::new(root.join("cache"))))
        .with_offline(Some(true));
    let prepared = config
        .with_options(options)
        .materialize_modules_selected_in_workspace(&BTreeSet::from([0]), None, &input)
        .expect("materialize Docker Sandbox OCI module");
    let expected = prepared.options.as_ref().and_then(|options| options.workspace_modules.get(&0)).cloned();
    assert_eq!(
        prepared.modules[0].sandbox_local_source().expect("resolve Docker Sandbox source"),
        expected
    );
    assert_eq!(prepared.modules[0].acquisition.as_ref().expect("OCI acquisition evidence").source, source);
    let identity = DockerSandboxIdentity::new(
        "run_oci",
        0,
        SandboxResourcePhase::Preparation,
        &format!("sha256:{}", "a".repeat(64)),
        1_700_000_000_000,
    )
    .expect("create Docker Sandbox OCI identity");
    let resources = resources();
    let request = DockerSandboxCreateRequest {
        acquisition: WorkloadAcquisition::LocalOnly,
        identity: &identity,
        network_deny: true,
        resources: &resources,
        workload: &format!("example.invalid/xylem@sha256:{}", "b".repeat(64)),
    };
    let arguments = request
        .arguments()
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.windows(2).any(|pair| pair == ["--deny-network", "**"]));
    assert!(arguments.windows(2).any(|pair| pair == ["--pull", "never"]));
    assert!(!arguments.iter().any(|argument| argument.contains("oci-layout:")));
    workspace.close().expect("close Docker Sandbox OCI workspace");
    remove_dir_all(root).expect("remove Docker Sandbox OCI fixture");
}

#[test]
#[ignore = "requires standalone sbx 0.45-0.47 and a running local Docker Sandbox daemon"]
fn test_native_docker_sandbox_feasibility() {
    let client = DockerSandboxClient::new();
    let (client_version, server_version) = client.inspect_provider().expect("discover Docker Sandbox");
    assert!(DockerSandboxVersion(&client_version).validate().is_ok());
    assert!(DockerSandboxVersion(&server_version).validate().is_ok());
}

#[test]
fn test_version_parser_accepts_qualified_json_and_rejects_prerelease() {
    let (client, server) = parse_versions(br#"{"client":{"version":"0.47.0"},"server":{"version":"v0.46.2"}}"#).expect("parse versions");
    assert!(DockerSandboxVersion(&client).validate().is_ok());
    assert!(DockerSandboxVersion(&server).validate().is_ok());
    assert!(SemanticVersion::parse("0.47.0-rc.1").is_err());
}
