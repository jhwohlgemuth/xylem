#![expect(clippy::indexing_slicing, reason = "test indexing deliberately fails on malformed fixtures")]
use super::*;

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
