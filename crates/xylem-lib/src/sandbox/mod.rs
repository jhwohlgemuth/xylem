//! Opt-in sandbox policy and provider contracts.
extern crate alloc;

mod archive;
mod lifecycle;
mod memory;

use crate::config::{Config, InputAttribute};
use crate::error::ProcessError;
use crate::remote::RemotePolicy;
use crate::script::{Module, ModuleLanguage, ModuleType, ModuleUri, Script, TemplateAttribute};
use acorn_cmd::{args, CommandSpec, ExecutionOptions, ExecutionResult};
use acorn_core::util::MimeType;
use acorn_core::validation::ValidationError;
use acorn_core::Location;
use acorn_schema::hardware::Memory;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::{BTreeSet, VecDeque};
use alloc::sync::Arc;
use core::fmt;
use core::time::Duration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env::current_dir;
use std::ffi::OsString;
use std::fs::{canonicalize, metadata, read_dir, read_to_string, symlink_metadata};
use std::path::{Component, Path, PathBuf};

const MAX_MOUNT_ENTRIES: usize = 100_000;
/// Current operator sandbox policy schema version.
pub const SANDBOX_POLICY_VERSION: u32 = 1;
/// Internal sealing support for the public sandbox traits.
mod private {
    /// Marker implemented only by in-tree provider and session types.
    ///
    /// Requiring this private supertrait prevents downstream implementations from bypassing Xylem's lifecycle and capability validation.
    pub trait Sealed {}
}
pub(super) trait PreparedSandboxBackend: Send + Sync {
    fn execute(&self, script: &Script, module_index: usize, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError>;
    fn finish(&self, succeeded: bool) -> Result<(), SandboxError>;
    fn observed_memory_peak(&self) -> Result<Option<Memory>, SandboxError> {
        Ok(None)
    }
    fn prepared_config(&self) -> &Config;
    fn prepared_input(&self) -> Option<&str>;
    fn resources(&self) -> &SandboxResources;
}
/// Sealed preparation phase for one admitted workflow input.
pub trait SandboxPreparation: private::Sealed + Send {
    /// Verify declared commands inside the preparation workload.
    fn verify_requirements(&mut self) -> Result<(), SandboxError>;
    /// Prepare one required module by its stable configuration index.
    fn prepare_module(&mut self, module_index: usize) -> Result<(), SandboxError>;
    /// Seal prepared content and start the network-disabled execution phase.
    fn seal(self: Box<Self>) -> Result<SandboxInput<Prepared>, SandboxError>;
}
/// Sealed lifecycle provider selected from an effective operator policy.
pub trait SandboxProvider: private::Sealed + Send + Sync {
    /// Discover and validate the provider's realized capabilities.
    fn discover(&self) -> Result<SandboxCapabilities, SandboxError>;
    /// Open one logical run session without executing workflow code.
    fn open(&self, run_id: &str, acquisition: WorkloadAcquisition) -> Result<Arc<dyn SandboxSession>, SandboxError>;
}
/// Sealed logical run session used by concurrent input workers.
pub trait SandboxSession: private::Sealed + Send + Sync {
    /// Begin one isolated input preparation phase.
    fn start_input(&self, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError>;
    /// Finish the logical session after all admitted inputs settle.
    fn finish(&self, succeeded: bool) -> Result<(), SandboxError>;
    /// Return the resource ceilings enforced for each command.
    fn resources(&self) -> &SandboxResources;
}
/// Minimum isolation boundary required by a trusted operator policy, ordered from weakest to strongest.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationClass {
    /// Kernel-enforced process confinement without a distinct container or virtual machine boundary.
    Process,
    /// Container isolation sharing the host kernel.
    SharedKernel,
    /// Virtual-machine isolation with a separate guest kernel.
    SeparateKernel,
}
/// OCI runtime selected by a trusted operator policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OciRuntime {
    /// Docker Engine.
    Docker,
    /// Podman.
    Podman,
}
/// Network access available while preparing workflow dependencies.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationNetwork {
    /// No network access.
    Deny,
    /// Unrestricted provider network access.
    Unrestricted,
}
/// Optional process-level confinement applied inside a provider workload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessGuard {
    /// The [Bubblewrap](https://github.com/containers/bubblewrap) Linux namespace sandbox.
    Bubblewrap,
    /// The [nono](https://nono.sh/) Landlock or Seatbelt process guard.
    Nono,
    /// The [nsjail](https://github.com/google/nsjail) Linux namespace and seccomp sandbox.
    Nsjail,
}
/// Policy parsing, validation, or provider-discovery failure.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Policy contents could not be read.
    #[error("read sandbox policy {path}: {source}", path = .path.display())]
    Read {
        /// Policy path.
        path: PathBuf,
        /// Filesystem failure.
        source: std::io::Error,
    },
    /// Policy contents could not be deserialized.
    #[error("parse sandbox policy {path}: {reason}", path = .path.display())]
    Parse {
        /// Policy path.
        path: PathBuf,
        /// Parser diagnostic.
        reason: String,
    },
    /// Runtime capability discovery failed.
    #[error("sandbox runtime is unavailable: {0}")]
    Runtime(String),
    /// Policy path uses an unsupported extension.
    #[error("sandbox policy {} must use a .json, .jsonc, .yaml, or .yml extension", .0.display())]
    UnsupportedExtension(PathBuf),
    /// Policy contents violate a sandbox invariant.
    #[error("invalid sandbox policy: {0}")]
    Validation(String),
}
/// Observable preparation transition for one required module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxPreparationState {
    /// Module preparation failed.
    Failed,
    /// Module preparation completed.
    Finished,
    /// Module preparation started.
    Started,
}
/// Provider-specific operator configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum SandboxProviderConfiguration {
    /// Native rootless OCI runtime.
    Oci {
        /// Docker or Podman runtime.
        runtime: OciRuntime,
    },
}
/// Supported sandbox lifecycle providers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProviderKind {
    /// Native OCI runtime provider.
    Oci,
}
/// Lifecycle behavior after a sandboxed run fails.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRetention {
    /// Delete resources after every terminal outcome.
    Delete,
    /// Retain resources only after failure.
    RetainOnFailure,
}
/// Network acquisition permitted while opening a sandbox provider session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkloadAcquisition {
    /// Require the immutable workload to exist in local provider storage.
    LocalOnly,
    /// Pull the immutable workload when it is absent from local provider storage.
    PullIfMissing,
}
/// Validated immutable sandbox policy with its canonical identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveSandboxPolicy {
    /// Canonical policy digest.
    pub digest: String,
    /// Validated operator policy.
    pub policy: OperatorSandboxPolicy,
    /// Provider selected by the operator.
    pub provider: SandboxProviderKind,
    /// OCI runtime selected by the operator, when applicable.
    pub runtime: Option<OciRuntime>,
    /// Immutable provider workload identity.
    pub workload: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OciImageConfig {
    #[serde(rename = "Entrypoint", default)]
    entrypoint: Option<Vec<String>>,
    #[serde(rename = "Env", default)]
    environment: Option<Vec<String>>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OciImageInspection {
    #[serde(rename = "Architecture")]
    architecture: String,
    #[serde(rename = "Config")]
    config: OciImageConfig,
    #[serde(rename = "Digest", default)]
    digest: Option<String>,
    #[serde(rename = "Os")]
    operating_system: String,
    #[serde(rename = "RepoDigests", default)]
    repository_digests: Vec<String>,
}
#[derive(Clone, Debug)]
struct OciSandboxSession {
    policy: EffectiveSandboxPolicy,
    run_id: String,
}
/// Strict operator-owned sandbox policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorSandboxPolicy {
    /// Child environment allowlist.
    pub environment: SandboxEnvironment,
    /// Cleanup and reconciliation behavior.
    pub lifecycle: SandboxLifecycle,
    /// Minimum acceptable provider isolation class.
    pub minimum_isolation: IsolationClass,
    /// Host path grants.
    pub paths: SandboxPaths,
    /// Preparation-session network access.
    pub preparation_network: PreparationNetwork,
    /// Process-level confinement layers required inside the provider workload.
    pub process_guards: Vec<ProcessGuard>,
    /// Sandbox lifecycle provider.
    pub provider: SandboxProviderConfiguration,
    /// Per-input resource ceilings.
    pub resources: SandboxResources,
    /// Policy schema version.
    pub version: u32,
    /// Immutable workloads admitted by the operator.
    pub workload: SandboxWorkload,
}
/// Prepared sandbox input state.
#[derive(Clone)]
pub struct Prepared {
    backend: Arc<dyn PreparedSandboxBackend>,
}
/// Requested sandbox input state.
#[derive(Clone, Debug)]
pub struct Requested {
    config: Config,
    input: Option<String>,
    input_index: usize,
    module_indices: Vec<usize>,
    staging_root: PathBuf,
}
/// Controls a sandbox provider can enforce for this implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SandboxCapabilities {
    /// Effective isolation boundary.
    pub isolation: IsolationClass,
    /// Provider can deny execution networking.
    pub network_deny: bool,
    /// Provider kind.
    pub provider: SandboxProviderKind,
    /// Provider can apply CPU, memory, and PID ceilings.
    pub resource_limits: bool,
    /// Provider supports a read-only root filesystem.
    pub read_only_root: bool,
    /// Provider is running without root privileges.
    pub rootless: bool,
}
/// Environment names that may be visible to workflow commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxEnvironment {
    /// Exact child environment names permitted by the operator.
    pub allow: Vec<String>,
}
/// One sandbox input whose generic state makes lifecycle transitions explicit.
#[derive(Clone, Debug)]
pub struct SandboxInput<State> {
    state: State,
}
/// Sandbox cleanup and reconciliation settings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxLifecycle {
    /// Terminal resource-retention behavior.
    pub retention: SandboxRetention,
    /// Age after which abandoned labeled resources may be reconciled.
    pub stale_after_seconds: u64,
}
/// Host roots that may be mapped into a sandbox.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxPaths {
    /// Roots under which final artifact destinations may be created.
    pub artifact_roots: Vec<PathBuf>,
    /// Roots from which declared workflow inputs may be read.
    pub input_roots: Vec<PathBuf>,
    /// Roots from which module sources may be staged.
    pub module_roots: Vec<PathBuf>,
}
/// Positive resource ceilings applied to one sandbox input container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxResources {
    /// Maximum artifact bytes exported by one input.
    pub artifact_bytes: u64,
    /// Maximum artifact entries exported by one input.
    pub artifact_files: u64,
    /// Maximum retained bytes per output stream and command.
    pub captured_output_bytes: u64,
    /// CPU allocation expressed as one-thousandths of a core.
    pub cpu_millis: u32,
    /// Maximum resident memory bytes.
    pub memory_bytes: u64,
    /// Maximum process identifiers.
    pub pids: u32,
    /// Maximum private scratch bytes.
    pub scratch_bytes: u64,
    /// Maximum command wall time in seconds.
    pub wall_time_seconds: u64,
}
/// Immutable provider workload allowlist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxWorkload {
    /// Workloads permitted for sandbox sessions, each pinned by digest.
    pub allowed: Vec<String>,
    /// Default digest-pinned workload.
    pub default: String,
}
impl Config {
    pub(crate) fn validate_sandbox(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if self.output.is_some() {
            report.add(
                "output",
                validation_error("unsupported", "artifact publication is not yet supported in sandbox mode"),
            );
        }
        if self.options.as_ref().and_then(|options| options.gpu_monitor).unwrap_or_default() {
            report.add(
                "options.gpu_monitor",
                validation_error("unsupported", "GPU monitoring is not supported in sandbox mode"),
            );
        }
        self.modules.iter().enumerate().for_each(|(index, module)| {
            let path = format!("modules[{index}]");
            let supported_source = module.sandbox_source().is_ok();
            let supported_type = module.sandbox_type_supported();
            match (&module.module_type, supported_source && supported_type) {
                | (ModuleType::Configuration { .. }, _) => report.add(
                    format!("{path}.type"),
                    validation_error(
                        "unsupported",
                        format!("nested configuration module {} is not supported in sandbox mode", module.name),
                    ),
                ),
                | (_, false) => report.add(
                    format!("{path}.uri"),
                    validation_error(
                        "sandbox",
                        format!("module {} must use a local or HTTP(S) generic, binary, or Python source", module.name),
                    ),
                ),
                | (_, true) => (),
            }
            if module.completion.is_some() {
                report.add(
                    format!("{path}.completion"),
                    validation_error("unsupported", "completion markers are not yet supported in sandbox mode"),
                );
            }
            if matches!(&module.template, Some(TemplateAttribute::Template(template)) if template.container.is_some()) {
                report.add(
                    format!("{path}.template"),
                    validation_error(
                        "unsupported",
                        format!(
                            "module {} uses an application container that is not supported in sandbox mode",
                            module.name
                        ),
                    ),
                );
            }
        });
        report.finish()
    }
}
impl EffectiveSandboxPolicy {
    /// Read, normalize, validate, and hash an operator policy.
    pub fn read(path: &Path) -> Result<Self, SandboxError> {
        read_to_string(path)
            .map_err(|source| SandboxError::Read {
                path: path.to_path_buf(),
                source,
            })
            .and_then(|content| OperatorSandboxPolicy::parse(path, &content))
            .and_then(|policy| policy.normalize(path))
            .and_then(|policy| {
                policy
                    .validate()
                    .map_err(|why| SandboxError::Validation(why.to_string()))
                    .map(|()| policy)
            })
            .and_then(|policy| {
                serde_json::to_vec(&policy)
                    .map_err(|why| SandboxError::Validation(format!("serialize canonical sandbox policy: {why}")))
                    .map(|canonical| {
                        let (provider, runtime) = match policy.provider {
                            | SandboxProviderConfiguration::Oci { runtime } => (SandboxProviderKind::Oci, Some(runtime)),
                        };
                        Self {
                            digest: format!("sha256:{:x}", Sha256::digest(canonical)),
                            workload: policy.workload.default.clone(),
                            policy,
                            provider,
                            runtime,
                        }
                    })
            })
    }
    /// Validate sandbox feature compatibility before resolving nested sources.
    pub fn validate_configuration(&self, config: &Config) -> Result<(), SandboxError> {
        config.validate().map_err(|why| SandboxError::Validation(why.to_string()))
    }
    /// Validate configured discovery roots before input resolution reads them.
    pub fn validate_input_discovery(&self, config: &Config, configuration_path: &Path, cli_inputs: Option<&[String]>) -> Result<(), SandboxError> {
        match cli_inputs.filter(|values| !values.is_empty()) {
            | Some(_) => Ok(()),
            | None => match &config.input {
                | Some(InputAttribute::Path(path)) => {
                    validate_granted_path(Path::new(path), &self.policy.paths.input_roots, "input directory").map(|_| ())
                }
                | Some(InputAttribute::Select(selection)) => {
                    let base = configuration_path.parent().unwrap_or_else(|| Path::new("."));
                    selection.select.options.roots.iter().try_for_each(|root| {
                        let path = Path::new(root);
                        let path = match path.is_absolute() {
                            | true => path.to_path_buf(),
                            | false => base.join(path),
                        };
                        validate_granted_path(&path, &self.policy.paths.input_roots, "input selection root").map(|_| ())
                    })
                }
                | Some(InputAttribute::List(_)) | None => Ok(()),
            },
        }
    }
    /// Reject workflow features that cannot yet cross the OCI boundary safely.
    pub fn validate_workflow(&self, config: &Config, inputs: &[Option<String>]) -> Result<(), SandboxError> {
        config.validate().map_err(|why| SandboxError::Validation(why.to_string())).and_then(|()| {
            RemotePolicy::from_hosts(config.options.as_ref().and_then(|options| options.allowed.as_deref()))
                .map_err(|why| SandboxError::Validation(format!("invalid remote module allowlist: {why}")))
                .and_then(|remote_policy| {
                    config
                        .modules
                        .iter()
                        .map(|module| {
                            module.sandbox_source().and_then(|location| match ModuleUri::from(&location) {
                                | ModuleUri::File(path) => {
                                    validate_granted_path(&path, &self.policy.paths.module_roots, "module").and_then(|module_source| {
                                        validate_template_path(module, &module_source).and_then(|()| {
                                            Script::from_module(module.clone(), None)
                                                .map_err(|why| {
                                                    SandboxError::Validation(format!(
                                                        "module {} cannot be loaded for sandbox execution: {why}",
                                                        module.name
                                                    ))
                                                })
                                                .and_then(|script| script.validate().map_err(|why| SandboxError::Validation(why.to_string())))
                                                .map(|()| Some(module_source))
                                        })
                                    })
                                }
                                | ModuleUri::Url(url) => match (
                                    config.options.as_ref().and_then(|options| options.offline).unwrap_or_default(),
                                    self.policy.preparation_network,
                                ) {
                                    | (true, _) => Err(SandboxError::Validation(format!(
                                        "module {} remote source is unavailable in offline sandbox mode",
                                        module.name
                                    ))),
                                    | (_, PreparationNetwork::Deny) => Err(SandboxError::Validation(format!(
                                        "module {} requires preparation networking denied by sandbox policy",
                                        module.name
                                    ))),
                                    | (false, PreparationNetwork::Unrestricted) => {
                                        let url = url.to_string();
                                        remote_policy
                                            .ensure_allowed(&url)
                                            .map(|_| None)
                                            .map_err(|why| SandboxError::Validation(format!("module {} remote source failed: {why}", module.name)))
                                    }
                                },
                                | ModuleUri::Unknown(value) => Err(SandboxError::Validation(format!(
                                    "module {} has invalid sandbox source {value}",
                                    module.name
                                ))),
                            })
                        })
                        .collect::<Result<Vec<Option<PathBuf>>, _>>()
                        .and_then(|modules| {
                            inputs
                                .iter()
                                .flatten()
                                .map(|input| validate_granted_path(Path::new(input), &self.policy.paths.input_roots, "input"))
                                .collect::<Result<Vec<_>, _>>()
                                .and_then(|inputs| {
                                    inputs.iter().try_for_each(|input| {
                                        modules.iter().flatten().try_for_each(|module| validate_mount_separation(module, input))
                                    })
                                })
                        })
                })
        })
    }
    /// Perform terminal cleanup for a named sandbox.
    fn cleanup(&self, container_name: &str, succeeded: bool) -> Result<(), SandboxError> {
        match succeeded || self.policy.lifecycle.retention == SandboxRetention::Delete {
            | true => self
                .runtime()
                .and_then(|runtime| {
                    CommandSpec::new(runtime.to_string())
                        .args(args!["rm", "--force", container_name])
                        .checked_output_with_timeout(Duration::from_secs(10))
                        .map_err(|why| SandboxError::Runtime(format!("remove sandbox {container_name}: {why}")))
                })
                .map(|_| ()),
            | false => Ok(()),
        }
    }
    fn inspect_workload(&self) -> Result<Option<OciImageInspection>, SandboxError> {
        self.runtime()
            .and_then(|runtime| {
                CommandSpec::new(runtime.to_string())
                    .args(args!["image", "inspect", &self.workload])
                    .output_with_timeout(Duration::from_secs(self.policy.resources.wall_time_seconds))
                    .map_err(|why| SandboxError::Runtime(format!("inspect pinned workload {}: {why}", self.workload)))
            })
            .and_then(|output| match output.status.success() {
                | true => serde_json::from_slice::<Vec<OciImageInspection>>(&output.stdout)
                    .map_err(|why| SandboxError::Runtime(format!("parse inspection for pinned workload {}: {why}", self.workload)))
                    .and_then(|inspections| match inspections.as_slice() {
                        | [inspection] => Ok(Some(inspection.clone())),
                        | values => Err(SandboxError::Runtime(format!(
                            "inspect pinned workload {} returned {} images",
                            self.workload,
                            values.len()
                        ))),
                    }),
                | false => Ok(None),
            })
    }
    fn prepare_workload(&self, run_id: &str, acquisition: WorkloadAcquisition) -> Result<(), SandboxError> {
        acquire_workload(&self.workload, acquisition, self.inspect_workload(), || {
            self.pull_workload().and_then(|()| self.inspect_workload())
        })
        .and_then(|inspection| self.verify_workload(&inspection))
        .and_then(|()| self.probe_workload(run_id))
    }
    fn probe_invocation(&self, run_id: &str) -> Vec<OsString> {
        let resources = &self.policy.resources;
        let cpus = format!("{:.3}", f64::from(resources.cpu_millis) / 1000.0);
        let container_name = format!("xylem-{run_id}-probe");
        args![
            "run",
            "--rm",
            "--init",
            ("--name", container_name),
            ("--label", "gov.ornl.xylem.sandbox=true"),
            ("--label", format!("gov.ornl.xylem.policy={}", self.digest)),
            "--pull=never",
            "--network=none",
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--ipc=none",
            ("--pids-limit", resources.pids.to_string()),
            ("--memory", resources.memory_bytes.to_string()),
            ("--memory-swap", resources.memory_bytes.to_string()),
            ("--cpus", cpus),
            "--user=65532:65532",
            ("--tmpfs", format!("/tmp:rw,noexec,nosuid,nodev,size={}", resources.scratch_bytes)),
            ("--entrypoint", "/bin/sh"),
            &self.workload,
            "-c",
            "test \"$(id -u)\" = 65532 && test \"$(id -g)\" = 65532 && command -v sh >/dev/null && command -v cp >/dev/null && command -v curl >/dev/null && command -v git >/dev/null && command -v mkdir >/dev/null && command -v python >/dev/null && command -v rm >/dev/null && command -v tar >/dev/null && test -w /tmp"
        ]
    }
    fn probe_workload(&self, run_id: &str) -> Result<(), SandboxError> {
        self.runtime()
            .and_then(|runtime| {
                CommandSpec::new(runtime.to_string())
                    .args(self.probe_invocation(run_id))
                    .checked_output_with_timeout(Duration::from_secs(self.policy.resources.wall_time_seconds))
                    .map_err(|why| SandboxError::Runtime(format!("probe pinned workload {}: {why}", self.workload)))
            })
            .map(|_| ())
    }
    fn pull_workload(&self) -> Result<(), SandboxError> {
        self.runtime()
            .and_then(|runtime| {
                CommandSpec::new(runtime.to_string())
                    .args(args!["pull", &self.workload])
                    .checked_output_with_timeout(Duration::from_secs(self.policy.resources.wall_time_seconds))
                    .map_err(|why| SandboxError::Runtime(format!("pull pinned workload {}: {why}", self.workload)))
            })
            .map(|_| ())
    }
    fn runtime(&self) -> Result<OciRuntime, SandboxError> {
        self.runtime
            .ok_or_else(|| SandboxError::Validation("OCI sandbox policy does not select an OCI runtime".to_string()))
    }
    fn verify_workload(&self, inspection: &OciImageInspection) -> Result<(), SandboxError> {
        let expected_digest = self.workload.rsplit_once('@').map_or(self.workload.as_str(), |(_, digest)| digest);
        let digest_matches = inspection.digest.as_deref() == Some(expected_digest)
            || inspection
                .repository_digests
                .iter()
                .filter_map(|digest| digest.rsplit_once('@').map(|(_, digest)| digest))
                .any(|digest| digest == expected_digest);
        let invalid_environment = inspection
            .config
            .environment
            .iter()
            .flatten()
            .map(|value| image_environment(value))
            .find(|name| !self.policy.environment.allow.iter().any(|allowed| allowed == name));
        match digest_matches {
            | false => Err(SandboxError::Validation(format!(
                "pinned workload {} realized an unexpected digest",
                self.workload
            ))),
            | true => Ok(()),
        }
        .and_then(|()| match inspection.operating_system.as_str() {
            | "linux" => Ok(()),
            | operating_system => Err(SandboxError::Validation(format!(
                "pinned workload {} targets unsupported operating system {operating_system}",
                self.workload
            ))),
        })
        .and_then(|()| match inspection.architecture.as_str() {
            | "amd64" => Ok(()),
            | architecture => Err(SandboxError::Validation(format!(
                "pinned workload {} targets unsupported architecture {architecture}",
                self.workload
            ))),
        })
        .and_then(|()| match inspection.config.entrypoint.as_ref().is_none_or(Vec::is_empty) {
            | true => Ok(()),
            | false => Err(SandboxError::Validation(format!(
                "pinned workload {} must not declare an entrypoint",
                self.workload
            ))),
        })
        .and_then(|()| match invalid_environment {
            | None => Ok(()),
            | Some(name) => Err(SandboxError::Validation(format!(
                "pinned workload {} declares environment variable {name} outside the policy allowlist",
                self.workload
            ))),
        })
    }
}
impl private::Sealed for EffectiveSandboxPolicy {}
impl SandboxProvider for EffectiveSandboxPolicy {
    fn discover(&self) -> Result<SandboxCapabilities, SandboxError> {
        self.runtime()
            .and_then(OciRuntime::discover)
            .map(|()| SandboxCapabilities {
                isolation: IsolationClass::SharedKernel,
                network_deny: true,
                provider: SandboxProviderKind::Oci,
                read_only_root: true,
                resource_limits: true,
                rootless: true,
            })
            .and_then(|capabilities| match capabilities.isolation >= self.policy.minimum_isolation {
                | true => Ok(capabilities),
                | false => Err(SandboxError::Runtime(format!(
                    "provider isolation {:?} does not satisfy required {:?}",
                    capabilities.isolation, self.policy.minimum_isolation
                ))),
            })
    }
    fn open(&self, run_id: &str, acquisition: WorkloadAcquisition) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        match run_id.is_empty() {
            | true => Err(SandboxError::Validation("sandbox run identifier must not be empty".to_string())),
            | false => self.prepare_workload(run_id, acquisition).map(|()| {
                Arc::new(OciSandboxSession {
                    policy: self.clone(),
                    run_id: run_id.to_string(),
                }) as Arc<dyn SandboxSession>
            }),
        }
    }
}
impl Module {
    fn sandbox_source(&self) -> Result<Location, SandboxError> {
        let location = match &self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri } | ModuleType::Script { uri, .. } => uri,
        };
        match location.is_local() || location.is_http_or_https() {
            | true => Ok(location.clone()),
            | false => Err(SandboxError::Validation(format!(
                "module {} must use a local or HTTP(S) source",
                self.name
            ))),
        }
    }
    fn sandbox_type_supported(&self) -> bool {
        matches!(
            self.module_type,
            ModuleType::Binary { .. }
                | ModuleType::Script {
                    programming_language: Some(ModuleLanguage::Generic | ModuleLanguage::Python) | None,
                    ..
                }
        )
    }
}
impl fmt::Display for OciRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Docker => formatter.write_str("docker"),
            | Self::Podman => formatter.write_str("podman"),
        }
    }
}
impl OciRuntime {
    fn discover(self) -> Result<(), SandboxError> {
        match (cfg!(target_os = "linux"), cfg!(target_arch = "x86_64")) {
            | (false, _) => Err(SandboxError::Runtime("the qualified OCI sandbox requires native Linux".to_string())),
            | (true, false) => Err(SandboxError::Runtime(
                "the qualified OCI sandbox currently requires x86_64 Linux".to_string(),
            )),
            | (true, true) => {
                let arguments = match self {
                    | Self::Docker => args!["info", "--format", "{{json .SecurityOptions}}"],
                    | Self::Podman => args!["info", "--format", "{{.Host.Security.Rootless}}"],
                };
                CommandSpec::new(self.to_string())
                    .args(arguments)
                    .checked_output_with_timeout(Duration::from_secs(10))
                    .map_err(|why| SandboxError::Runtime(format!("inspect {self}: {why}")))
                    .and_then(|output| {
                        let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                        match (self, value.contains("rootless") || value.trim() == "true") {
                            | (_, true) => Ok(()),
                            | (Self::Docker, false) => Err(SandboxError::Runtime("Docker does not report rootless security mode".to_string())),
                            | (Self::Podman, false) => Err(SandboxError::Runtime("Podman is not rootless".to_string())),
                        }
                    })
                    .and_then(|()| self.validate().map_err(|why| SandboxError::Runtime(why.to_string())))
            }
        }
    }
}
impl Validate for OciRuntime {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self {
            | Self::Docker => CommandSpec::new("docker")
                .args(args!["context", "show"])
                .checked_output_with_timeout(Duration::from_secs(10))
                .map_err(|why| validation_report("provider.runtime", "runtime", format!("inspect Docker context: {why}")))
                .and_then(|output| {
                    let context = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    CommandSpec::new("docker")
                        .args(args!["context", "inspect", "--format", "{{json .Endpoints.docker.Host}}", context])
                        .checked_output_with_timeout(Duration::from_secs(10))
                        .map_err(|why| validation_report("provider.runtime", "runtime", format!("inspect Docker endpoint: {why}")))
                })
                .and_then(|output| {
                    serde_json::from_slice::<String>(&output.stdout)
                        .map_err(|why| validation_report("provider.runtime", "runtime", format!("parse Docker endpoint: {why}")))
                })
                .and_then(|endpoint| match endpoint.starts_with("unix://") {
                    | true => Ok(()),
                    | false => Err(validation_report(
                        "provider.runtime",
                        "local",
                        format!("Docker endpoint {endpoint} is remote; the sandbox requires a local Unix socket"),
                    )),
                }),
            | Self::Podman => CommandSpec::new("podman")
                .args(args!["info", "--format", "{{.Host.ServiceIsRemote}}"])
                .checked_output_with_timeout(Duration::from_secs(10))
                .map_err(|why| validation_report("provider.runtime", "runtime", format!("inspect Podman connection: {why}")))
                .and_then(|output| match String::from_utf8_lossy(&output.stdout).trim() {
                    | "false" => Ok(()),
                    | _ => Err(validation_report(
                        "provider.runtime",
                        "local",
                        "Podman is using a remote service; the sandbox requires a local runtime",
                    )),
                }),
        }
    }
}
impl private::Sealed for OciSandboxSession {}
impl SandboxSession for OciSandboxSession {
    fn start_input(&self, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError> {
        lifecycle::start(self, request)
    }
    fn finish(&self, _succeeded: bool) -> Result<(), SandboxError> {
        Ok(())
    }
    fn resources(&self) -> &SandboxResources {
        &self.policy.policy.resources
    }
}
impl OperatorSandboxPolicy {
    fn normalize(mut self, path: &Path) -> Result<Self, SandboxError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let base = match parent.is_absolute() {
            | true => Ok(parent.to_path_buf()),
            | false => current_dir()
                .map(|directory| directory.join(parent))
                .map_err(|why| SandboxError::Validation(format!("resolve sandbox policy directory: {why}"))),
        };
        base.map(|base| {
            self.environment.allow.sort();
            self.paths.artifact_roots = self.paths.artifact_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.input_roots = self.paths.input_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.module_roots = self.paths.module_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.artifact_roots.sort();
            self.paths.input_roots.sort();
            self.paths.module_roots.sort();
            self.process_guards.sort();
            self.workload.allowed.sort();
            self
        })
    }
    fn parse(path: &Path, content: &str) -> Result<Self, SandboxError> {
        match MimeType::from(path.to_string_lossy().as_ref()) {
            | MimeType::Json => serde_json::from_str(content).map_err(|why| SandboxError::Parse {
                path: path.to_path_buf(),
                reason: why.to_string(),
            }),
            | MimeType::Jsonc => jsonc_parser::parse_to_serde_value(content, &jsonc_parser::ParseOptions::default())
                .map_err(|why| SandboxError::Parse {
                    path: path.to_path_buf(),
                    reason: why.to_string(),
                })
                .and_then(|value: Option<serde_json::Value>| {
                    value.ok_or_else(|| SandboxError::Parse {
                        path: path.to_path_buf(),
                        reason: "policy document is empty".to_string(),
                    })
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|why| SandboxError::Parse {
                        path: path.to_path_buf(),
                        reason: why.to_string(),
                    })
                }),
            | MimeType::Yaml => serde_norway::from_str(content).map_err(|why| SandboxError::Parse {
                path: path.to_path_buf(),
                reason: why.to_string(),
            }),
            | _ => Err(SandboxError::UnsupportedExtension(path.to_path_buf())),
        }
    }
}
impl Validate for OperatorSandboxPolicy {
    fn validate(&self) -> Result<(), ValidationReport> {
        let resource_values = [
            self.resources.artifact_bytes,
            self.resources.artifact_files,
            self.resources.captured_output_bytes,
            u64::from(self.resources.cpu_millis),
            self.resources.memory_bytes,
            u64::from(self.resources.pids),
            self.resources.scratch_bytes,
            self.resources.wall_time_seconds,
        ];
        let allowed_workloads = self.workload.allowed.iter().collect::<BTreeSet<_>>();
        let environment = self.environment.allow.iter().collect::<BTreeSet<_>>();
        let artifact_roots = self.paths.artifact_roots.iter().collect::<BTreeSet<_>>();
        let input_roots = self.paths.input_roots.iter().collect::<BTreeSet<_>>();
        let module_roots = self.paths.module_roots.iter().collect::<BTreeSet<_>>();
        let process_guards = self.process_guards.iter().collect::<BTreeSet<_>>();
        let mut report = ValidationReport::new();
        if self.version != SANDBOX_POLICY_VERSION {
            report.add(
                "version",
                validation_error("version", format!("policy version must be {SANDBOX_POLICY_VERSION}")),
            );
        }
        if !workload_is_pinned(&self.workload.default) {
            report.add(
                "workload.default",
                validation_error("digest", "workload.default must use an immutable sha256 digest"),
            );
        }
        if self.workload.allowed.iter().any(|workload| !workload_is_pinned(workload)) {
            report.add(
                "workload.allowed",
                validation_error("digest", "workload.allowed must contain only immutable sha256 digests"),
            );
        }
        if !self.workload.allowed.contains(&self.workload.default) {
            report.add(
                "workload.allowed",
                validation_error("contains", "workload.allowed must contain workload.default"),
            );
        }
        if allowed_workloads.len() != self.workload.allowed.len() {
            report.add(
                "workload.allowed",
                validation_error("unique", "workload.allowed must not contain duplicates"),
            );
        }
        if process_guards.len() != self.process_guards.len() {
            report.add("process_guards", validation_error("unique", "process_guards must not contain duplicates"));
        }
        if !self.process_guards.is_empty() {
            report.add(
                "process_guards",
                validation_error("unsupported", "process guards are reserved for a later provider integration phase"),
            );
        }
        if !paths_are_valid(&self.paths.artifact_roots) || !paths_are_valid(&self.paths.input_roots) || !paths_are_valid(&self.paths.module_roots) {
            report.add(
                "paths",
                validation_error("absolute", "artifact, input, and module roots must be nonempty normalized absolute paths"),
            );
        }
        if artifact_roots.len() != self.paths.artifact_roots.len()
            || input_roots.len() != self.paths.input_roots.len()
            || module_roots.len() != self.paths.module_roots.len()
        {
            report.add("paths", validation_error("unique", "sandbox path roots must not contain duplicates"));
        }
        if resource_values.contains(&0) {
            report.add(
                "resources",
                validation_error("positive", "sandbox resource limits must be greater than zero"),
            );
        }
        if self.lifecycle.stale_after_seconds == 0 {
            report.add(
                "lifecycle.stale_after_seconds",
                validation_error("positive", "lifecycle.stale_after_seconds must be greater than zero"),
            );
        }
        if self.lifecycle.retention != SandboxRetention::Delete {
            report.add(
                "lifecycle.retention",
                validation_error(
                    "unsupported",
                    "retain_on_failure requires durable resource evidence and is not yet supported",
                ),
            );
        }
        if environment.len() != self.environment.allow.len()
            || self.environment.allow.iter().any(|name| {
                name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
            })
        {
            report.add(
                "environment.allow",
                validation_error("identifier", "environment allowlist names must be unique nonempty uppercase identifiers"),
            );
        }
        report.finish()
    }
}
impl SandboxInput<Prepared> {
    pub(super) fn from_backend(backend: Arc<dyn PreparedSandboxBackend>) -> Self {
        Self { state: Prepared { backend } }
    }
    /// Execute one module command inside the prepared input container.
    pub fn execute(&self, script: &Script, module_index: usize, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError> {
        self.state.backend.execute(script, module_index, options)
    }
    /// Finish the input and remove its provider resources.
    pub fn finish(&self, succeeded: bool) -> Result<(), SandboxError> {
        self.state.backend.finish(succeeded)
    }
    /// Return the exact provider-attributed peak memory when the platform exposes it.
    pub fn observed_memory_peak(&self) -> Result<Option<Memory>, SandboxError> {
        self.state.backend.observed_memory_peak()
    }
    /// Return the host-staged configuration used to resolve module templates.
    pub fn prepared_config(&self) -> &Config {
        self.state.backend.prepared_config()
    }
    /// Return the original admitted input path.
    pub fn prepared_input(&self) -> Option<&str> {
        self.state.backend.prepared_input()
    }
    /// Return the resource ceilings enforced for this input.
    pub fn resources(&self) -> &SandboxResources {
        self.state.backend.resources()
    }
}
impl SandboxInput<Requested> {
    /// Build a request from resolved workflow state and deterministically ordered required modules.
    pub fn new(
        config: Config,
        input_index: usize,
        module_indices: impl IntoIterator<Item = usize>,
        staging_root: PathBuf,
        input: Option<String>,
    ) -> Result<Self, SandboxError> {
        let module_indices = module_indices.into_iter().collect::<Vec<_>>();
        let ordered = module_indices.windows(2).all(|pair| pair[0] < pair[1]);
        let indices = module_indices.iter().all(|index| *index < config.modules.len());
        let root = staging_root.as_os_str().is_empty();
        match (ordered, indices, root) {
            | (false, _, _) => Err(SandboxError::Validation(
                "sandbox input module indices must be strictly increasing".to_string(),
            )),
            | (_, false, _) => Err(SandboxError::Validation("sandbox input contains an invalid module index".to_string())),
            | (_, _, true) => Err(SandboxError::Validation("sandbox staging root must not be empty".to_string())),
            | (true, true, false) => Ok(Self {
                state: Requested {
                    config,
                    input,
                    input_index,
                    module_indices,
                    staging_root,
                },
            }),
        }
    }
    /// Prepare every required module and transition this input to its prepared state.
    pub fn prepare(
        self,
        session: &dyn SandboxSession,
        mut observe: impl FnMut(usize, SandboxPreparationState),
    ) -> Result<SandboxInput<Prepared>, SandboxError> {
        let module_indices = self.state.module_indices.clone();
        session.start_input(self).and_then(|mut preparation| {
            preparation
                .verify_requirements()
                .and_then(|()| {
                    module_indices.iter().try_for_each(|module_index| {
                        observe(*module_index, SandboxPreparationState::Started);
                        let result = preparation.prepare_module(*module_index);
                        observe(
                            *module_index,
                            match result.is_ok() {
                                | true => SandboxPreparationState::Finished,
                                | false => SandboxPreparationState::Failed,
                            },
                        );
                        result
                    })
                })
                .and_then(|()| preparation.seal())
        })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.state.config
    }
    pub(crate) fn input(&self) -> Option<&str> {
        self.state.input.as_deref()
    }
    pub(crate) fn input_index(&self) -> usize {
        self.state.input_index
    }
    pub(crate) fn module_indices(&self) -> &[usize] {
        &self.state.module_indices
    }
    pub(crate) fn staging_root(&self) -> &Path {
        &self.state.staging_root
    }
}
impl fmt::Display for SandboxProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Oci => formatter.write_str("oci"),
        }
    }
}
impl dyn SandboxSession {
    /// Open the selected provider session, or return host mode when no policy is active.
    pub fn open(
        run_id: &str,
        policy: Option<&EffectiveSandboxPolicy>,
        acquisition: WorkloadAcquisition,
    ) -> Result<Option<Arc<dyn SandboxSession>>, SandboxError> {
        policy.map_or(Ok(None), |policy| SandboxProvider::open(policy, run_id, acquisition).map(Some))
    }
}
impl Validate for Script {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if self.command.container.is_some() {
            report.add(
                "command.container",
                validation_error("unsupported", "application container declarations are not supported inside sandbox mode"),
            );
        }
        let unsupported_environment = self
            .command
            .virtual_environment
            .as_ref()
            .is_some_and(|environment| !environment.manager.supports_sandbox());
        if unsupported_environment {
            report.add(
                "command.virtual_environment.manager",
                validation_error("unsupported", "sandbox mode supports conda, mamba, and pixi environments"),
            );
        }
        report.finish()
    }
}
fn acquire_workload(
    workload: &str,
    acquisition: WorkloadAcquisition,
    local: Result<Option<OciImageInspection>, SandboxError>,
    pull: impl FnOnce() -> Result<Option<OciImageInspection>, SandboxError>,
) -> Result<OciImageInspection, SandboxError> {
    local.and_then(|inspection| match (inspection, acquisition) {
        | (Some(inspection), _) => Ok(inspection),
        | (None, WorkloadAcquisition::LocalOnly) => Err(SandboxError::Runtime(format!(
            "pinned workload {workload} is not available locally and offline mode prohibits pulling it"
        ))),
        | (None, WorkloadAcquisition::PullIfMissing) => pull().and_then(|inspection| {
            inspection.ok_or_else(|| SandboxError::Runtime(format!("pinned workload {workload} is unavailable after a successful pull")))
        }),
    })
}
fn image_environment(value: &str) -> &str {
    value.split_once('=').map_or(value, |(name, _)| name)
}
fn normalize_path(base: &Path, path: PathBuf) -> PathBuf {
    match path.is_absolute() {
        | true => path,
        | false => base.join(path),
    }
}
fn paths_are_valid(values: &[PathBuf]) -> bool {
    !values.is_empty()
        && values.iter().all(|value| {
            value.is_absolute()
                && value
                    .components()
                    .all(|component| !matches!(component, Component::ParentDir | Component::CurDir))
        })
}
fn rewrite_path(value: &str, replacements: &[(String, String)]) -> String {
    replacements
        .iter()
        .find_map(|(host, guest)| {
            value
                .strip_prefix(host)
                .and_then(|suffix| match suffix.is_empty() || suffix.starts_with(std::path::MAIN_SEPARATOR) {
                    | true => Some(format!("{guest}{suffix}")),
                    | false => None,
                })
        })
        .unwrap_or_else(|| value.to_string())
}
fn validate_granted_path(path: &Path, roots: &[PathBuf], role: &str) -> Result<PathBuf, SandboxError> {
    symlink_metadata(path)
        .map_err(|why| SandboxError::Validation(format!("{role} path {} cannot be inspected: {why}", path.display())))
        .and_then(|entry| match entry.file_type().is_symlink() {
            | true => Err(SandboxError::Validation(format!("{role} path {} is a symlink", path.display()))),
            | false => Ok(()),
        })
        .and_then(|()| {
            canonicalize(path).map_err(|why| SandboxError::Validation(format!("{role} path {} cannot be resolved: {why}", path.display())))
        })
        .and_then(|resolved| {
            metadata(&resolved)
                .map_err(|why| SandboxError::Validation(format!("{role} path {} cannot be inspected: {why}", resolved.display())))
                .and_then(|entry| match entry.is_file() || entry.is_dir() {
                    | true => Ok(resolved),
                    | false => Err(SandboxError::Validation(format!(
                        "{role} path {} is not a regular file or directory",
                        resolved.display()
                    ))),
                })
        })
        .and_then(|resolved| {
            let granted = roots.iter().any(|root| canonicalize(root).is_ok_and(|root| resolved.starts_with(root)));
            match granted {
                | true => Ok(resolved),
                | false => Err(SandboxError::Validation(format!(
                    "{role} path {} is outside the operator policy roots",
                    resolved.display()
                ))),
            }
        })
        .and_then(|resolved| validate_mount_tree(&resolved, role).map(|()| resolved))
        .and_then(
            |resolved| match resolved.to_str().is_some() && !resolved.to_string_lossy().contains(',') {
                | true => Ok(resolved),
                | false => Err(SandboxError::Validation(format!(
                    "{role} path {} cannot be represented safely as an OCI mount",
                    resolved.display()
                ))),
            },
        )
}
fn validate_mount_separation(module: &Path, input: &Path) -> Result<(), SandboxError> {
    match module.starts_with(input) || input.starts_with(module) {
        | true => Err(SandboxError::Validation(format!(
            "module path {} and input path {} overlap",
            module.display(),
            input.display()
        ))),
        | false => Ok(()),
    }
}
fn validate_mount_tree(root: &Path, role: &str) -> Result<(), SandboxError> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut visited = 0_usize;
    loop {
        let Some(path) = pending.pop_front() else {
            break Ok(());
        };
        visited = visited.saturating_add(1);
        if visited > MAX_MOUNT_ENTRIES {
            break Err(SandboxError::Validation(format!(
                "{role} path {} exceeds the {MAX_MOUNT_ENTRIES}-entry mount validation limit",
                root.display()
            )));
        }
        let entry = match symlink_metadata(&path) {
            | Ok(entry) => entry,
            | Err(why) => {
                break Err(SandboxError::Validation(format!(
                    "{role} mount entry {} cannot be inspected: {why}",
                    path.display()
                )));
            }
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            break Err(SandboxError::Validation(format!("{role} mount entry {} is a symlink", path.display())));
        }
        if entry.is_dir() {
            let children = match read_dir(&path) {
                | Ok(children) => children,
                | Err(why) => {
                    break Err(SandboxError::Validation(format!(
                        "{role} directory {} cannot be read: {why}",
                        path.display()
                    )));
                }
            };
            let children = children
                .map(|child| {
                    child
                        .map(|child| child.path())
                        .map_err(|why| SandboxError::Validation(format!("{role} directory {} cannot be read: {why}", path.display())))
                })
                .collect::<Result<Vec<_>, _>>();
            match children {
                | Ok(mut children) => {
                    children.sort();
                    pending.extend(children);
                }
                | Err(why) => break Err(why),
            }
        } else if !entry.is_file() {
            break Err(SandboxError::Validation(format!(
                "{role} mount entry {} is not a regular file or directory",
                path.display()
            )));
        }
    }
}
fn validate_template_path(module: &Module, module_source: &Path) -> Result<(), SandboxError> {
    match &module.template {
        | Some(TemplateAttribute::Path(path)) => {
            validate_granted_path(&module_source.join(path), &[module_source.to_path_buf()], "module template").map(|_| ())
        }
        | Some(TemplateAttribute::Template(_)) | None => Ok(()),
    }
}
fn validation_error(code: &str, message: impl Into<String>) -> ValidationError {
    ValidationError::new(code).with_message(message)
}
fn validation_report(path: &str, code: &str, message: impl Into<String>) -> ValidationReport {
    ValidationReport::from_error(path, validation_error(code, message))
}
fn workload_is_pinned(value: &str) -> bool {
    value.rsplit_once("@sha256:").is_some_and(|(name, digest)| {
        !name.is_empty() && digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

#[cfg(test)]
mod tests;
