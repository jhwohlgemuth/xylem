//! Local Docker Sandbox provider through the standalone `sbx` CLI.
extern crate alloc;

mod provider;

use super::{workload_is_pinned, SandboxError, SandboxProviderClient, SandboxResourcePhase, SandboxResources, WorkloadAcquisition};
use acorn_cmd::{args, CommandError, ExecutionOptions, ExecutionResult};
use acorn_core::util::SemanticVersion;
use acorn_core::validation::ValidationError;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::BTreeMap;
use core::time::Duration;
pub(super) use provider::DockerSandboxProvider;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::Path;

const DOCKER_SANDBOX_GROUP: &str = "1000";
const DOCKER_SANDBOX_NAME_PREFIX: &str = "xy1";
const DOCKER_SANDBOX_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const DOCKER_SANDBOX_USER: &str = "1000";
const POLICY_FINGERPRINT_LENGTH: usize = 12;

trait DockerSandboxSemanticVersion {
    fn parse(value: &str) -> Result<Self, SandboxError>
    where
        Self: Sized;
}
#[derive(Clone, Debug)]
struct DockerSandboxClient {
    execution_timeout: Duration,
}
struct DockerSandboxCreateRequest<'a> {
    acquisition: WorkloadAcquisition,
    identity: &'a DockerSandboxIdentity,
    network_deny: bool,
    resources: &'a SandboxResources,
    workload: &'a str,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct DockerSandboxIdentity {
    created_at: u64,
    input_index: usize,
    name: String,
    phase: SandboxResourcePhase,
    policy_fingerprint: String,
    run_id: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct DockerSandboxInstance {
    #[serde(alias = "sandbox_name")]
    name: String,
    #[serde(default)]
    status: String,
}
#[derive(Clone, Copy, Debug)]
struct DockerSandboxVersion<'a>(&'a SemanticVersion);
impl DockerSandboxClient {
    fn new() -> Self {
        Self {
            execution_timeout: Self::EXECUTION_TIMEOUT,
        }
    }
    fn bootstrap(&self, sandbox: &str) -> Result<(), SandboxError> {
        self.execute_root_command(
            sandbox,
            "mkdir -p /xylem/input /xylem/output /xylem/prepared /xylem/tmp && chown -R 1000:1000 /xylem && chmod 0700 /xylem/tmp",
        )
    }
    fn create_sandbox(&self, request: &DockerSandboxCreateRequest<'_>) -> Result<DockerSandboxInstance, SandboxError> {
        match workload_is_pinned(request.workload) {
            | false => Err(SandboxError::Validation(
                "Docker Sandbox workloads must use a complete sha256 digest".to_string(),
            )),
            | true => SandboxProviderClient::create(self, request.arguments(), "create Docker sandbox")
                .and_then(|_| self.list_sandboxes())
                .and_then(|sandboxes| {
                    sandboxes
                        .into_iter()
                        .find(|sandbox| sandbox.name == request.identity.name)
                        .ok_or_else(|| SandboxError::Runtime("Docker Sandbox create did not return the requested sandbox".to_string()))
                })
                .and_then(|sandbox| self.bootstrap(&sandbox.name).map(|()| sandbox)),
        }
    }
    fn delete_sandbox_and_wait(&self, identity: &DockerSandboxIdentity) -> Result<(), SandboxError> {
        SandboxProviderClient::delete_and_wait(
            self,
            args!["rm", "--force", &identity.name],
            "remove Docker sandbox",
            SandboxError::Runtime(format!(
                "Docker sandbox {} was not deleted within {} seconds",
                identity.name,
                Self::DELETION_TIMEOUT.as_secs()
            )),
            || {
                self.list_sandboxes()
                    .map(|sandboxes| sandboxes.iter().all(|sandbox| sandbox.name != identity.name).then_some(()))
            },
        )
    }
    fn download_file(&self, sandbox: &str, remote: &str, local: &Path) -> Result<(), SandboxError> {
        SandboxProviderClient::download(self, args!["cp", format!("{sandbox}:{remote}"), local], "copy from Docker sandbox")
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
            "Docker Sandbox command",
        )
    }
    fn execute_root_command(&self, sandbox: &str, command: &str) -> Result<(), SandboxError> {
        SandboxProviderClient::execute_checked_output(
            self,
            args![
                "exec",
                ("--user", "root"),
                ("--workdir", "/"),
                sandbox,
                "env",
                "-i",
                format!("PATH={DOCKER_SANDBOX_PATH}"),
                "--",
                "sh",
                "-c",
                command
            ],
            "bootstrap Docker sandbox",
        )
    }
    fn execution_arguments(&self, sandbox: &str, workdir: &str, environment: &BTreeMap<String, String>, command: &[String]) -> Vec<OsString> {
        let environment = environment.iter().map(|(name, value)| format!("{name}={value}"));
        args![
            "exec",
            ("--user", format!("{DOCKER_SANDBOX_USER}:{DOCKER_SANDBOX_GROUP}")),
            ("--workdir", workdir),
            sandbox,
            "env",
            "-i",
            format!("PATH={DOCKER_SANDBOX_PATH}"),
            "HOME=/xylem/tmp",
            "TMPDIR=/xylem/tmp",
            ..environment,
            "--",
            ..command.iter()
        ]
    }
    fn freeze_execution_filesystem(&self, sandbox: &str) -> Result<(), SandboxError> {
        self.execute_root_command(
            sandbox,
            "chown -R 1000:1000 /xylem && chmod -R a-w /xylem/input /xylem/prepared && chmod -R u+rwX,go-rwx /xylem/output /xylem/tmp",
        )
    }
    fn inspect_provider(&self) -> Result<(SemanticVersion, SemanticVersion), SandboxError> {
        self.checked_output(args!["version", "--json"], "inspect Docker Sandbox version")
            .and_then(|output| parse_versions(&output.stdout))
            .and_then(|(client, server)| {
                [DockerSandboxVersion(&client), DockerSandboxVersion(&server)]
                    .into_iter()
                    .try_for_each(|version| version.validate().map_err(|why| SandboxError::Runtime(why.to_string())))
                    .map(|()| (client, server))
            })
            .and_then(|versions| self.list_sandboxes().map(|_| versions))
    }
    fn list_sandboxes(&self) -> Result<Vec<DockerSandboxInstance>, SandboxError> {
        SandboxProviderClient::list(self, args!["ls", "--json"], "list Docker sandboxes").and_then(|output| parse_sandboxes(&output.stdout))
    }
    fn upload_file(&self, sandbox: &str, local: &Path, remote: &str) -> Result<(), SandboxError> {
        SandboxProviderClient::upload(self, args!["cp", local, format!("{sandbox}:{remote}")], "copy to Docker sandbox")
    }
}
impl SandboxProviderClient for DockerSandboxClient {
    const DELETION_TIMEOUT: Duration = Duration::from_secs(60);
    const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
    const MANAGEMENT_TIMEOUT: Duration = Duration::from_secs(120);

