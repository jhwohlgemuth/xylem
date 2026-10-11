//! OpenShell sandbox provider and qualification primitives.
extern crate alloc;

mod provider;

use super::{EffectiveSandboxPolicy, SandboxError, SandboxProviderClient, SandboxResourcePhase, SandboxResources};
use crate::util::constants::app::{
    POLICY_DIGEST_FIRST_LABEL, POLICY_DIGEST_SECOND_LABEL, PREPARATION_NETWORK_POLICY, SANDBOX_CREATED_LABEL, SANDBOX_INPUT_LABEL,
    SANDBOX_MANAGED_LABEL, SANDBOX_PHASE_LABEL, SANDBOX_PROVIDER_LABEL, SANDBOX_RESOURCE_VERSION, SANDBOX_RUN_LABEL, SANDBOX_VERSION_LABEL,
    SYSTEM_READ_ONLY_PATHS,
};
use acorn_cmd::{args, CommandError, ExecutionOptions, ExecutionResult};
use acorn_core::util::SemanticVersion;
use acorn_core::validation::ValidationError;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use core::time::Duration;
pub(super) use provider::OpenShellProvider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::Path;
#[cfg(test)]
use std::process::Output;

trait OpenShellSemanticVersion {
    fn parse(value: &str) -> Result<Self, SandboxError>
    where
        Self: Sized;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenShellDriver {
    Docker,
    Podman,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum OpenShellNetworkAccess {
    Deny,
    ExactEndpoints(Vec<OpenShellNetworkEndpoint>),
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
enum OpenShellSandboxCollection {
    Items(Vec<OpenShellSandbox>),
    Page {
        #[serde(default)]
        sandboxes: Vec<OpenShellSandbox>,
    },
}
#[derive(Clone, Debug)]
struct OpenShellClient {
    driver: OpenShellDriver,
    execution_timeout: Duration,
}
struct OpenShellCreateRequest<'a> {
    command: &'a [String],
    identity: &'a OpenShellIdentity,
    policy: &'a Path,
    resources: &'a SandboxResources,
    workload: &'a str,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OpenShellDriverCapabilities {
    driver_name: String,
    driver_version: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OpenShellDriverInfo {
    capabilities: OpenShellDriverCapabilities,
    name: String,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellFilesystemPolicy {
    include_workdir: bool,
    read_only: Vec<String>,
    read_write: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct OpenShellGatewayInfo {
    auth: Option<String>,
    compute_drivers: Vec<OpenShellDriverInfo>,
    gateway: String,
    server: String,
    status: String,
    version: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenShellIdentity {
    labels: BTreeMap<String, String>,
    name: String,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellLandlockPolicy {
    compatibility: &'static str,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellNetworkBinary {
    path: String,
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct OpenShellNetworkEndpoint {
    host: String,
    port: u16,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellNetworkPolicy {
    binaries: Vec<OpenShellNetworkBinary>,
    endpoints: Vec<OpenShellNetworkEndpoint>,
    name: String,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellPolicy {
    filesystem_policy: OpenShellFilesystemPolicy,
    landlock: OpenShellLandlockPolicy,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    network_policies: BTreeMap<String, OpenShellNetworkPolicy>,
    process: OpenShellProcessPolicy,
    version: u32,
}
#[derive(Clone, Debug, Serialize)]
struct OpenShellProcessPolicy {
    run_as_group: String,
    run_as_user: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OpenShellSandbox {
    #[serde(default)]
    id: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    name: String,
    #[serde(default)]
    phase: String,
    #[serde(default)]
    policy: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OpenShellSandboxEnvelope {
    sandbox: OpenShellSandbox,
}
#[derive(Clone, Copy, Debug)]
struct OpenShellVersion<'a>(&'a SemanticVersion);
impl OpenShellClient {
    fn new(driver: OpenShellDriver) -> Self {
        Self {
            driver,
            execution_timeout: Self::EXECUTION_TIMEOUT,
        }
    }
    fn create_sandbox(&self, request: &OpenShellCreateRequest<'_>) -> Result<OpenShellSandbox, SandboxError> {
        match is_pinned_digest(request.workload) {
            | true => SandboxProviderClient::create(self, request.arguments(), "create OpenShell sandbox")
                .and_then(|output| OpenShellSandbox::parse(&output.stdout)),
            | false => Err(SandboxError::Validation(
                "OpenShell workloads must use a complete sha256 digest".to_string(),
            )),
        }
    }
    fn delete_sandbox_and_wait(&self, identity: &OpenShellIdentity) -> Result<(), SandboxError> {
        SandboxProviderClient::delete_and_wait(
            self,
            args!["sandbox", "delete", &identity.name],
            "delete OpenShell sandbox",
            SandboxError::Runtime(format!(
                "OpenShell sandbox {} was not deleted within {} seconds",
                identity.name,
                Self::DELETION_TIMEOUT.as_secs()
            )),
            || {
                self.list_sandboxes(identity)
                    .map(|sandboxes| sandboxes.iter().all(|sandbox| sandbox.name != identity.name).then_some(()))
            },
        )
    }
    fn inspect_provider(&self) -> Result<(SemanticVersion, OpenShellGatewayInfo), SandboxError> {
        self.checked_output(args!["--version"], "read OpenShell CLI version")
            .and_then(|output| SemanticVersion::parse(&String::from_utf8_lossy(&output.stdout)))
            .and_then(|version| {
                OpenShellVersion(&version)
                    .validate()
                    .map_err(|why| SandboxError::Runtime(why.to_string()))
                    .and_then(|()| {
                        self.checked_output(args!["gateway", "info", "--output", "json"], "inspect OpenShell gateway")
                            .and_then(|output| OpenShellGatewayInfo::parse(&output.stdout))
                            .and_then(|gateway| validate_gateway(&gateway, self.driver).map(|()| (version, gateway)))
                    })
            })
    }
    fn download_file(&self, sandbox: &str, remote: &str, local: &Path) -> Result<(), SandboxError> {
        SandboxProviderClient::download(
            self,
            args!["sandbox", "download", sandbox, remote, local],
            "download from OpenShell sandbox",
        )
    }
    #[cfg(test)]
    fn execute_in_sandbox(
        &self,
        sandbox: &str,
        workdir: &str,
        environment: &BTreeMap<String, String>,
        command: &[String],
    ) -> Result<Output, SandboxError> {
        SandboxProviderClient::execute(
            self,
            self.execution_arguments(sandbox, workdir, environment, command),
            "execute in OpenShell sandbox",
        )
    }
    fn execute_in_sandbox_checked(
        &self,
        sandbox: &str,
        workdir: &str,
        environment: &BTreeMap<String, String>,
        command: &[String],
        options: &ExecutionOptions,
    ) -> Result<ExecutionResult, CommandError> {
        SandboxProviderClient::execute_checked(self, self.execution_arguments(sandbox, workdir, environment, command), options)
    }
    fn execute_in_sandbox_checked_output(
        &self,
        sandbox: &str,
        workdir: &str,
        environment: &BTreeMap<String, String>,
        command: &[String],
    ) -> Result<(), SandboxError> {
        SandboxProviderClient::execute_checked_output(
            self,
            self.execution_arguments(sandbox, workdir, environment, command),
            "OpenShell command",
        )
    }
    fn execution_arguments(&self, sandbox: &str, workdir: &str, environment: &BTreeMap<String, String>, command: &[String]) -> Vec<OsString> {
        let environment = environment.iter().flat_map(|(name, value)| args!["--env", format!("{name}={value}")]);
        args![
            "sandbox",
            "exec",
            ("--name", sandbox),
            ("--workdir", workdir),
            ("--timeout", self.execution_timeout.as_secs().to_string()),
            "--no-login-shell",
            "--no-tty",
            ..environment,
            "--",
            ..command.iter()
        ]
    }
    fn get_sandbox(&self, name: &str) -> Result<OpenShellSandbox, SandboxError> {
        SandboxProviderClient::get(self, args!["sandbox", "get", name, "--output", "json"], "inspect OpenShell sandbox")
            .and_then(|output| OpenShellSandbox::parse(&output.stdout))
    }
    fn list_sandboxes(&self, identity: &OpenShellIdentity) -> Result<Vec<OpenShellSandbox>, SandboxError> {
        self.list_sandboxes_by_selector(&identity.selector()).and_then(|sandboxes| {
            sandboxes
                .into_iter()
                .map(|sandbox| validate_owned_sandbox(identity, sandbox))
                .collect::<Result<Vec<_>, _>>()
        })
    }
    fn list_sandboxes_by_selector(&self, selector: &str) -> Result<Vec<OpenShellSandbox>, SandboxError> {
        SandboxProviderClient::list(
            self,
            args!["sandbox", "list", ("--selector", selector), "--output", "json"],
            "list OpenShell sandboxes",
        )
        .and_then(|output| OpenShellSandboxCollection::parse(&output.stdout))
    }
    fn upload_file(&self, sandbox: &str, local: &Path, remote: &str) -> Result<(), SandboxError> {
        SandboxProviderClient::upload(
            self,
            args!["sandbox", "upload", sandbox, local, remote, "--no-git-ignore"],
            "upload to OpenShell sandbox",
        )
    }
    fn wait_ready(&self, identity: &OpenShellIdentity) -> Result<OpenShellSandbox, SandboxError> {
        self.wait_for_readiness(
            SandboxError::Runtime(format!(
                "OpenShell sandbox {} was not ready within {} seconds",
                identity.name,
                Self::READINESS_TIMEOUT.as_secs()
            )),
            || {
                self.get_sandbox(&identity.name)
                    .and_then(|sandbox| validate_owned_sandbox(identity, sandbox))
                    .and_then(|sandbox| match sandbox.phase.to_ascii_lowercase().as_str() {
                        | "ready" | "running" => Ok(Some(sandbox)),
                        | "completed" | "error" | "failed" => Err(SandboxError::Runtime(format!(
                            "OpenShell sandbox {} entered terminal phase {} before readiness",
                            identity.name, sandbox.phase
                        ))),
                        | _ => Ok(None),
                    })
            },
        )
    }
}
impl SandboxProviderClient for OpenShellClient {
    const DELETION_TIMEOUT: Duration = Duration::from_secs(30);
    const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
    const MANAGEMENT_TIMEOUT: Duration = Duration::from_secs(30);
    const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

    fn discover(&self) -> Result<(), SandboxError> {
        self.inspect_provider().map(|_| ())
    }
    fn execution_timeout(&self) -> Duration {
        self.execution_timeout
    }
    fn program(&self) -> &'static str {
        "openshell"
    }
}
impl OpenShellCreateRequest<'_> {
    fn arguments(&self) -> Vec<OsString> {
        let labels = self
            .identity
            .labels
            .iter()
            .flat_map(|(name, value)| args!["--label", format!("{name}={value}")]);
        args![
            "sandbox",
            "create",
            "--output",
            "json",
            ("--name", &self.identity.name),
            ("--from", self.workload),
            ("--policy", self.policy),
            ("--cpu", format!("{}m", self.resources.cpu_millis)),
            ("--memory", self.resources.memory_bytes.to_string()),
            ..labels,
            "--no-auto-providers",
            "--detach",
            "--",
            ..self.command.iter()
        ]
    }
}
impl fmt::Display for OpenShellDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Docker => formatter.write_str("docker"),
            | Self::Podman => formatter.write_str("podman"),
        }
    }
}
impl OpenShellGatewayInfo {
    fn parse(bytes: &[u8]) -> Result<Self, SandboxError> {
        serde_json::from_slice(bytes).map_err(|why| SandboxError::Runtime(format!("parse OpenShell gateway information: {why}")))
    }
}
impl OpenShellIdentity {
    fn new(run_id: &str, input_index: usize, phase: SandboxResourcePhase, policy_digest: &str, created_at: u64) -> Result<Self, SandboxError> {
        policy_digest_hex(policy_digest).map(|digest| {
            let identity = format!("{run_id}\0{input_index}\0{phase}\0{policy_digest}");
            let name_hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
            let labels = [
                (SANDBOX_CREATED_LABEL, created_at.to_string()),
                (SANDBOX_INPUT_LABEL, label_value(&input_index.to_string())),
                (SANDBOX_MANAGED_LABEL, "true".to_string()),
                (SANDBOX_PHASE_LABEL, phase.to_string()),
                (SANDBOX_PROVIDER_LABEL, "openshell".to_string()),
                (SANDBOX_RUN_LABEL, label_value(run_id)),
                (SANDBOX_VERSION_LABEL, SANDBOX_RESOURCE_VERSION.to_string()),
                (POLICY_DIGEST_FIRST_LABEL, digest[..32].to_string()),
                (POLICY_DIGEST_SECOND_LABEL, digest[32..].to_string()),
            ]
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect();
            Self {
                labels,
                name: format!("xylem-{phase}-{}", &name_hash[..24]),
            }
        })
    }
    fn selector(&self) -> String {
        self.labels
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}
impl OpenShellPolicy {
    fn new(phase: SandboxResourcePhase, network: OpenShellNetworkAccess) -> Result<Self, SandboxError> {
        let (read_only, read_write) = match phase {
            | SandboxResourcePhase::Execution => (
                SYSTEM_READ_ONLY_PATHS
                    .into_iter()
                    .chain(["/xylem/input", "/xylem/modules", "/xylem/prepared"])
                    .map(str::to_string)
                    .collect(),
                ["/xylem/output", "/xylem/tmp", "/tmp"].into_iter().map(str::to_string).collect(),
            ),
            | SandboxResourcePhase::Preparation => (
                SYSTEM_READ_ONLY_PATHS.into_iter().map(str::to_string).collect(),
                ["/xylem/input", "/xylem/modules", "/xylem/prepared", "/xylem/tmp", "/tmp"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            ),
        };
        network_policies(phase, network).map(|network_policies| Self {
            filesystem_policy: OpenShellFilesystemPolicy {
                include_workdir: false,
                read_only,
                read_write,
            },
            landlock: OpenShellLandlockPolicy {
                compatibility: "hard_requirement",
            },
            network_policies,
            process: OpenShellProcessPolicy {
                run_as_group: "65532".to_string(),
                run_as_user: "65532".to_string(),
            },
            version: 1,
        })
    }
    fn write(path: &Path, phase: SandboxResourcePhase, network: OpenShellNetworkAccess) -> Result<(), SandboxError> {
        Self::new(phase, network).and_then(|policy| policy.yaml()).and_then(|policy| {
            std::fs::write(path, policy).map_err(|why| SandboxError::Runtime(format!("write OpenShell policy {}: {why}", path.display())))
        })
    }
    fn yaml(&self) -> Result<String, SandboxError> {
        serde_norway::to_string(self).map_err(|why| SandboxError::Runtime(format!("serialize OpenShell policy: {why}")))
    }
}
impl OpenShellSandbox {
    fn identity(&self, policy: &EffectiveSandboxPolicy) -> Result<(OpenShellIdentity, u64, String, usize, SandboxResourcePhase), SandboxError> {
        let value = |name: &str| {
            self.labels
                .get(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| SandboxError::Runtime(format!("managed OpenShell sandbox {} is missing label {name}", self.name)))
        };
        value(SANDBOX_MANAGED_LABEL)
            .and_then(|managed| match managed == "true" {
                | true => Ok(()),
                | false => Err(SandboxError::Runtime(format!(
                    "managed OpenShell sandbox {} has invalid ownership label",
                    self.name
                ))),
            })
            .and_then(|()| value(SANDBOX_PROVIDER_LABEL))
            .and_then(|provider| match provider == "openshell" {
                | true => Ok(()),
                | false => Err(SandboxError::Runtime(format!(
                    "managed OpenShell sandbox {} has invalid provider label",
                    self.name
                ))),
            })
            .and_then(|()| value(SANDBOX_VERSION_LABEL))
            .and_then(|version| match version == &SANDBOX_RESOURCE_VERSION.to_string() {
                | true => Ok(()),
                | false => Err(SandboxError::Runtime(format!(
                    "managed OpenShell sandbox {} has invalid version label",
                    self.name
                ))),
            })
            .and_then(|()| value(SANDBOX_RUN_LABEL).map(|run_id| run_id.to_string()))
            .and_then(|run_id| {
                value(SANDBOX_INPUT_LABEL)
                    .and_then(|input| {
                        input
                            .parse::<usize>()
                            .map_err(|why| SandboxError::Runtime(format!("managed OpenShell sandbox {} has invalid input label: {why}", self.name)))
                    })
                    .map(|input_index| (run_id, input_index))
            })
            .and_then(|(run_id, input_index)| {
                value(SANDBOX_CREATED_LABEL)
                    .and_then(|created| {
                        created.parse::<u64>().map_err(|why| {
                            SandboxError::Runtime(format!("managed OpenShell sandbox {} has invalid creation label: {why}", self.name))
                        })
                    })
                    .map(|created_at| (run_id, input_index, created_at))
            })
            .and_then(|(run_id, input_index, created_at)| {
                value(SANDBOX_PHASE_LABEL)
                    .and_then(|phase| match phase.as_str() {
                        | "execution" => Ok(SandboxResourcePhase::Execution),
                        | "preparation" => Ok(SandboxResourcePhase::Preparation),
                        | _ => Err(SandboxError::Runtime(format!(
                            "managed OpenShell sandbox {} has invalid phase label",
                            self.name
                        ))),
                    })
                    .map(|phase| (run_id, input_index, created_at, phase))
            })
            .and_then(|(run_id, input_index, created_at, phase)| {
                OpenShellIdentity::new(&run_id, input_index, phase, &policy.digest, created_at)
                    .and_then(|identity| validate_owned_sandbox(&identity, self.clone()).map(|_| (identity, created_at, run_id, input_index, phase)))
            })
    }
    fn parse(bytes: &[u8]) -> Result<Self, SandboxError> {
        serde_json::from_slice(bytes)
            .or_else(|_| serde_json::from_slice::<OpenShellSandboxEnvelope>(bytes).map(|envelope| envelope.sandbox))
            .map_err(|why| SandboxError::Runtime(format!("parse OpenShell sandbox: {why}")))
    }
}
impl OpenShellSandboxCollection {
    fn parse(bytes: &[u8]) -> Result<Vec<OpenShellSandbox>, SandboxError> {
        serde_json::from_slice::<Self>(bytes)
            .map(Self::into_items)
            .map_err(|why| SandboxError::Runtime(format!("parse OpenShell sandbox list: {why}")))
    }
    fn into_items(self) -> Vec<OpenShellSandbox> {
        match self {
            | Self::Items(sandboxes) | Self::Page { sandboxes } => sandboxes,
        }
    }
}
impl Validate for OpenShellVersion<'_> {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self.0.major == 0 && self.0.minor == 1 {
            | true => Ok(()),
            | false => Err(ValidationReport::from_error(
                "version",
                ValidationError::new("supported").with_message(format!("OpenShell {} is unsupported; qualification requires >=0.1.0,<0.2.0", self.0)),
            )),
        }
    }
}
impl OpenShellSemanticVersion for SemanticVersion {
    fn parse(value: &str) -> Result<Self, SandboxError> {
        let version = value
            .split_ascii_whitespace()
            .find(|part| part.trim_start_matches('v').bytes().next().is_some_and(|byte| byte.is_ascii_digit()))
            .map(|part| part.trim_start_matches('v'))
            .ok_or_else(|| SandboxError::Runtime("OpenShell CLI did not report a semantic version".to_string()));
        version.and_then(|version| match version.contains('-') {
            | true => Err(SandboxError::Runtime(format!(
                "OpenShell CLI reported unsupported prerelease version {version}"
            ))),
            | false => {
                let core = version.split_once('+').map_or(version, |(core, _)| core);
                match core.split('.').count() == 3 {
                    | true => Self::parse_exact(core).map_err(|why| SandboxError::Runtime(format!("parse OpenShell CLI version: {why}"))),
                    | false => Err(SandboxError::Runtime(format!("OpenShell CLI reported invalid version {version}"))),
                }
            }
        })
    }
}
fn is_pinned_digest(value: &str) -> bool {
    value
        .rsplit_once("@sha256:")
        .is_some_and(|(name, digest)| !name.is_empty() && digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn label_value(value: &str) -> String {
    let valid = !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && value.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric);
    match valid {
        | true => value.to_string(),
        | false => format!("h-{:x}", Sha256::digest(value.as_bytes()))[..63].to_string(),
    }
}
fn network_policies(phase: SandboxResourcePhase, access: OpenShellNetworkAccess) -> Result<BTreeMap<String, OpenShellNetworkPolicy>, SandboxError> {
    let endpoints = match access {
        | OpenShellNetworkAccess::Deny => return Ok(BTreeMap::new()),
        | OpenShellNetworkAccess::ExactEndpoints(endpoints) => endpoints
            .into_iter()
            .map(|endpoint| {
                normalize_exact_host(&endpoint.host).and_then(|host| match endpoint.port {
                    | 0 => Err(SandboxError::Validation(
                        "OpenShell network endpoint port must be greater than zero".to_string(),
                    )),
                    | port => Ok(OpenShellNetworkEndpoint { host, port }),
                })
            })
            .collect::<Result<BTreeSet<_>, _>>(),
    };
    endpoints.and_then(|endpoints| match (phase, endpoints.is_empty()) {
        | (SandboxResourcePhase::Execution, _) => Err(SandboxError::Validation(
            "OpenShell execution policy must deny all network access".to_string(),
        )),
        | (SandboxResourcePhase::Preparation, true) => Err(SandboxError::Validation(
            "OpenShell exact-host preparation policy requires at least one host".to_string(),
        )),
        | (SandboxResourcePhase::Preparation, false) => Ok([(
            PREPARATION_NETWORK_POLICY.to_string(),
            OpenShellNetworkPolicy {
                binaries: [
                    "/usr/bin/curl",
                    "/usr/bin/git",
                    "/usr/lib/git-core/git-remote-http",
                    "/usr/lib/git-core/git-remote-https",
                ]
                .into_iter()
                .map(|path| OpenShellNetworkBinary { path: path.to_string() })
                .collect(),
                endpoints: endpoints.into_iter().collect(),
                name: PREPARATION_NETWORK_POLICY.to_string(),
            },
        )]
        .into_iter()
        .collect()),
    })
}
fn normalize_exact_host(host: &str) -> Result<String, SandboxError> {
    let host = host.to_ascii_lowercase();
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                && label.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
                && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    match valid {
        | true => Ok(host),
        | false => Err(SandboxError::Validation(format!(
            "OpenShell preparation endpoint {host} must be an exact DNS hostname"
        ))),
    }
}
fn policy_digest_hex(digest: &str) -> Result<&str, SandboxError> {
    digest
        .strip_prefix("sha256:")
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| SandboxError::Validation("OpenShell policy identity requires a complete sha256 digest".to_string()))
}
fn server_is_local(server: &str) -> bool {
    url::Url::parse(server).is_ok_and(|server| match server.scheme() {
        | "unix" => true,
        | "http" | "https" => server.host().is_some_and(|host| match host {
            | url::Host::Domain(host) => host.eq_ignore_ascii_case("localhost"),
            | url::Host::Ipv4(address) => address.is_loopback(),
            | url::Host::Ipv6(address) => address.is_loopback(),
        }),
        | _ => false,
    })
}
fn validate_gateway(gateway: &OpenShellGatewayInfo, expected_driver: OpenShellDriver) -> Result<(), SandboxError> {
    let authenticated = gateway.auth.as_deref().is_some_and(|value| value.eq_ignore_ascii_case("bearer"));
    let active = gateway.status.eq_ignore_ascii_case("healthy");
    let local = server_is_local(&gateway.server);
    match (authenticated, active, local, gateway.compute_drivers.as_slice()) {
        | (false, _, _, _) => Err(SandboxError::Runtime("OpenShell gateway is not authenticated".to_string())),
        | (_, false, _, _) => Err(SandboxError::Runtime(format!("OpenShell gateway is not healthy: {}", gateway.status))),
        | (_, _, false, _) => Err(SandboxError::Runtime(format!("OpenShell gateway is not local: {}", gateway.server))),
        | (_, _, _, [driver]) if driver.name == expected_driver.to_string() && driver.capabilities.driver_name == expected_driver.to_string() => {
            SemanticVersion::parse(&gateway.version)
                .and_then(|version| {
                    OpenShellVersion(&version)
                        .validate()
                        .map_err(|why| SandboxError::Runtime(why.to_string()))
                })
                .and_then(|()| SemanticVersion::parse(&driver.capabilities.driver_version))
                .and_then(|version| {
                    OpenShellVersion(&version)
                        .validate()
                        .map_err(|why| SandboxError::Runtime(why.to_string()))
                })
        }
        | (_, _, _, drivers) => Err(SandboxError::Runtime(format!(
            "OpenShell gateway must expose exactly one {expected_driver} compute driver, found {}",
            drivers.len()
        ))),
    }
}
fn validate_owned_sandbox(identity: &OpenShellIdentity, sandbox: OpenShellSandbox) -> Result<OpenShellSandbox, SandboxError> {
    match identity.labels.iter().find(|(name, value)| sandbox.labels.get(*name) != Some(*value)) {
        | None => Ok(sandbox),
        | Some((name, _)) => Err(SandboxError::Runtime(format!(
            "OpenShell sandbox {} does not have the expected ownership label {name}",
            sandbox.name
        ))),
    }
}
#[cfg(test)]
mod tests;
