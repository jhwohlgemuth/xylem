use super::*;
use crate::sandbox::{EffectiveSandboxPolicy, OciRuntime, SandboxProviderKind};
use crate::util::timestamp;
use alloc::collections::BTreeMap;
use serde::Serialize;
use std::env;
use std::fs::{create_dir_all, read, read_to_string, remove_dir_all, write, File};
use std::path::{Path, PathBuf};
use tar::Builder;

const EXAMPLE_EXECUTION_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/openshell-provider/policies/execution.yaml"
));
const EXAMPLE_PREPARATION_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/openshell-provider/policies/preparation.yaml"
));
const QUALIFIED_OPENSHELL_VERSION: &str = "0.1.2";
const REQUIRED_CHECKS: [&str; 19] = [
    "cleanup",
    "cpu_limit",
    "driver",
    "environment",
    "exact_argv",
    "execution_network_deny",
    "filesystem_read_only",
    "gateway",
    "lifecycle",
    "memory_limit",
    "offline_semantics",
    "pids_limit",
    "policy_inspection",
    "preparation_network_exact_host",
    "preparation_unrestricted",
    "process_identity",
    "scratch_limit",
    "transfer",
    "workload_digest",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Blocked,
    Failed,
    Passed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FeasibilityVerdict {
    Blocked,
    Ready,
}
#[derive(Clone, Debug, Serialize)]
struct FeasibilityCheck {
    detail: String,
    status: CheckStatus,
}
#[derive(Clone, Debug, Serialize)]
struct FeasibilityReport {
    checks: BTreeMap<String, FeasibilityCheck>,
    cli_version: Option<String>,
    driver: String,
    execution_policy_digest: String,
    gateway_version: Option<String>,
    preparation_policy_digest: String,
    schema_version: u32,
    verdict: FeasibilityVerdict,
    workload: String,
}

impl FeasibilityReport {
    fn new(driver: OpenShellDriver, workload: String, preparation_policy: &str, execution_policy: &str) -> Self {
        let checks = REQUIRED_CHECKS
            .into_iter()
            .map(|name| {
                (
                    name.to_string(),
                    FeasibilityCheck {
                        detail: "not evaluated".to_string(),
                        status: CheckStatus::Blocked,
                    },
                )
            })
            .collect();
        Self {
            checks,
            cli_version: None,
            driver: driver.to_string(),
            execution_policy_digest: digest(execution_policy.as_bytes()),
            gateway_version: None,
            preparation_policy_digest: digest(preparation_policy.as_bytes()),
            schema_version: 1,
            verdict: FeasibilityVerdict::Blocked,
            workload,
        }
    }
    fn finish(mut self) -> Self {
        self.verdict = match self.checks.values().all(|check| matches!(check.status, CheckStatus::Passed)) {
            | true => FeasibilityVerdict::Ready,
            | false => FeasibilityVerdict::Blocked,
        };
        self
    }
    fn record(&mut self, name: &str, status: CheckStatus, detail: impl Into<String>) {
        self.checks.insert(
            name.to_string(),
            FeasibilityCheck {
                detail: detail.into(),
                status,
            },
        );
    }
}

fn arguments(arguments: Vec<OsString>) -> Vec<String> {
    arguments.into_iter().map(|argument| argument.to_string_lossy().into_owned()).collect()
}
fn cleanup_identity(client: &OpenShellClient, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
    client
        .list_sandboxes(identity)
        .and_then(|sandboxes| match sandboxes.iter().any(|sandbox| sandbox.name == identity.name) {
            | true => client.delete_sandbox_and_wait(identity),
            | false => Ok(()),
        })
}
fn create_example_module_archive(path: &Path) -> Result<(), SandboxError> {
    File::create(path)
        .map_err(|why| SandboxError::Runtime(format!("create OpenShell transfer archive: {why}")))
        .and_then(|file| {
            let mut builder = Builder::new(file);
            builder
                .append_path_with_name(example_directory().join("module/run.sh"), "run.sh")
                .and_then(|()| builder.finish())
                .map_err(|why| SandboxError::Runtime(format!("write OpenShell transfer archive: {why}")))
        })
}
fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn evidence_directory(driver: OpenShellDriver) -> PathBuf {
    env::var_os("XYLEM_SANDBOX_EVIDENCE_DIR").map_or_else(
        || env::temp_dir().join(format!("xylem-openshell-{}-{}", std::process::id(), driver)),
        PathBuf::from,
    )
}
fn exact_hosts(hosts: &[&str]) -> OpenShellNetworkAccess {
    OpenShellNetworkAccess::ExactEndpoints(
        hosts
            .iter()
            .map(|host| OpenShellNetworkEndpoint {
                host: (*host).to_string(),
                port: 443,
            })
            .collect(),
    )
}
fn example_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/openshell-provider")
}
fn record_command(report: &mut FeasibilityReport, client: &OpenShellClient, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
    let environment = [("XYLEM_VISIBLE".to_string(), "allowed".to_string())].into_iter().collect();
    let command = [
        "python".to_string(),
        "-c".to_string(),
        "import json,os,sys; print(json.dumps([sys.argv[1:], os.environ.get('XYLEM_VISIBLE'), os.environ.get('XYLEM_OPENSHELL_WORKLOAD')]))"
            .to_string(),
        "value with spaces".to_string(),
        "$(not-executed)".to_string(),
    ];
    client
        .execute_in_sandbox(&identity.name, "/xylem", &environment, &command)
        .and_then(|output| match output.status.success() {
            | false => Err(SandboxError::Runtime(format!(
                "OpenShell argv probe failed with status {}",
                output.status
            ))),
            | true => serde_json::from_slice::<(Vec<String>, Option<String>, Option<String>)>(&output.stdout)
                .map_err(|why| SandboxError::Runtime(format!("parse OpenShell argv probe: {why}"))),
        })
        .and_then(
            |(actual, visible, host_workload)| match (actual.as_slice(), visible.as_deref(), host_workload) {
                | ([space, literal], Some("allowed"), None) if space == "value with spaces" && literal == "$(not-executed)" => {
                    report.record("exact_argv", CheckStatus::Passed, "argument boundaries preserved");
                    report.record(
                        "environment",
                        CheckStatus::Passed,
                        "only explicitly requested probe variable was observed",
                    );
                    Ok(())
                }
                | _ => Err(SandboxError::Runtime(
                    "OpenShell changed argv boundaries or explicit environment".to_string(),
                )),
            },
        )
}
fn record_execution_checks(report: &mut FeasibilityReport, client: &OpenShellClient, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
    record_command(report, client, identity)
        .and_then(|()| {
            client
                .execute_in_sandbox(
                    &identity.name,
                    "/xylem",
                    &BTreeMap::new(),
                    &["sh".to_string(), "-c".to_string(), "test \"$(id -u):$(id -g)\" = 65532:65532".to_string()],
                )
                .and_then(|output| match output.status.success() {
                    | true => {
                        report.record("process_identity", CheckStatus::Passed, "observed uid:gid 65532:65532");
                        Ok(())
                    }
                    | false => Err(SandboxError::Runtime("OpenShell did not enforce uid:gid 65532:65532".to_string())),
                })
        })
        .and_then(|()| {
            client
                .execute_in_sandbox(
                    &identity.name,
                    "/xylem",
                    &BTreeMap::new(),
                    &[
                        "sh".to_string(),
                        "-c".to_string(),
                        "printf mutation >> /xylem/prepared/modules/0/run.sh".to_string(),
                    ],
                )
                .map(|output| match output.status.success() {
                    | true => report.record("filesystem_read_only", CheckStatus::Failed, "prepared input was mutable during execution"),
                    | false => report.record("filesystem_read_only", CheckStatus::Passed, "prepared input rejected mutation"),
                })
        })
        .and_then(|()| {
            client
                .execute_in_sandbox(
                    &identity.name,
                    "/xylem",
                    &BTreeMap::new(),
                    &[
                        "curl".to_string(),
                        "--fail".to_string(),
                        "--max-time".to_string(),
                        "5".to_string(),
                        "https://example.com".to_string(),
                    ],
                )
                .map(|output| match output.status.success() {
                    | true => report.record("execution_network_deny", CheckStatus::Failed, "unexpected network request succeeded"),
                    | false => report.record("execution_network_deny", CheckStatus::Passed, "undeclared network request was denied"),
                })
        })
}
fn record_preparation_network(report: &mut FeasibilityReport, client: &OpenShellClient, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
    let curl = |host: &str| {
        client.execute_in_sandbox(
            &identity.name,
            "/xylem",
            &BTreeMap::new(),
            &[
                "curl".to_string(),
                "--fail".to_string(),
                "--head".to_string(),
                "--max-time".to_string(),
                "5".to_string(),
                format!("https://{host}"),
            ],
        )
    };
    curl("example.com").and_then(|allowed| {
        curl("example.org").and_then(|denied| match (allowed.status.success(), denied.status.success()) {
            | (true, false) => {
                report.record(
                    "preparation_network_exact_host",
                    CheckStatus::Passed,
                    "declared host succeeded and undeclared host was denied",
                );
                Ok(())
            }
            | _ => Err(SandboxError::Runtime(
                "OpenShell preparation network did not enforce the exact-host policy".to_string(),
            )),
        })
    })
}
fn record_resource_limits(report: &mut FeasibilityReport, client: &OpenShellClient, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
    client
        .execute_in_sandbox(
            &identity.name,
            "/xylem",
            &BTreeMap::new(),
            &[
                "python".to_string(),
                "-c".to_string(),
                "import json,pathlib; print(json.dumps({name:pathlib.Path('/sys/fs/cgroup', name).read_text().strip() for name in ['cpu.max','memory.max','pids.max']}))".to_string(),
            ],
        )
        .and_then(|output| match output.status.success() {
            | true => serde_json::from_slice::<BTreeMap<String, String>>(&output.stdout)
                .map_err(|why| SandboxError::Runtime(format!("parse OpenShell cgroup limits: {why}"))),
            | false => Err(SandboxError::Runtime(format!(
                "OpenShell cgroup limit probe failed with status {}",
                output.status
            ))),
        })
        .and_then(|observed| validate_resource_limits(report, &observed, &resources()))
}
fn resource_limit<'a>(observed: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, SandboxError> {
    observed
        .get(name)
        .map(String::as_str)
        .filter(|value| *value != "max")
        .ok_or_else(|| SandboxError::Runtime(format!("OpenShell {name} is unavailable or unlimited")))
}
fn resources() -> SandboxResources {
    SandboxResources {
        artifact_bytes: 1_048_576,
        artifact_files: 100,
        captured_output_bytes: 1_048_576,
        cpu_millis: 500,
        memory_bytes: 268_435_456,
        pids: 2048,
        scratch_bytes: 67_108_864,
        wall_time_seconds: 30,
    }
}
fn run_feasibility(
    client: &OpenShellClient,
    driver: OpenShellDriver,
    workload: &str,
    directory: &Path,
    preparation_policy_path: &Path,
    execution_policy_path: &Path,
    report: &mut FeasibilityReport,
) -> Result<(), SandboxError> {
    client
        .inspect_provider()
        .and_then(|(version, gateway)| {
            report.cli_version = Some(version.to_string());
            report.gateway_version = Some(gateway.version.clone());
            let driver_version_matches =
                matches!(gateway.compute_drivers.as_slice(), [driver] if driver.capabilities.driver_version == QUALIFIED_OPENSHELL_VERSION);
            SemanticVersion::parse(&gateway.version)
                .and_then(|gateway_version| {
                    match version.to_string() == QUALIFIED_OPENSHELL_VERSION
                        && gateway_version.to_string() == QUALIFIED_OPENSHELL_VERSION
                        && driver_version_matches
                    {
                        | true => {
                            report.record("gateway", CheckStatus::Passed, "authenticated local OpenShell 0.1.2 gateway is active");
                            report.record("driver", CheckStatus::Passed, format!("exactly one {driver} compute driver is active"));
                            Ok(())
                        }
                        | false => Err(SandboxError::Runtime(format!(
                            "qualification requires OpenShell CLI, gateway, and compute driver {QUALIFIED_OPENSHELL_VERSION}"
                        ))),
                    }
                })
                .and_then(|()| match is_pinned_digest(workload) {
                    | true => {
                        report.record("workload_digest", CheckStatus::Passed, "workload uses a complete sha256 digest");
                        Ok(())
                    }
                    | false => Err(SandboxError::Validation(
                        "XYLEM_OPENSHELL_WORKLOAD must use a complete sha256 digest".to_string(),
                    )),
                })
        })
        .and_then(|()| {
            let run_id = format!("qualification-{}", timestamp().get());
            let workspace = directory.join(&run_id);
            let policy_digest = digest(format!("{}:{}", report.preparation_policy_digest, report.execution_policy_digest).as_bytes());
            let preparation = OpenShellIdentity::new(&run_id, 0, SandboxResourcePhase::Preparation, &policy_digest, timestamp().get());
            let execution = OpenShellIdentity::new(&run_id, 0, SandboxResourcePhase::Execution, &policy_digest, timestamp().get());
            create_dir_all(&workspace)
                .map_err(|why| SandboxError::Runtime(format!("create OpenShell example workspace: {why}")))
                .and(preparation)
                .and_then(|preparation| execution.map(|execution| (preparation, execution)))
                .and_then(|(preparation, execution)| {
                    let preparation_request = OpenShellCreateRequest {
                        command: &["sleep".to_string(), "infinity".to_string()],
                        identity: &preparation,
                        policy: preparation_policy_path,
                        resources: &resources(),
                        workload,
                    };
                    let lifecycle = client.create_sandbox(&preparation_request).and_then(|created| {
                        match created.name == preparation.name {
                            | true => client.wait_ready(&preparation),
                            | false => Err(SandboxError::Runtime("OpenShell create returned an unexpected sandbox name".to_string())),
                        }
                        .and_then(|observed| match observed.policy.is_some() {
                            | true => Ok(()),
                            | false => Err(SandboxError::Runtime("OpenShell sandbox inspection omitted effective policy".to_string())),
                        })
                        .and_then(|()| {
                            report.record(
                                "policy_inspection",
                                CheckStatus::Passed,
                                "effective policy and preparation ownership labels round-tripped",
                            );
                            let source = workspace.join("module-source.tar");
                            let transfer = workspace.join("prepared.tar");
                            let prepared = workspace.join("prepared");
                            let result = workspace.join("greeting.txt");
                            create_example_module_archive(&source)
                                .and_then(|()| record_preparation_network(report, client, &preparation))
                                .and_then(|()| client.upload_file(&preparation.name, &source, "/xylem/input/module.tar"))
                                .and_then(|()| {
                                    client.execute_in_sandbox(
                                        &preparation.name,
                                        "/xylem",
                                        &BTreeMap::new(),
                                        &[
                                            "sh".to_string(),
                                            "-c".to_string(),
                                            "set -eu; mkdir -p /xylem/prepared/modules/0; tar -C /xylem/prepared/modules/0 -xf /xylem/input/module.tar; tar -C /xylem/prepared -cf /xylem/tmp/prepared.tar .".to_string(),
                                        ],
                                    )
                                })
                                .and_then(|output| match output.status.success() {
                                    | true => client.download_file(&preparation.name, "/xylem/tmp/prepared.tar", &transfer),
                                    | false => Err(SandboxError::Runtime("OpenShell example preparation failed".to_string())),
                                })
                                .and_then(|()| validate_prepared_archive(&transfer, &prepared))
                                .and_then(|prepared_module| {
                                    client.delete_sandbox_and_wait(&preparation).map(|()| prepared_module)
                                })
                                .and_then(|prepared_module| {
                                    let execution_request = OpenShellCreateRequest {
                                        command: &["sleep".to_string(), "infinity".to_string()],
                                        identity: &execution,
                                        policy: execution_policy_path,
                                        resources: &resources(),
                                        workload,
                                    };
                                    client.create_sandbox(&execution_request).map(|created| (created, prepared_module))
                                })
                                .and_then(|(created, prepared_module)| match created.name == execution.name {
                                    | true => client.wait_ready(&execution).map(|ready| (ready, prepared_module)),
                                    | false => Err(SandboxError::Runtime("OpenShell create returned an unexpected sandbox name".to_string())),
                                })
                                .and_then(|(_, prepared_module)| {
                                    client
                                        .upload_file(&execution.name, &prepared_module, "/xylem/prepared/modules/0/run.sh")
                                        .and_then(|()| {
                                            client.upload_file(
                                                &execution.name,
                                                &example_directory().join("input/name.txt"),
                                                "/xylem/input/current",
                                            )
                                        })
                                })
                                .and_then(|()| {
                                    let environment = [
                                        ("OUTPUT_DIRECTORY".to_string(), "/xylem/output".to_string()),
                                        ("_XYLEM_INPUT".to_string(), "/xylem/input/current".to_string()),
                                    ]
                                    .into_iter()
                                    .collect();
                                    client.execute_in_sandbox(
                                        &execution.name,
                                        "/xylem/prepared/modules/0",
                                        &environment,
                                        &[
                                            "sh".to_string(),
                                            "/xylem/prepared/modules/0/run.sh".to_string(),
                                            "Hello".to_string(),
                                        ],
                                    )
                                })
                                .and_then(|output| match output.status.success() {
                                    | true => client.download_file(&execution.name, "/xylem/output/greeting.txt", &result),
                                    | false => Err(SandboxError::Runtime("OpenShell example execution failed".to_string())),
                                })
                                .and_then(|()| validate_example_output(&result))
                                .map(|()| {
                                    report.record(
                                        "transfer",
                                        CheckStatus::Passed,
                                        "checked-in module and input crossed separate phases and produced the expected artifact",
                                    );
                                })
                                .and_then(|()| record_resource_limits(report, client, &execution))
                                .and_then(|()| record_execution_checks(report, client, &execution))
                                .map(|()| {
                                    report.record("lifecycle", CheckStatus::Passed, "separate preparation and execution sandboxes started");
                                })
                        })
                    });
                    let cleanup_errors = [&preparation, &execution]
                        .into_iter()
                        .filter_map(|identity| cleanup_identity(client, identity).err().map(|why| why.to_string()))
                        .collect::<Vec<_>>();
                    let cleanup = match cleanup_errors.is_empty() {
                        | true => {
                            report.record("cleanup", CheckStatus::Passed, "all qualification sandboxes were deleted");
                            Ok(())
                        }
                        | false => {
                            let detail = cleanup_errors.join("; ");
                            report.record("cleanup", CheckStatus::Failed, detail.clone());
                            Err(SandboxError::Runtime(detail))
                        }
                    };
                    match (lifecycle, cleanup) {
                        | (Err(why), _) | (Ok(()), Err(why)) => Err(why),
                        | (Ok(()), Ok(())) => Ok(()),
                    }
                })
        })
}
fn validate_example_output(path: &Path) -> Result<(), SandboxError> {
    read_to_string(path)
        .map_err(|why| SandboxError::Runtime(format!("read OpenShell example output: {why}")))
        .and_then(|contents| match contents == "Hello, OpenShell!\n" {
            | true => Ok(()),
            | false => Err(SandboxError::Runtime(format!(
                "OpenShell example returned unexpected output {contents:?}"
            ))),
        })
}
fn validate_prepared_archive(path: &Path, destination: &Path) -> Result<PathBuf, SandboxError> {
    super::super::archive::extract(path, destination, resources().scratch_bytes, resources().artifact_files)
        .map_err(|why| SandboxError::Runtime(format!("validate OpenShell prepared archive: {why}")))
        .and_then(|()| {
            let prepared_module = destination.join("modules/0/run.sh");
            read(&prepared_module)
                .map_err(|why| SandboxError::Runtime(format!("read OpenShell prepared module: {why}")))
                .and_then(|prepared| {
                    read(example_directory().join("module/run.sh"))
                        .map_err(|why| SandboxError::Runtime(format!("read OpenShell example module: {why}")))
                        .and_then(|expected| match prepared == expected {
                            | true => Ok(prepared_module),
                            | false => Err(SandboxError::Runtime("OpenShell changed the prepared module".to_string())),
                        })
                })
        })
}
fn validate_resource_limits(
    report: &mut FeasibilityReport,
    observed: &BTreeMap<String, String>,
    expected: &SandboxResources,
) -> Result<(), SandboxError> {
    resource_limit(observed, "cpu.max")
        .and_then(|value| {
            value
                .split_ascii_whitespace()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|why| SandboxError::Runtime(format!("parse OpenShell cpu.max: {why}")))
        })
        .and_then(|cpu| match cpu.as_slice() {
            | [quota, period] if *quota > 0 && *period > 0 && quota.saturating_mul(1000) <= u64::from(expected.cpu_millis).saturating_mul(*period) => {
                Ok((*quota, *period))
            }
            | _ => Err(SandboxError::Runtime(format!(
                "OpenShell cpu.max does not enforce {} millicpu: {cpu:?}",
                expected.cpu_millis
            ))),
        })
        .and_then(|(quota, period)| {
            resource_limit(observed, "memory.max").and_then(|value| {
                value
                    .parse::<u64>()
                    .map_err(|why| SandboxError::Runtime(format!("parse OpenShell memory.max: {why}")))
                    .and_then(|memory| match memory > 0 && memory <= expected.memory_bytes {
                        | true => Ok((quota, period, memory)),
                        | false => Err(SandboxError::Runtime(format!(
                            "OpenShell memory.max {memory} exceeds policy limit {}",
                            expected.memory_bytes
                        ))),
                    })
            })
        })
        .and_then(|(quota, period, memory)| {
            resource_limit(observed, "pids.max").and_then(|value| {
                value
                    .parse::<u32>()
                    .map_err(|why| SandboxError::Runtime(format!("parse OpenShell pids.max: {why}")))
                    .and_then(|pids| match pids > 0 && pids <= expected.pids {
                        | true => Ok((quota, period, memory, pids)),
                        | false => Err(SandboxError::Runtime(format!(
                            "OpenShell pids.max {pids} exceeds policy limit {}",
                            expected.pids
                        ))),
                    })
            })
        })
        .map(|(quota, period, memory, pids)| {
            report.record(
                "cpu_limit",
                CheckStatus::Passed,
                format!("observed cgroup cpu.max {quota} {period}"),
            );
            report.record(
                "memory_limit",
                CheckStatus::Passed,
                format!("observed cgroup memory.max {memory}"),
            );
            report.record(
                "pids_limit",
                CheckStatus::Passed,
                format!("observed cgroup pids.max {pids}"),
            );
        })
}
#[test]
fn test_create_arguments_disable_implicit_providers_and_pin_resources() {
    let identity = OpenShellIdentity::new(
        "run",
        1,
        SandboxResourcePhase::Preparation,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        2,
    )
    .expect("build OpenShell identity");
    let resources = resources();
    let command = ["sleep".to_string(), "infinity".to_string()];
    let request = OpenShellCreateRequest {
        command: &command,
        identity: &identity,
        policy: Path::new("/tmp/policy.yaml"),
        resources: &resources,
        workload: "example.invalid/xylem@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    };
    let arguments = arguments(request.arguments());
    assert!(arguments.contains(&"--no-auto-providers".to_string()));
    assert!(arguments.contains(&"--detach".to_string()));
    assert!(arguments.contains(&"500m".to_string()));
    assert!(arguments.contains(&"268435456".to_string()));
}
#[test]
fn test_example_operator_policy_selects_openshell_provider() {
    let directory = std::fs::canonicalize(example_directory()).expect("resolve example directory");
    let policy = EffectiveSandboxPolicy::read(&directory.join("sandbox-policy.yaml")).expect("read example OpenShell operator policy");
    assert_eq!(policy.provider, SandboxProviderKind::OpenShell);
    assert_eq!(policy.runtime, Some(OciRuntime::Docker));
}
#[test]
fn test_example_policies_match_generated_translation() {
    let preparation = OpenShellPolicy::new(SandboxResourcePhase::Preparation, exact_hosts(&["example.com"]))
        .and_then(|policy| policy.yaml())
        .expect("generate OpenShell preparation policy");
    let execution = OpenShellPolicy::new(SandboxResourcePhase::Execution, OpenShellNetworkAccess::Deny)
        .and_then(|policy| policy.yaml())
        .expect("generate OpenShell execution policy");
    let preparation: serde_json::Value = serde_norway::from_str(&preparation).expect("parse generated preparation policy");
    let example_preparation: serde_json::Value = serde_norway::from_str(EXAMPLE_PREPARATION_POLICY).expect("parse example preparation policy");
    let execution: serde_json::Value = serde_norway::from_str(&execution).expect("parse generated execution policy");
    let example_execution: serde_json::Value = serde_norway::from_str(EXAMPLE_EXECUTION_POLICY).expect("parse example execution policy");
    assert_eq!(example_execution, execution);
    assert_eq!(example_preparation, preparation);
}
#[test]
fn test_gateway_validation_requires_one_expected_local_driver() {
    let gateway = OpenShellGatewayInfo::parse(
        br#"{
            "auth":"bearer",
            "compute_drivers":[{"capabilities":{"driver_name":"docker","driver_version":"0.1.2"},"name":"docker"}],
            "extensions":[],
            "gateway":"local",
            "server":"https://127.0.0.1:17670",
            "status":"healthy",
            "version":"0.1.2"
        }"#,
    )
    .expect("parse tagged OpenShell gateway output");
    assert!(validate_gateway(&gateway, OpenShellDriver::Docker).is_ok());
    assert!(validate_gateway(&gateway, OpenShellDriver::Podman).is_err());
    let unauthenticated = OpenShellGatewayInfo {
        auth: None,
        ..gateway.clone()
    };
    assert!(validate_gateway(&unauthenticated, OpenShellDriver::Docker).is_err());
    let remote = OpenShellGatewayInfo {
        server: "https://gateway.example.com".to_string(),
        ..gateway
    };
    assert!(validate_gateway(&remote, OpenShellDriver::Docker).is_err());
    assert!(!server_is_local("https://localhost.example.com"));
    assert!(server_is_local("http://127.0.0.2:8080"));
    assert!(server_is_local("https://[::1]:17670"));
}
#[test]
fn test_identity_uses_dns_name_and_complete_digest_labels() {
    let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let first =
        OpenShellIdentity::new("Run/With Hostile Spaces", 9, SandboxResourcePhase::Execution, digest, 3).expect("build first OpenShell identity");
    let second =
        OpenShellIdentity::new("Run/With Hostile Spaces", 9, SandboxResourcePhase::Execution, digest, 3).expect("build second OpenShell identity");
    assert_eq!(first, second);
    assert!(first
        .name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'));
    assert_eq!(first.labels[POLICY_DIGEST_FIRST_LABEL].len(), 32);
    assert_eq!(first.labels[POLICY_DIGEST_SECOND_LABEL].len(), 32);
    assert_eq!(
        format!("{}{}", first.labels[POLICY_DIGEST_FIRST_LABEL], first.labels[POLICY_DIGEST_SECOND_LABEL]),
        &digest[7..]
    );
}
#[test]
#[ignore = "requires OpenShell 0.1.x, an authenticated local gateway, and a digest-pinned workload"]
fn test_openshell_provider_feasibility() {
    let driver = env::var("XYLEM_OPENSHELL_DRIVER")
        .map_err(|why| SandboxError::Validation(format!("XYLEM_OPENSHELL_DRIVER is required: {why}")))
        .and_then(|value| match value.as_str() {
            | "docker" => Ok(OpenShellDriver::Docker),
            | "podman" => Ok(OpenShellDriver::Podman),
            | _ => Err(SandboxError::Validation(format!(
                "XYLEM_OPENSHELL_DRIVER must select docker or podman, not {value}"
            ))),
        })
        .expect("select OpenShell compute driver");
    let workload = env::var("XYLEM_OPENSHELL_WORKLOAD").expect("XYLEM_OPENSHELL_WORKLOAD must be digest-pinned");
    let directory = evidence_directory(driver);
    create_dir_all(&directory).expect("create OpenShell evidence directory");
    let preparation_policy = EXAMPLE_PREPARATION_POLICY.to_string();
    let execution_policy = EXAMPLE_EXECUTION_POLICY.to_string();
    let preparation_policy_path = directory.join("preparation-policy.yaml");
    let execution_policy_path = directory.join("execution-policy.yaml");
    write(&preparation_policy_path, &preparation_policy).expect("write OpenShell preparation policy");
    write(&execution_policy_path, &execution_policy).expect("write OpenShell execution policy");
    let mut report = FeasibilityReport::new(driver, workload.clone(), &preparation_policy, &execution_policy);
    report.record(
        "scratch_limit",
        CheckStatus::Blocked,
        "OpenShell 0.1.x create API has no portable scratch-size limit",
    );
    report.record(
        "offline_semantics",
        CheckStatus::Blocked,
        "offline preparation lifecycle is not yet probed",
    );
    report.record(
        "preparation_unrestricted",
        CheckStatus::Blocked,
        "OpenShell exact-host policies are not treated as unrestricted preparation networking",
    );
    let result = run_feasibility(
        &OpenShellClient::new(driver),
        driver,
        &workload,
        &directory,
        &preparation_policy_path,
        &execution_policy_path,
        &mut report,
    );
    if let Err(why) = &result {
        report.record("lifecycle", CheckStatus::Failed, why.to_string());
    }
    if report
        .checks
        .get("cleanup")
        .is_some_and(|check| check.status == CheckStatus::Blocked && check.detail == "not evaluated")
    {
        report.record("cleanup", CheckStatus::Passed, "no qualification sandboxes were created");
    }
    let cleanup_failed = report.checks.get("cleanup").is_some_and(|check| check.status == CheckStatus::Failed);
    let report = report.finish();
    let report_path = directory.join("openshell-feasibility.json");
    let serialized = serde_json::to_vec_pretty(&report).expect("serialize OpenShell feasibility report");
    write(&report_path, serialized).expect("write OpenShell feasibility report");
    assert_eq!(report.checks.len(), REQUIRED_CHECKS.len());
    assert!(report.checks.contains_key("cleanup"));
    assert!(!cleanup_failed, "OpenShell qualification cleanup failed");
    if env::var_os("XYLEM_SANDBOX_EVIDENCE_DIR").is_none() {
        remove_dir_all(&directory).expect("remove temporary OpenShell evidence directory");
    }
}
#[test]
fn test_policy_is_phase_specific_hard_landlock_and_non_root() {
    let preparation = OpenShellPolicy::new(SandboxResourcePhase::Preparation, exact_hosts(&["packages.example.com"]))
        .expect("build preparation policy")
        .yaml()
        .expect("serialize preparation policy");
    let execution = OpenShellPolicy::new(SandboxResourcePhase::Execution, OpenShellNetworkAccess::Deny)
        .expect("build execution policy")
        .yaml()
        .expect("serialize execution policy");
    assert!(preparation.contains("compatibility: hard_requirement"));
    assert!(
        preparation.contains("run_as_user: '65532'") || preparation.contains("run_as_user: \"65532\"") || preparation.contains("run_as_user: 65532")
    );
    assert!(preparation.contains("packages.example.com"));
    assert!(preparation.contains("/xylem/input"));
    assert!(!execution.contains("network_policies"));
    assert!(execution.contains("/xylem/prepared"));
    assert!(execution.contains("/xylem/output"));
    assert!(OpenShellPolicy::new(SandboxResourcePhase::Execution, exact_hosts(&["example.com"])).is_err());
    assert!(OpenShellPolicy::new(SandboxResourcePhase::Preparation, exact_hosts(&["*.example.com"])).is_err());
}
#[test]
fn test_reconciliation_requires_every_ownership_label() {
    let identity = OpenShellIdentity::new(
        "run",
        4,
        SandboxResourcePhase::Execution,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        9,
    )
    .expect("build OpenShell identity");
    let sandbox = OpenShellSandbox {
        id: "sandbox-id".to_string(),
        labels: identity.labels.clone(),
        name: identity.name.clone(),
        phase: "RUNNING".to_string(),
        policy: None,
    };
    assert!(validate_owned_sandbox(&identity, sandbox.clone()).is_ok());
    let mut wrong = sandbox;
    wrong.labels.remove(SANDBOX_RUN_LABEL);
    assert!(validate_owned_sandbox(&identity, wrong).is_err());
    let selector = identity.selector();
    assert_eq!(selector.split(',').count(), identity.labels.len());
    assert!(identity.labels.iter().all(|(name, value)| selector.contains(&format!("{name}={value}"))));
}
#[test]
fn test_resource_limit_observation_requires_bounded_cgroups() {
    let expected = resources();
    let observed = [
        ("cpu.max".to_string(), "50000 100000".to_string()),
        ("memory.max".to_string(), expected.memory_bytes.to_string()),
        ("pids.max".to_string(), expected.pids.to_string()),
    ]
    .into_iter()
    .collect();
    let mut report = FeasibilityReport::new(OpenShellDriver::Docker, "workload".to_string(), "preparation", "execution");
    validate_resource_limits(&mut report, &observed, &expected).expect("validate bounded cgroup controls");
    assert_eq!(report.checks["cpu_limit"].status, CheckStatus::Passed);
    assert_eq!(report.checks["memory_limit"].status, CheckStatus::Passed);
    assert_eq!(report.checks["pids_limit"].status, CheckStatus::Passed);

    let unlimited = [
        ("cpu.max".to_string(), "max 100000".to_string()),
        ("memory.max".to_string(), "max".to_string()),
        ("pids.max".to_string(), "max".to_string()),
    ]
    .into_iter()
    .collect();
    assert!(validate_resource_limits(&mut report, &unlimited, &expected).is_err());
}
#[test]
fn test_structured_json_parsers_reject_missing_identity() {
    let sandbox = OpenShellSandbox::parse(br#"{"name":"qualified","labels":{"owner":"xylem"},"phase":"RUNNING"}"#).expect("parse OpenShell sandbox");
    assert_eq!(sandbox.name, "qualified");
    assert!(OpenShellSandbox::parse(br#"{"labels":{}}"#).is_err());
    let list = OpenShellSandboxCollection::parse(br#"{"sandboxes":[{"name":"one"},{"name":"two"}]}"#).expect("parse OpenShell sandbox list");
    assert_eq!(list.len(), 2);
}
#[test]
fn test_timeouts_are_bounded() {
    let client = OpenShellClient::new(OpenShellDriver::Docker);
    assert_eq!(client.execution_timeout, Duration::from_secs(30));
    assert_eq!(client.management_timeout(), Duration::from_secs(30));
    assert!(OpenShellClient::DELETION_POLL_INTERVAL < OpenShellClient::DELETION_TIMEOUT);
    assert!(OpenShellClient::DELETION_TIMEOUT <= Duration::from_secs(30));
    assert!(OpenShellClient::READINESS_TIMEOUT <= Duration::from_secs(30));
}
#[test]
fn test_version_range_accepts_only_stable_zero_one_line() {
    let supported = |value| SemanticVersion::parse(value).is_ok_and(|version| OpenShellVersion(&version).validate().is_ok());
    assert_eq!(SemanticVersion::parse("openshell 0.1.2").expect("parse version").to_string(), "0.1.2");
    assert!(supported("OpenShell v0.1.99"));
    assert!(!supported("openshell 0.2.0"));
    assert!(!supported("openshell 1.0.0"));
    assert!(!supported("openshell 0.1.3-rc.1"));
    assert!(SemanticVersion::parse("no version").is_err());
}
#[test]
fn test_workload_requires_complete_digest() {
    assert!(is_pinned_digest(
        "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ));
    assert!(!is_pinned_digest("example.invalid/xylem:latest"));
    assert!(!is_pinned_digest("example.invalid/xylem@sha256:aaaa"));
}