    fn discover(&self) -> Result<(), SandboxError> {
        self.inspect_provider().map(|_| ())
    }
    fn execution_timeout(&self) -> Duration {
        self.execution_timeout
    }
    fn program(&self) -> &'static str {
        "sbx"
    }
}
impl DockerSandboxCreateRequest<'_> {
    fn arguments(&self) -> Vec<OsString> {
        let cpus = self.resources.cpu_millis / 1000;
        let memory = format!("{}m", self.resources.memory_bytes / (1024 * 1024));
        let pull = match self.acquisition {
            | WorkloadAcquisition::LocalOnly => "never",
            | WorkloadAcquisition::PullIfMissing => "missing",
        };
        let network = self.network_deny.then(|| args!["--deny-network", "**"]).into_iter().flatten();
        args![
            "create",
            ("--name", &self.identity.name),
            ("--template", self.workload),
            ("--cpus", cpus.to_string()),
            ("--memory", memory),
            ("--pull", pull),
            ("--skills", "off"),
            ..network,
            "shell"
        ]
    }
}
impl DockerSandboxIdentity {
    fn new(run_id: &str, input_index: usize, phase: SandboxResourcePhase, policy_digest: &str, created_at: u64) -> Result<Self, SandboxError> {
        policy_fingerprint(policy_digest).and_then(|policy_fingerprint| {
            let run = encode_hex(run_id.as_bytes());
            let phase_name = match phase {
                | SandboxResourcePhase::Execution => "e",
                | SandboxResourcePhase::Preparation => "p",
            };
            let name = format!(
                "{DOCKER_SANDBOX_NAME_PREFIX}-{run}-{}-{phase_name}-{}-{policy_fingerprint}",
                encode_base36(input_index as u64),
                encode_base36(created_at)
            );
            let valid = !run_id.is_empty() && u32::try_from(input_index).is_ok() && name.len() <= 63;
            match valid {
                | false => Err(SandboxError::Validation(
                    "Docker Sandbox run identity cannot be represented in a 63-character sandbox name".to_string(),
                )),
                | true => Ok(Self {
                    created_at,
                    input_index,
                    name,
                    phase,
                    policy_fingerprint,
                    run_id: run_id.to_string(),
                }),
            }
        })
    }
    fn parse(name: &str) -> Result<Self, SandboxError> {
        let parts = name.split('-').collect::<Vec<_>>();
        match parts.as_slice() {
            | [prefix, run, input, phase, created, policy_fingerprint]
                if *prefix == DOCKER_SANDBOX_NAME_PREFIX && policy_fingerprint.len() == POLICY_FINGERPRINT_LENGTH =>
            {
                decode_hex(run)
                    .and_then(|run| {
                        String::from_utf8(run)
                            .map_err(|why| SandboxError::Runtime(format!("Docker Sandbox name {name} has an invalid run identifier: {why}")))
                    })
                    .and_then(|run_id| {
                        decode_base36(input)
                            .and_then(|input_index| {
                                usize::try_from(input_index)
                                    .map_err(|why| SandboxError::Runtime(format!("Docker Sandbox name {name} has an invalid input index: {why}")))
                            })
                            .map(|input_index| (run_id, input_index))
                    })
                    .and_then(|(run_id, input_index)| {
                        let phase = match *phase {
                            | "e" => Ok(SandboxResourcePhase::Execution),
                            | "p" => Ok(SandboxResourcePhase::Preparation),
                            | _ => Err(SandboxError::Runtime(format!("Docker Sandbox name {name} has an invalid phase"))),
                        };
                        phase.map(|phase| (run_id, input_index, phase))
                    })
                    .and_then(|(run_id, input_index, phase)| {
                        decode_base36(created).and_then(|created_at| {
                            let canonical = !run_id.is_empty()
                                && u32::try_from(input_index).is_ok()
                                && *run == encode_hex(run_id.as_bytes())
                                && *input == encode_base36(input_index as u64)
                                && *created == encode_base36(created_at)
                                && policy_fingerprint
                                    .bytes()
                                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                                && name.len() <= 63;
                            match canonical {
                                | true => Ok(Self {
                                    created_at,
                                    input_index,
                                    name: name.to_string(),
                                    phase,
                                    policy_fingerprint: (*policy_fingerprint).to_string(),
                                    run_id,
                                }),
                                | false => Err(SandboxError::Runtime(format!(
                                    "Docker Sandbox name {name} is not a canonical Xylem resource identity"
                                ))),
                            }
                        })
                    })
            }
            | _ => Err(SandboxError::Runtime(format!(
                "Docker Sandbox name {name} is not a Xylem resource identity"
            ))),
        }
    }
}
impl Validate for DockerSandboxVersion<'_> {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self.0.major == 0 && (45..48).contains(&self.0.minor) {
            | true => Ok(()),
            | false => Err(ValidationReport::from_error(
                "version",
                ValidationError::new("supported").with_message(format!(
                    "Docker Sandbox {} is unsupported; qualification requires >=0.45.0,<0.48.0",
                    self.0
                )),
            )),
        }
    }
}
impl DockerSandboxSemanticVersion for SemanticVersion {
    fn parse(value: &str) -> Result<Self, SandboxError> {
        let version = value.trim().trim_start_matches('v');
        match version.contains('-') {
            | true => Err(SandboxError::Runtime(format!(
                "Docker Sandbox reported unsupported prerelease version {version}"
            ))),
            | false => {
                let core = version.split_once('+').map_or(version, |(core, _)| core);
                match core.split('.').count() == 3 {
                    | true => Self::parse_exact(core).map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox version: {why}"))),
                    | false => Err(SandboxError::Runtime(format!("Docker Sandbox reported invalid version {version}"))),
                }
            }
        }
    }
}
fn component_version(value: &serde_json::Value, component: &str) -> Option<String> {
    value
        .as_object()
        .and_then(|object| {
            object.iter().find_map(|(name, value)| {
                name.eq_ignore_ascii_case(component).then(|| match value {
                    | serde_json::Value::String(version) => Some(version.clone()),
                    | serde_json::Value::Object(component) => component
                        .iter()
                        .find_map(|(name, value)| name.eq_ignore_ascii_case("version").then(|| value.as_str().map(str::to_string)).flatten()),
                    | _ => None,
                })
            })
        })
        .flatten()
}
fn decode_base36(value: &str) -> Result<u64, SandboxError> {
    u64::from_str_radix(value, 36).map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox identity number {value}: {why}")))
}
fn decode_hex(value: &str) -> Result<Vec<u8>, SandboxError> {
    match value.len() % 2 {
        | 0 => value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                core::str::from_utf8(pair)
                    .map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox identity: {why}")))
                    .and_then(|pair| {
                        u8::from_str_radix(pair, 16).map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox identity: {why}")))
                    })
            })
            .collect(),
        | _ => Err(SandboxError::Runtime(
            "Docker Sandbox identity contains invalid hexadecimal data".to_string(),
        )),
    }
}
fn encode_base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    match value {
        | 0 => "0".to_string(),
        | _ => {
            let mut encoded = Vec::new();
            while value > 0 {
                encoded.push(DIGITS[(value % 36) as usize]);
                value /= 36;
            }
            encoded.reverse();
            String::from_utf8(encoded).unwrap_or_default()
        }
    }
}
fn encode_hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn parse_sandboxes(bytes: &[u8]) -> Result<Vec<DockerSandboxInstance>, SandboxError> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox list: {why}")))
        .and_then(|value| match value {
            | serde_json::Value::Array(values) => Ok(values),
            | serde_json::Value::Object(mut object) => object
                .remove("sandboxes")
                .and_then(|value| value.as_array().cloned())
                .ok_or_else(|| SandboxError::Runtime("Docker Sandbox list JSON omitted sandboxes".to_string())),
            | _ => Err(SandboxError::Runtime("Docker Sandbox list JSON has an invalid shape".to_string())),
        })
        .and_then(|values| {
            values
                .into_iter()
                .map(|value| serde_json::from_value(value).map_err(|why| SandboxError::Runtime(format!("parse Docker sandbox: {why}"))))
                .collect()
        })
}
fn parse_versions(bytes: &[u8]) -> Result<(SemanticVersion, SemanticVersion), SandboxError> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .map_err(|why| SandboxError::Runtime(format!("parse Docker Sandbox version information: {why}")))
        .and_then(|value| {
            ["client", "server"]
                .into_iter()
                .map(|component| {
                    component_version(&value, component)
                        .ok_or_else(|| SandboxError::Runtime(format!("Docker Sandbox version JSON omitted {component} version")))
                        .and_then(|version| SemanticVersion::parse(&version))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .and_then(|versions| match versions.as_slice() {
            | [client, server] => Ok((*client, *server)),
            | _ => Err(SandboxError::Runtime(
                "Docker Sandbox returned incomplete version information".to_string(),
            )),
        })
}
fn policy_fingerprint(digest: &str) -> Result<String, SandboxError> {
    digest
        .strip_prefix("sha256:")
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(|value| value[..POLICY_FINGERPRINT_LENGTH].to_ascii_lowercase())
        .ok_or_else(|| SandboxError::Validation("Docker Sandbox policy identity requires a complete sha256 digest".to_string()))
}

#[cfg(test)]
mod tests;
