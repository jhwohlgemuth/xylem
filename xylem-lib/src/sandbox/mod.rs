//! Opt-in sandbox policy and provider contracts.
extern crate alloc;

use crate::config::{Config, InputAttribute};
use crate::error::ProcessError;
use crate::script::{Module, ModuleLanguage, ModuleType, ModuleUri, Script, TemplateAttribute};
use acorn_cmd::{args, CommandError, CommandSpec, ExecutionOptions, ExecutionResult};
use acorn_core::util::MimeType;
use acorn_core::validation::ValidationError;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::{BTreeSet, VecDeque};
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
/// Network access available while preparing workflow dependencies.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationNetwork {
    /// No network access.
    Deny,
    /// Unrestricted provider network access.
    Unrestricted,
}
/// OCI runtime selected by a trusted operator policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxBackend {
    /// Docker Engine.
    Docker,
    /// Podman.
    Podman,
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
/// Lifecycle behavior after a sandboxed run fails.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRetention {
    /// Delete resources after every terminal outcome.
    Delete,
    /// Retain resources only after failure.
    RetainOnFailure,
}
/// Validated immutable sandbox policy with its canonical identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveSandboxPolicy {
    /// OCI runtime selected by the operator.
    pub backend: SandboxBackend,
    /// Canonical policy digest.
    pub digest: String,
    /// Digest-pinned runner image.
    pub image: String,
    /// Validated operator policy.
    pub policy: OperatorSandboxPolicy,
}
/// Strict operator-owned sandbox policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorSandboxPolicy {
    /// Images permitted for sandbox sessions, each pinned by digest.
    pub allowed_images: Vec<String>,
    /// OCI runtime selected by the operator.
    pub backend: SandboxBackend,
    /// Default digest-pinned runner image.
    pub default_image: String,
    /// Child environment allowlist.
    pub environment: SandboxEnvironment,
    /// Cleanup and reconciliation behavior.
    pub lifecycle: SandboxLifecycle,
    /// Host path grants.
    pub paths: SandboxPaths,
    /// Preparation-session network access.
    pub preparation_network: PreparationNetwork,
    /// Per-input resource ceilings.
    pub resources: SandboxResources,
    /// Policy schema version.
    pub version: u32,
}
/// Controls an OCI provider can enforce for this implementation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SandboxCapabilities {
    /// Provider can deny execution networking.
    pub network_deny: bool,
    /// Provider uses a native shared Linux kernel.
    pub shared_kernel: bool,
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
impl Config {
    pub(crate) fn validate_sandbox(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if !self.requirements.is_empty() {
            report.add(
                "requirements",
                validation_error(
                    "unsupported",
                    "workflow requirements are not supported in sandbox mode; provide them in the pinned runner image",
                ),
            );
        }
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
            match &module.module_type {
                | ModuleType::Script {
                    programming_language: Some(ModuleLanguage::Generic) | None,
                    uri,
                } if matches!(ModuleUri::from(uri), ModuleUri::File(_)) => (),
                | ModuleType::Script { .. } => report.add(
                    format!("{path}.uri"),
                    validation_error("sandbox", format!("module {} must be a local generic script", module.name)),
                ),
                | ModuleType::Binary { .. } => report.add(
                    format!("{path}.type"),
                    validation_error("unsupported", format!("binary module {} is not supported in sandbox mode", module.name)),
                ),
                | ModuleType::Configuration { .. } => report.add(
                    format!("{path}.type"),
                    validation_error(
                        "unsupported",
                        format!("nested configuration module {} is not supported in sandbox mode", module.name),
                    ),
                ),
            }
            if module.completion.is_some() {
                report.add(
                    format!("{path}.completion"),
                    validation_error("unsupported", "completion markers are not yet supported in sandbox mode"),
                );
            }
            if matches!(
                &module.template,
                Some(TemplateAttribute::Template(template)) if template.container.is_some() || template.virtual_environment.is_some()
            ) {
                report.add(
                    format!("{path}.template"),
                    validation_error(
                        "unsupported",
                        format!(
                            "module {} uses a host environment wrapper that is not supported in sandbox mode",
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
                    .map(|canonical| Self {
                        backend: policy.backend,
                        digest: format!("sha256:{:x}", Sha256::digest(canonical)),
                        image: policy.default_image.clone(),
                        policy,
                    })
            })
    }
    /// Return the declared provider capabilities after verifying the local runtime.
    pub fn discover(&self) -> Result<SandboxCapabilities, SandboxError> {
        self.backend.discover().and_then(|()| self.verify()).map(|()| SandboxCapabilities {
            network_deny: true,
            read_only_root: true,
            resource_limits: true,
            rootless: true,
            shared_kernel: true,
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
            config
                .modules
                .iter()
                .map(|module| {
                    validate_granted_path(&module.sandbox_path(), &self.policy.paths.module_roots, "module").and_then(|module_source| {
                        validate_template_path(module, &module_source).and_then(|()| {
                            Script::from_module(module.clone(), None)
                                .map_err(|why| {
                                    SandboxError::Validation(format!("module {} cannot be loaded for sandbox execution: {why}", module.name))
                                })
                                .and_then(|script| script.validate().map_err(|why| SandboxError::Validation(why.to_string())))
                                .map(|()| module_source)
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .and_then(|modules| {
                    inputs
                        .iter()
                        .flatten()
                        .map(|input| validate_granted_path(Path::new(input), &self.policy.paths.input_roots, "input"))
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|inputs| {
                            inputs
                                .iter()
                                .try_for_each(|input| modules.iter().try_for_each(|module| validate_mount_separation(module, input)))
                        })
                })
        })
    }
    /// Execute one resolved module command through the configured OCI runtime.
    pub fn execute(&self, script: &Script, container_name: &str, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError> {
        let working_directory = PathBuf::from(&script.current_dir);
        self.invocation(script, container_name)
            .map_err(|why| ProcessError::Spawn {
                script: script.to_string(),
                working_directory: working_directory.clone(),
                source: std::io::Error::other(why),
            })
            .and_then(|arguments| {
                CommandSpec::new(self.backend.to_string())
                    .args(arguments)
                    .checked_execute(options)
                    .map_err(|source| match source {
                        | CommandError::NonZeroExit(result) => {
                            let details = String::from_utf8_lossy(result.stderr().bytes()).into_owned();
                            let status = result.status().map_or_else(
                                || "unknown".to_string(),
                                |status| {
                                    status.code().map_or_else(
                                        || status.signal().map_or_else(|| "unknown".to_string(), |signal| format!("signal {signal}")),
                                        |code| code.to_string(),
                                    )
                                },
                            );
                            ProcessError::Unsuccessful {
                                script: script.to_string(),
                                working_directory: working_directory.clone(),
                                status,
                                stderr: details,
                            }
                        }
                        | CommandError::Spawn { source } => ProcessError::Spawn {
                            script: script.to_string(),
                            working_directory: working_directory.clone(),
                            source,
                        },
                        | source => ProcessError::Managed {
                            script: script.to_string(),
                            working_directory: working_directory.clone(),
                            source,
                        },
                    })
            })
    }
    /// Best-effort terminal cleanup for a named sandbox.
    pub fn cleanup(&self, container_name: &str, succeeded: bool) {
        if succeeded || self.policy.lifecycle.retention == SandboxRetention::Delete {
            let _cleanup = CommandSpec::new(self.backend.to_string())
                .args(args!["rm", "--force", container_name])
                .checked_output_with_timeout(Duration::from_secs(10));
        }
    }
    fn invocation(&self, script: &Script, container_name: &str) -> Result<Vec<OsString>, SandboxError> {
        script.validate().map_err(|why| SandboxError::Validation(why.to_string())).and_then(|()| {
            validate_granted_path(
                Path::new(&script.current_dir),
                &self.policy.paths.module_roots,
                "module working directory",
            )
            .and_then(|module_source| {
                let input = script.envs.as_ref().and_then(|envs| envs.get("_XYLEM_INPUT"));
                input
                    .map(|value| validate_granted_path(Path::new(value), &self.policy.paths.input_roots, "input"))
                    .transpose()
                    .and_then(|input_source| {
                        input_source
                            .as_ref()
                            .map_or(Ok(()), |input_source| validate_mount_separation(&module_source, input_source))
                            .map(|()| (module_source, input.map(PathBuf::from), input_source))
                    })
            })
            .map(|(module_source, input, input_source)| {
                let replacements = path_replacements(Path::new(&script.current_dir), &module_source, input.as_deref(), input_source.as_deref());
                let command = rewrite_path(&script.command.name, &replacements);
                let arguments = script.arguments.iter().map(|argument| rewrite_path(argument, &replacements));
                let environment = script
                    .envs
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(name, _)| self.policy.environment.allow.contains(name))
                    .map(|(name, value)| format!("{name}={}", rewrite_path(&value, &replacements)))
                    .collect::<Vec<_>>();
                let resources = &self.policy.resources;
                let cpus = format!("{:.3}", f64::from(resources.cpu_millis) / 1000.0);
                let module_mount = format!("type=bind,source={},target=/xylem/modules/current,readonly", module_source.display());
                let input_mount = input_source
                    .as_ref()
                    .map(|source| format!("type=bind,source={},target=/xylem/input/current,readonly", source.display()));
                args![
                    "run",
                    ..(self.policy.lifecycle.retention == SandboxRetention::Delete).then_some("--rm"),
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
                    ("--mount", module_mount),
                    ..input_mount.map(|mount| args!["--mount", mount]).unwrap_or_default(),
                    ..environment.into_iter().flat_map(|value| args!["--env", value]),
                    "--workdir=/xylem/modules/current",
                    &self.image,
                    command,
                    ..arguments
                ]
            })
        })
    }
    fn verify(&self) -> Result<(), SandboxError> {
        CommandSpec::new(self.backend.to_string())
            .args(args!["image", "inspect", "--format", "{{json .Config.Env}}", &self.image])
            .checked_output_with_timeout(Duration::from_secs(10))
            .map_err(|why| SandboxError::Runtime(format!("inspect pinned image {}: {why}", self.image)))
            .and_then(|output| {
                serde_json::from_slice::<Option<Vec<String>>>(&output.stdout)
                    .map(Option::unwrap_or_default)
                    .map_err(|why| SandboxError::Runtime(format!("parse environment for pinned image {}: {why}", self.image)))
            })
            .and_then(|environment| {
                environment
                    .iter()
                    .map(|value| image_environment(value))
                    .find(|name| !self.policy.environment.allow.iter().any(|allowed| allowed == name))
                    .map_or(Ok(()), |name| {
                        Err(SandboxError::Runtime(format!(
                            "pinned image {} declares environment variable {name} outside the policy allowlist",
                            self.image
                        )))
                    })
            })
    }
}
impl Module {
    fn sandbox_path(&self) -> PathBuf {
        let uri = match &self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri } | ModuleType::Script { uri, .. } => uri,
        };
        match ModuleUri::from(uri) {
            | ModuleUri::File(path) => path,
            | ModuleUri::Url(_) | ModuleUri::Unknown(_) => PathBuf::new(),
        }
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
            self.allowed_images.sort();
            self.environment.allow.sort();
            self.paths.artifact_roots = self.paths.artifact_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.input_roots = self.paths.input_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.module_roots = self.paths.module_roots.into_iter().map(|value| normalize_path(&base, value)).collect();
            self.paths.artifact_roots.sort();
            self.paths.input_roots.sort();
            self.paths.module_roots.sort();
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
        let allowed_images = self.allowed_images.iter().collect::<BTreeSet<_>>();
        let environment = self.environment.allow.iter().collect::<BTreeSet<_>>();
        let artifact_roots = self.paths.artifact_roots.iter().collect::<BTreeSet<_>>();
        let input_roots = self.paths.input_roots.iter().collect::<BTreeSet<_>>();
        let module_roots = self.paths.module_roots.iter().collect::<BTreeSet<_>>();
        let mut report = ValidationReport::new();
        if self.version != SANDBOX_POLICY_VERSION {
            report.add(
                "version",
                validation_error("version", format!("policy version must be {SANDBOX_POLICY_VERSION}")),
            );
        }
        if !image_is_pinned(&self.default_image) {
            report.add(
                "default_image",
                validation_error("digest", "default_image must use an immutable sha256 digest"),
            );
        }
        if self.allowed_images.iter().any(|image| !image_is_pinned(image)) {
            report.add(
                "allowed_images",
                validation_error("digest", "allowed_images must contain only immutable sha256 digests"),
            );
        }
        if !self.allowed_images.contains(&self.default_image) {
            report.add(
                "allowed_images",
                validation_error("contains", "allowed_images must contain default_image"),
            );
        }
        if allowed_images.len() != self.allowed_images.len() {
            report.add("allowed_images", validation_error("unique", "allowed_images must not contain duplicates"));
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
impl fmt::Display for SandboxBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Docker => formatter.write_str("docker"),
            | Self::Podman => formatter.write_str("podman"),
        }
    }
}
impl SandboxBackend {
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
impl Validate for SandboxBackend {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self {
            | Self::Docker => CommandSpec::new("docker")
                .args(args!["context", "show"])
                .checked_output_with_timeout(Duration::from_secs(10))
                .map_err(|why| validation_report("backend", "runtime", format!("inspect Docker context: {why}")))
                .and_then(|output| {
                    let context = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    CommandSpec::new("docker")
                        .args(args!["context", "inspect", "--format", "{{json .Endpoints.docker.Host}}", context])
                        .checked_output_with_timeout(Duration::from_secs(10))
                        .map_err(|why| validation_report("backend", "runtime", format!("inspect Docker endpoint: {why}")))
                })
                .and_then(|output| {
                    serde_json::from_slice::<String>(&output.stdout)
                        .map_err(|why| validation_report("backend", "runtime", format!("parse Docker endpoint: {why}")))
                })
                .and_then(|endpoint| match endpoint.starts_with("unix://") {
                    | true => Ok(()),
                    | false => Err(validation_report(
                        "backend",
                        "local",
                        format!("Docker endpoint {endpoint} is remote; the sandbox requires a local Unix socket"),
                    )),
                }),
            | Self::Podman => CommandSpec::new("podman")
                .args(args!["info", "--format", "{{.Host.ServiceIsRemote}}"])
                .checked_output_with_timeout(Duration::from_secs(10))
                .map_err(|why| validation_report("backend", "runtime", format!("inspect Podman connection: {why}")))
                .and_then(|output| match String::from_utf8_lossy(&output.stdout).trim() {
                    | "false" => Ok(()),
                    | _ => Err(validation_report(
                        "backend",
                        "local",
                        "Podman is using a remote service; the sandbox requires a local runtime",
                    )),
                }),
        }
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
        if self.command.virtual_environment.is_some() {
            report.add(
                "command.virtual_environment",
                validation_error("unsupported", "host virtual environments are not supported inside sandbox mode"),
            );
        }
        report.finish()
    }
}
fn image_environment(value: &str) -> &str {
    value.split_once('=').map_or(value, |(name, _)| name)
}
fn image_is_pinned(value: &str) -> bool {
    value.rsplit_once("@sha256:").is_some_and(|(name, digest)| {
        !name.is_empty() && digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn normalize_path(base: &Path, path: PathBuf) -> PathBuf {
    match path.is_absolute() {
        | true => path,
        | false => base.join(path),
    }
}
fn path_replacements(module: &Path, resolved_module: &Path, input: Option<&Path>, resolved_input: Option<&Path>) -> Vec<(String, String)> {
    let mut replacements = [input, resolved_input]
        .into_iter()
        .flatten()
        .map(|path| (path.display().to_string(), "/xylem/input/current".to_string()))
        .collect::<Vec<_>>();
    replacements.extend(
        [module, resolved_module]
            .into_iter()
            .map(|path| (path.display().to_string(), "/xylem/modules/current".to_string())),
    );
    replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| left.0.cmp(&right.0)));
    replacements.dedup();
    replacements
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

#[cfg(test)]
mod tests;
