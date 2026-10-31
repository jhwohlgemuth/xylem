//! OCI sandbox lifecycle implementation.
//!
//! The lifecycle separates network-aware preparation from network-disabled
//! execution. A provider first opens a run session:
//!
//! ```rust,ignore
//! use xylem_lib::sandbox::{EffectiveSandboxPolicy, SandboxProvider, WorkloadAcquisition};
//!
//! let policy = EffectiveSandboxPolicy::read("sandbox-policy.yaml")?;
//! policy.discover()?;
//! let session = policy.open("run-id", WorkloadAcquisition::LocalOnly)?;
//! ```
//!
//! Each admitted input then moves from [`Requested`](super::Requested) to
//! [`Prepared`](super::Prepared) exactly once before commands can execute:
//!
//! ```rust,ignore
//! use xylem_lib::sandbox::{Requested, SandboxInput, SandboxPreparationState};
//!
//! let request = SandboxInput::<Requested>::new(config, 0, [0, 2], staging_root, input)?;
//! let prepared = request.prepare(session.as_ref(), |module_index, state| {
//!     if state == SandboxPreparationState::Finished {
//!         println!("prepared module {module_index}");
//!     }
//! })?;
//! prepared.execute(&script, 0, &execution_options)?;
//! prepared.finish(true)?;
//! session.finish(true)?;
//! ```
extern crate alloc;

use acorn_core::time::UnitOfTime;

#[cfg(target_os = "linux")]
use super::memory;
use super::{
    archive, artifact::ArtifactInput, artifact::ArtifactManager, private, rewrite_path, validate_granted_path, validate_mount_separation,
    EffectiveSandboxPolicy, OciSandboxSession, PreparationNetwork, Prepared, PreparedSandboxBackend, Requested, SandboxError, SandboxInput,
    SandboxPreparation, SandboxProviderClient, SandboxResourceDisposition, SandboxResourcePhase, SandboxResourceTracker, SandboxResources,
    SANDBOX_CREATED_LABEL, SANDBOX_INPUT_LABEL, SANDBOX_MANAGED_LABEL, SANDBOX_PHASE_LABEL, SANDBOX_POLICY_LABEL, SANDBOX_PROVIDER_LABEL,
    SANDBOX_RESOURCE_VERSION, SANDBOX_RUN_LABEL, SANDBOX_STAGING_LABEL, SANDBOX_VERSION_LABEL,
};
use crate::config::Config;
use crate::error::ProcessError;
use crate::script::{Module, ModuleLanguage, ModuleType, ModuleUri, Script, VirtualEnvironmentManager};
use crate::util::timestamp;
use acorn_cmd::{args, ExecutionOptions, ExecutionResult};
use acorn_core::util::Checksum;
use acorn_core::Location;
use acorn_host::fs::file_checksum;
use acorn_schema::hardware::Memory;
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use core::time::Duration;
use std::ffi::OsString;
use std::fs::{create_dir, create_dir_all, metadata, read_to_string, remove_dir_all, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Instant;

pub(super) const PREPARATION_ENVIRONMENT: &str = "if [ -f environment.yml ]; then if command -v conda >/dev/null 2>&1; then manager=conda; elif command -v mamba >/dev/null 2>&1; then manager=mamba; else echo 'environment.yml requires conda or mamba in the sandbox workload' >&2; exit 127; fi; mkdir -p .xylem-conda/envs .xylem-conda/pkgs .xylem-home .xylem-tmp; printf '%s\n' \"$manager\" > .xylem-manager; HOME=\"$PWD/.xylem-home\" TMPDIR=\"$PWD/.xylem-tmp\" CONDA_ENVS_PATH=\"$PWD/.xylem-conda/envs\" CONDA_PKGS_DIRS=\"$PWD/.xylem-conda/pkgs\" \"$manager\" env create -f environment.yml; elif [ -f pixi.toml ]; then command -v pixi >/dev/null 2>&1 || { echo 'pixi.toml requires pixi in the sandbox workload' >&2; exit 127; }; mkdir -p .xylem-home .xylem-tmp; printf 'pixi\n' > .xylem-manager; HOME=\"$PWD/.xylem-home\" TMPDIR=\"$PWD/.xylem-tmp\" pixi install; fi";
const SANDBOX_EXPORT_PATH: &str = "/xylem/export.tar";

pub(super) struct OciPreparation {
    artifact: Option<ArtifactInput>,
    artifacts: Arc<ArtifactManager>,
    policy: EffectiveSandboxPolicy,
    prepared: BTreeSet<usize>,
    request: SandboxInput<Requested>,
    resource: OciResource,
    run_id: String,
    workspace: PathBuf,
    workspace_transferred: bool,
}
struct OciPreparedInput {
    artifact: Option<ArtifactInput>,
    artifacts: Arc<ArtifactManager>,
    config: Config,
    input: Option<String>,
    policy: EffectiveSandboxPolicy,
    prepared_root: PathBuf,
    resource: OciResource,
    workspace: PathBuf,
}
struct OciResource {
    identifier: String,
    policy: EffectiveSandboxPolicy,
    terminal: Mutex<Option<SandboxResourceDisposition>>,
    tracker: Arc<SandboxResourceTracker>,
}
pub(super) struct OciResourceSpec<'a> {
    pub(super) artifact_staging: Option<&'a Path>,
    pub(super) input_index: usize,
    pub(super) name: &'a str,
    pub(super) phase: SandboxResourcePhase,
    pub(super) run_id: &'a str,
    pub(super) tracker: Arc<SandboxResourceTracker>,
}
impl Config {
    pub(super) fn with_sandbox_paths(mut self, prepared_root: &Path, artifacts: bool, module_indices: &[usize]) -> Self {
        module_indices.iter().for_each(|index| {
            if let Some(module) = self.modules.get_mut(*index) {
                let path = match &module.module_type {
                    | ModuleType::Binary { .. } => prepared_root.join("modules").join(index.to_string()).join("module"),
                    | ModuleType::Configuration { .. } | ModuleType::Script { .. } => prepared_root.join("modules").join(index.to_string()),
                };
                let location = Location::from(format!("file:{}", path.display()).as_str());
                match &mut module.module_type {
                    | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri, .. } | ModuleType::Script { uri, .. } => *uri = location,
                }
                if artifacts {
                    module.completion = None;
                }
            }
        });
        if artifacts {
            self.output = Some("/xylem/output".to_string());
            self.variables
                .get_or_insert_default()
                .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String("/xylem/output".to_string()));
            self.modules.iter_mut().for_each(|module| {
                module
                    .variables
                    .get_or_insert_default()
                    .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String("/xylem/output".to_string()));
            });
        }
        self
    }
}
impl EffectiveSandboxPolicy {
    pub(super) fn container_base(&self, spec: &OciResourceSpec<'_>, network: Option<&str>, maximum_export_bytes: u64) -> Vec<OsString> {
        let resources = &self.policy.resources;
        let cpus = format!("{:.3}", f64::from(resources.cpu_millis) / 1000.0);
        let file_size = resources
            .artifact_files
            .checked_mul(4096)
            .and_then(|overhead| maximum_export_bytes.checked_add(overhead))
            .unwrap_or(u64::MAX);
        args![
            "create",
            "--init",
            ("--name", spec.name),
            ("--label", format!("{SANDBOX_MANAGED_LABEL}=true")),
            ("--label", format!("{SANDBOX_POLICY_LABEL}={}", self.digest)),
            ("--label", format!("{SANDBOX_PROVIDER_LABEL}=oci")),
            ("--label", format!("{SANDBOX_RUN_LABEL}={}", spec.run_id)),
            ("--label", format!("{SANDBOX_INPUT_LABEL}={}", spec.input_index)),
            ("--label", format!("{SANDBOX_PHASE_LABEL}={}", spec.phase)),
            ("--label", format!("{SANDBOX_CREATED_LABEL}={}", timestamp().get())),
            ("--label", format!("{SANDBOX_VERSION_LABEL}={SANDBOX_RESOURCE_VERSION}")),
            ..spec
                .artifact_staging
                .map(|path| args![("--label", format!("{SANDBOX_STAGING_LABEL}={}", path.display()))])
                .unwrap_or_default(),
            "--pull=never",
            ..network.map(|value| args![format!("--network={value}")]).unwrap_or_default(),
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--ipc=private",
            ("--pids-limit", resources.pids.to_string()),
            ("--memory", resources.memory_bytes.to_string()),
            ("--memory-swap", resources.memory_bytes.to_string()),
            ("--cpus", cpus),
            ("--ulimit", format!("fsize={file_size}:{file_size}")),
            "--user=65532:65532"
        ]
    }
    fn create_execution_container(
        &self,
        spec: OciResourceSpec<'_>,
        prepared_root: &Path,
        input: Option<&str>,
        archive_path: Option<&Path>,
    ) -> Result<OciResource, SandboxError> {
        let prepared_mount = format!("type=bind,source={},target=/xylem/prepared,readonly", prepared_root.display());
        let input_mount = input
            .map(Path::new)
            .map(|path| validate_granted_path(path, &self.policy.paths.input_roots, "input"))
            .transpose()
            .map(|input| input.map(|path| format!("type=bind,source={},target=/xylem/input/current,readonly", path.display())));
        input_mount.and_then(|input_mount| {
            let archive_mount = archive_path.map(archive_mount);
            let arguments = args![
                ..self.container_base(&spec, Some("none"), self.policy.resources.artifact_bytes),
                (
                    "--tmpfs",
                    format!("/xylem/tmp:rw,noexec,nosuid,nodev,size={},mode=1777", self.policy.resources.scratch_bytes)
                ),
                ("--mount", prepared_mount),
                ..input_mount.map(|mount| args!["--mount", mount]).unwrap_or_default(),
                ..archive_mount.map(|mount| args!["--mount", mount]).unwrap_or_default(),
                ..archive_path
                    .map(|_| args![
                        "--tmpfs",
                        format!(
                            "/xylem/output:rw,noexec,nosuid,nodev,size={},mode=1777",
                            self.policy.resources.artifact_bytes
                        )
                    ])
                    .unwrap_or_default(),
                "--workdir=/xylem/prepared",
                ("--entrypoint", "sleep"),
                &self.workload,
                "infinity"
            ];
            self.create_and_start(spec, arguments)
        })
    }
    fn create_preparation_container(
        &self,
        spec: OciResourceSpec<'_>,
        request: &SandboxInput<Requested>,
        archive_path: &Path,
    ) -> Result<OciResource, SandboxError> {
        request
            .module_indices()
            .iter()
            .filter_map(|index| request.config().modules.get(*index).map(|module| (*index, module)))
            .map(|(index, module)| {
                module.sandbox_local_source().and_then(|source| {
                    source
                        .map(|source| validate_granted_path(&source, &self.policy.paths.module_roots, "module").map(|source| (index, source)))
                        .transpose()
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|sources| sources.into_iter().flatten().collect::<Vec<_>>())
            .and_then(|sources| {
                sources
                    .iter()
                    .try_for_each(|(_, source)| {
                        request.input().map_or(Ok(()), |input| {
                            validate_granted_path(Path::new(input), &self.policy.paths.input_roots, "input")
                                .and_then(|input| validate_mount_separation(source, &input))
                        })
                    })
                    .and_then(|()| {
                        let mounts = sources.into_iter().flat_map(|(index, source)| {
                            args![
                                "--mount",
                                format!("type=bind,source={},target=/xylem/source-{index},readonly", source.display())
                            ]
                        });
                        let network = match self.policy.preparation_network {
                            | PreparationNetwork::Deny => Some("none"),
                            | PreparationNetwork::Unrestricted => None,
                        };
                        let arguments = args![
                            ..self.container_base(&spec, network, self.policy.resources.scratch_bytes),
                            (
                                "--tmpfs",
                                format!("/xylem/prepared:rw,nosuid,nodev,size={},mode=1777", self.policy.resources.scratch_bytes)
                            ),
                            ..mounts,
                            ("--mount", archive_mount(archive_path)),
                            "--workdir=/xylem/prepared",
                            ("--entrypoint", "sleep"),
                            &self.workload,
                            "infinity"
                        ];
                        self.create_and_start(spec, arguments)
                    })
            })
    }
    fn create_and_start(&self, spec: OciResourceSpec<'_>, arguments: Vec<OsString>) -> Result<OciResource, SandboxError> {
        let name = spec.name.to_string();
        self.runtime().and_then(|runtime| {
            runtime
                .checked_output_with_timeout(
                    arguments,
                    Duration::from_secs(self.policy.resources.wall_time_seconds),
                    &format!("create sandbox {name}"),
                )
                .and_then(|_| spec.tracker.register(name.clone(), spec.input_index, spec.phase))
                .map(|()| OciResource {
                    identifier: name.clone(),
                    policy: self.clone(),
                    terminal: Mutex::new(None),
                    tracker: spec.tracker,
                })
                .and_then(|resource| {
                    let started = runtime.checked_output_with_timeout(
                        args!["start", &name],
                        Duration::from_secs(self.policy.resources.wall_time_seconds),
                        &format!("start sandbox {name}"),
                    );
                    match started {
                        | Ok(_) => Ok(resource),
                        | Err(primary) => match resource.finish(false) {
                            | Ok(_) => Err(primary),
                            | Err(cleanup) => Err(SandboxError::Runtime(format!("{primary}; additionally, {cleanup}"))),
                        },
                    }
                })
        })
    }
    fn execute_preparation(&self, container_name: &str, working_directory: &str, command: Vec<OsString>) -> Result<(), SandboxError> {
        self.runtime()
            .and_then(|runtime| {
                runtime.checked_output_with_timeout(
                    args!["exec", ("--workdir", working_directory), container_name, ..command],
                    Duration::from_secs(self.policy.resources.wall_time_seconds),
                    &format!("prepare sandbox {container_name}"),
                )
            })
            .map(|_| ())
    }
}
impl EffectiveSandboxPolicy {
    fn export_archive(&self, container_name: &str, archive_path: &Path, source: &str, maximum_bytes: u64) -> Result<(), SandboxError> {
        let stderr_path = archive_path.with_extension("stderr");
        self.runtime().and_then(|runtime| {
            File::create(archive_path)
                .map_err(|why| SandboxError::Runtime(format!("create prepared archive {}: {why}", archive_path.display())))
                .and_then(|_| {
                    File::create(&stderr_path)
                        .map_err(|why| SandboxError::Runtime(format!("create prepared archive diagnostics {}: {why}", stderr_path.display())))
                        .and_then(|stderr| {
                            Command::new(runtime.to_string())
                                .args(archive_export_arguments(container_name, source))
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::from(stderr))
                                .spawn()
                                .map_err(|why| SandboxError::Runtime(format!("export prepared sandbox {container_name}: {why}")))
                        })
                })
                .and_then(|mut child| {
                    let timeout = Duration::from_secs(self.policy.resources.wall_time_seconds);
                    let started = Instant::now();
                    let transfer_limit = self
                        .policy
                        .resources
                        .artifact_files
                        .checked_mul(4096)
                        .and_then(|overhead| maximum_bytes.checked_add(overhead))
                        .unwrap_or(u64::MAX);
                    loop {
                        let size = metadata(archive_path).map(|value| value.len()).unwrap_or_default();
                        if size > transfer_limit || started.elapsed() >= timeout {
                            let _ = child.kill();
                            let _ = child.wait();
                            break Err(SandboxError::Runtime(match size > transfer_limit {
                                | true => format!("prepared sandbox archive exceeded the {transfer_limit}-byte transfer limit"),
                                | false => format!("export prepared sandbox {container_name} timed out after {} seconds", timeout.as_secs()),
                            }));
                        }
                        match child.try_wait() {
                            | Ok(Some(status)) if status.success() => break Ok(()),
                            | Ok(Some(status)) => {
                                let diagnostics = read_to_string(&stderr_path).unwrap_or_default();
                                break Err(SandboxError::Runtime(format!(
                                    "export prepared sandbox {container_name} exited with {status}: {}",
                                    diagnostics.trim()
                                )));
                            }
                            | Ok(None) => sleep(Duration::from_millis(25)),
                            | Err(why) => break Err(SandboxError::Runtime(format!("observe prepared sandbox export: {why}"))),
                        }
                    }
                })
        })
    }
}
impl EffectiveSandboxPolicy {
    fn prepare_module(&self, container_name: &str, module_index: usize, module: &Module) -> Result<(), SandboxError> {
        let destination = format!("/xylem/prepared/modules/{module_index}");
        self.execute_preparation(container_name, "/xylem/prepared", args!["mkdir", "-p", &destination])
            .and_then(|()| {
                match &module.module_type {
                    | ModuleType::Binary { uri, .. } => Ok((uri, true, false)),
                    | ModuleType::Script { programming_language, uri } => {
                        Ok((uri, false, matches!(programming_language, Some(ModuleLanguage::Python))))
                    }
                    | ModuleType::Configuration { .. } => Err(SandboxError::Validation(format!(
                        "configuration module {} cannot be prepared in sandbox mode",
                        module.name
                    ))),
                }
                .and_then(|(uri, binary, python)| {
                    let acquisition = match ModuleUri::from(uri) {
                        | ModuleUri::File(_) if binary => self.execute_preparation(
                            container_name,
                            "/xylem/prepared",
                            args![
                                "cp",
                                "/xylem/source-".to_string() + &module_index.to_string(),
                                format!("{destination}/module")
                            ],
                        ),
                        | ModuleUri::File(_) => self.execute_preparation(
                            container_name,
                            "/xylem/prepared",
                            args!["cp", "-a", format!("/xylem/source-{module_index}/."), &destination],
                        ),
                        | ModuleUri::Url(_) | ModuleUri::Oci(_) => Err(SandboxError::Validation(format!(
                            "module {} remote source was not materialized before sandbox preparation",
                            module.name
                        ))),
                        | ModuleUri::Unknown(value) => Err(SandboxError::Validation(format!("module {} has invalid URI {value}", module.name))),
                    };
                    acquisition
                        .and_then(|()| match binary {
                            | true => {
                                self.execute_preparation(container_name, "/xylem/prepared", args!["chmod", "0755", format!("{destination}/module")])
                            }
                            | false => Ok(()),
                        })
                        .and_then(|()| match python {
                            | true => self.execute_preparation(container_name, &destination, args!["sh", "-c", PREPARATION_ENVIRONMENT]),
                            | false => Ok(()),
                        })
                })
            })
    }
}
impl Module {
    pub(super) fn sandbox_local_source(&self) -> Result<Option<PathBuf>, SandboxError> {
        self.sandbox_source().and_then(|location| match ModuleUri::from(&location) {
            | ModuleUri::File(path) => Ok(Some(path)),
            | ModuleUri::Url(_) | ModuleUri::Oci(_) => Ok(None),
            | ModuleUri::Unknown(value) => Err(SandboxError::Validation(format!("module {} has invalid URI {value}", self.name))),
        })
    }
}
impl Drop for OciPreparation {
    fn drop(&mut self) {
        if !self.workspace_transferred && matches!(self.resource.finish(false), Ok(SandboxResourceDisposition::Removed)) {
            let _ = remove_workspace(&self.workspace);
        }
    }
}
impl private::Sealed for OciPreparation {}
impl SandboxPreparation for OciPreparation {
    fn prepare_module(&mut self, module_index: usize) -> Result<(), SandboxError> {
        match (
            self.request.module_indices().binary_search(&module_index).is_ok(),
            self.prepared.contains(&module_index),
            self.request.config().modules.get(module_index),
        ) {
            | (false, _, _) => Err(SandboxError::Validation(format!(
                "module {module_index} is not required for sandbox input"
            ))),
            | (_, true, _) => Err(SandboxError::Validation(format!("module {module_index} was prepared more than once"))),
            | (_, _, None) => Err(SandboxError::Validation(format!("module {module_index} does not exist"))),
            | (true, false, Some(module)) => self.policy.prepare_module(&self.resource.identifier, module_index, module).map(|()| {
                self.prepared.insert(module_index);
            }),
        }
    }
    fn seal(mut self: Box<Self>) -> Result<SandboxInput<Prepared>, SandboxError> {
        let every_module_prepared = self.prepared.len() == self.request.module_indices().len();
        match every_module_prepared {
            | false => Err(SandboxError::Validation("not every required sandbox module was prepared".to_string())),
            | true => {
                let archive_path = self.workspace.join("prepared.tar");
                self.policy
                    .export_archive(
                        &self.resource.identifier,
                        &archive_path,
                        "/xylem/prepared",
                        self.policy.policy.resources.scratch_bytes,
                    )
                    .and_then(|()| self.resource.finish(true).map(|_| ()))
                    .and_then(|()| {
                        let prepared_root = self.workspace.join("prepared");
                        archive::extract(
                            &archive_path,
                            &prepared_root,
                            self.policy.policy.resources.scratch_bytes,
                            self.policy.policy.resources.artifact_files,
                        )
                        .map_err(|why| SandboxError::Runtime(format!("validate prepared sandbox archive: {why}")))
                        .map(|()| prepared_root)
                    })
                    .and_then(|prepared_root| {
                        self.request.verify_binary_checksums(&prepared_root).and_then(|()| {
                            let config = self.request.config().clone().with_sandbox_paths(
                                &prepared_root,
                                self.artifact.is_some(),
                                self.request.module_indices(),
                            );
                            let execution_name = format!("xylem-{}-{}-exec", self.run_id, self.request.input_index());
                            let resource = OciResourceSpec {
                                artifact_staging: self
                                    .artifact
                                    .as_ref()
                                    .and_then(|artifact| artifact.archive.parent())
                                    .and_then(Path::parent),
                                input_index: self.request.input_index(),
                                name: &execution_name,
                                phase: SandboxResourcePhase::Execution,
                                run_id: &self.run_id,
                                tracker: Arc::clone(&self.resource.tracker),
                            };
                            self.policy
                                .create_execution_container(
                                    resource,
                                    &prepared_root,
                                    self.request.input(),
                                    self.artifact.as_ref().map(|artifact| artifact.archive.as_path()),
                                )
                                .map(|resource| {
                                    self.workspace_transferred = true;
                                    SandboxInput::from_backend(Arc::new(OciPreparedInput {
                                        artifact: self.artifact.clone(),
                                        artifacts: Arc::clone(&self.artifacts),
                                        config,
                                        input: self.request.input().map(str::to_string),
                                        policy: self.policy.clone(),
                                        prepared_root,
                                        resource,
                                        workspace: self.workspace.clone(),
                                    }))
                                })
                        })
                    })
            }
        }
    }
    fn verify_requirements(&mut self) -> Result<(), SandboxError> {
        match self
            .request
            .config()
            .options
            .as_ref()
            .and_then(|options| options.skip_verify_requirements)
            .unwrap_or_default()
        {
            | true => Ok(()),
            | false => self.request.config().requirements.iter().try_for_each(|requirement| {
                self.policy
                    .execute_preparation(
                        &self.resource.identifier,
                        "/xylem/prepared",
                        args!["sh", "-c", "command -v \"$1\" >/dev/null 2>&1", "xylem-requirement", requirement],
                    )
                    .map(|_| ())
                    .map_err(|why| SandboxError::Runtime(format!("sandbox requirement {requirement} is unavailable: {why}")))
            }),
        }
    }
}
impl Drop for OciPreparedInput {
    fn drop(&mut self) {
        if matches!(self.resource.finish(false), Ok(SandboxResourceDisposition::Removed)) {
            let _ = remove_workspace(&self.workspace);
        }
    }
}
impl OciPreparedInput {
    fn execution_invocation(&self, script: &Script, module_index: usize) -> Result<Vec<OsString>, SandboxError> {
        let mut replacements = vec![(self.prepared_root.display().to_string(), "/xylem/prepared".to_string())];
        if self.artifact.is_some() {
            replacements.push(("/xylem/output".to_string(), "/xylem/output".to_string()));
        }
        if let Some(value) = self.input.as_ref() {
            replacements.push((value.clone(), "/xylem/input/current".to_string()));
            if let Ok(resolved) = std::fs::canonicalize(value) {
                replacements.push((resolved.display().to_string(), "/xylem/input/current".to_string()));
            }
        }
        replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| left.0.cmp(&right.0)));
        replacements.dedup();
        let manager = read_to_string(self.prepared_root.join("modules").join(module_index.to_string()).join(".xylem-manager"))
            .ok()
            .and_then(|manager| match manager.trim() {
                | "conda" => Some(VirtualEnvironmentManager::Conda),
                | "mamba" => Some(VirtualEnvironmentManager::Mamba),
                | "pixi" => Some(VirtualEnvironmentManager::Pixi),
                | _ => None,
            });
        let mut script = script.clone();
        if let (Some(manager), Some(environment)) = (manager.as_ref(), script.command.virtual_environment.as_mut()) {
            environment.manager = manager.clone();
        }
        script.validate().map_err(|why| SandboxError::Validation(why.to_string())).map(|()| {
            let environment = script
                .envs
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|(name, _)| self.policy.policy.environment.allow.contains(name))
                .map(|(name, value)| format!("{name}={}", rewrite_path(&value, &replacements)))
                .collect::<Vec<_>>();
            let sandbox_environment = match manager {
                | Some(VirtualEnvironmentManager::Conda | VirtualEnvironmentManager::Mamba) => args![
                    "HOME=/xylem/tmp",
                    "TMPDIR=/xylem/tmp",
                    format!("CONDA_ENVS_PATH=/xylem/prepared/modules/{module_index}/.xylem-conda/envs"),
                    "CONDA_PKGS_DIRS=/xylem/tmp/conda-pkgs"
                ],
                | Some(VirtualEnvironmentManager::Pixi) | None => args!["HOME=/xylem/tmp", "TMPDIR=/xylem/tmp"],
                | Some(_) => Vec::new(),
            };
            let command = rewrite_path(&script.get_command(), &replacements);
            let arguments = script.get_arguments().into_iter().map(|argument| rewrite_path(&argument, &replacements));
            args![
                "exec",
                ("--workdir", rewrite_path(&script.current_dir, &replacements)),
                ..environment.into_iter().flat_map(|value| args!["--env", value]),
                ..sandbox_environment.into_iter().flat_map(|value| args!["--env", value]),
                &self.resource.identifier,
                command,
                ..arguments
            ]
        })
    }
    #[cfg(target_os = "linux")]
    fn process_id(&self) -> Result<u32, SandboxError> {
        self.policy.runtime().and_then(|runtime| {
            Command::new(runtime.to_string())
                .args(["inspect", "--format", "{{.State.Pid}}", &self.resource.identifier])
                .output()
                .map_err(|why| SandboxError::Runtime(format!("inspect sandbox input {}: {why}", self.resource.identifier)))
                .and_then(|output| match output.status.success() {
                    | true => String::from_utf8(output.stdout)
                        .map_err(|why| SandboxError::Runtime(format!("decode sandbox input {} process id: {why}", self.resource.identifier)))
                        .and_then(|value| {
                            value
                                .trim()
                                .parse::<u32>()
                                .map_err(|why| SandboxError::Runtime(format!("parse sandbox input {} process id: {why}", self.resource.identifier)))
                        })
                        .and_then(|process_id| match process_id {
                            | 0 => Err(SandboxError::Runtime(format!(
                                "sandbox input {} is not running",
                                self.resource.identifier
                            ))),
                            | process_id => Ok(process_id),
                        }),
                    | false => Err(SandboxError::Runtime(format!(
                        "inspect sandbox input {}: {}",
                        self.resource.identifier,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ))),
                })
        })
    }
}
impl PreparedSandboxBackend for OciPreparedInput {
    fn execute(&self, script: &Script, module_index: usize, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError> {
        let working_directory = PathBuf::from(&script.current_dir);
        self.execution_invocation(script, module_index)
            .map_err(|why| ProcessError::Spawn {
                script: script.to_string(),
                working_directory: working_directory.clone(),
                source: std::io::Error::other(why),
            })
            .and_then(|arguments| {
                self.policy
                    .runtime()
                    .map_err(|why| ProcessError::Spawn {
                        script: script.to_string(),
                        working_directory: working_directory.clone(),
                        source: std::io::Error::other(why),
                    })
                    .and_then(|runtime| {
                        runtime
                            .execute_checked(arguments, options)
                            .map_err(|source| script.sandbox_process_error(&working_directory, source))
                    })
            })
    }
    fn finish(&self, succeeded: bool) -> Result<(), SandboxError> {
        let artifacts = match (&self.artifact, succeeded) {
            | (Some(artifact), true) => self
                .policy
                .export_archive(
                    &self.resource.identifier,
                    &artifact.archive,
                    "/xylem/output",
                    self.policy.policy.resources.artifact_bytes,
                )
                .and_then(|()| self.artifacts.stage(artifact)),
            | _ => Ok(()),
        };
        let cleanup = self.resource.finish(succeeded);
        let workspace = match cleanup.as_ref() {
            | Ok(SandboxResourceDisposition::Removed) => remove_workspace(&self.workspace),
            | _ => Ok(()),
        };
        combine_errors([artifacts, cleanup.map(|_| ()), workspace])
    }
    fn observed_memory_peak(&self) -> Result<Option<Memory>, SandboxError> {
        #[cfg(target_os = "linux")]
        {
            self.process_id()
                .and_then(|process_id| memory::observed_peak(process_id).map_err(SandboxError::Runtime))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(None)
        }
    }
    fn prepared_config(&self) -> &Config {
        &self.config
    }
    fn prepared_input(&self) -> Option<&str> {
        self.input.as_deref()
    }
    fn resources(&self) -> &SandboxResources {
        &self.policy.policy.resources
    }
}
impl Drop for OciResource {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}
impl OciResource {
    fn finish(&self, succeeded: bool) -> Result<SandboxResourceDisposition, SandboxError> {
        self.terminal
            .lock()
            .map_err(|_| SandboxError::Runtime(format!("sandbox resource {} lock was poisoned", self.identifier)))
            .and_then(|mut terminal| match *terminal {
                | Some(status) => Ok(status),
                | None => {
                    let cleanup = self.policy.cleanup(&self.identifier, succeeded);
                    let (status, error) = match &cleanup {
                        | Ok(status) => (*status, None),
                        | Err(why) => (SandboxResourceDisposition::Failed, Some(why.to_string())),
                    };
                    let tracked = self.tracker.finalize(&self.identifier, status, error);
                    *terminal = Some(status);
                    match (cleanup, tracked) {
                        | (Ok(status), Ok(())) => Ok(status),
                        | (Err(primary), Ok(())) => Err(primary),
                        | (Ok(_), Err(tracking)) => Err(tracking),
                        | (Err(primary), Err(tracking)) => Err(SandboxError::Runtime(format!("{primary}; additionally, {tracking}"))),
                    }
                }
            })
    }
}
impl SandboxInput<Requested> {
    pub(super) fn verify_binary_checksums(&self, prepared_root: &Path) -> Result<(), SandboxError> {
        self.module_indices()
            .iter()
            .try_for_each(|index| match self.config().modules.get(*index) {
                | Some(Module {
                    module_type: ModuleType::Binary { checksum, .. },
                    name,
                    ..
                }) => checksum.as_deref().map_or(Ok(()), |expected| {
                    file_checksum(prepared_root.join("modules").join(index.to_string()).join("module"), None)
                        .map_err(|why| SandboxError::Runtime(format!("calculate sandbox binary checksum for module {name}: {why}")))
                        .and_then(|calculated| {
                            let expected = Checksum::from(expected);
                            match expected.algorithm == calculated.algorithm && expected.checksum_value == calculated.checksum_value {
                                | true => Ok(()),
                                | false => Err(SandboxError::Validation(format!(
                                    "module {name} checksum {expected} does not match {}:{}",
                                    calculated.algorithm, calculated.checksum_value
                                ))),
                            }
                        })
                }),
                | _ => Ok(()),
            })
    }
}
pub(super) fn archive_export_arguments(container_name: &str, source: &str) -> Vec<OsString> {
    args!["exec", container_name, "tar", "-C", source, "-cf", SANDBOX_EXPORT_PATH, "."]
}
pub(super) fn archive_mount(path: &Path) -> String {
    format!("type=bind,source={},target={SANDBOX_EXPORT_PATH}", path.display())
}
pub(super) fn combine_errors<const N: usize>(results: [Result<(), SandboxError>; N]) -> Result<(), SandboxError> {
    let errors = results.into_iter().filter_map(Result::err).map(|why| why.to_string()).collect::<Vec<_>>();
    match errors.is_empty() {
        | true => Ok(()),
        | false => Err(SandboxError::Runtime(errors.join("; additionally, "))),
    }
}
pub(super) fn remove_workspace(path: &Path) -> Result<(), SandboxError> {
    match remove_dir_all(path) {
        | Ok(()) => Ok(()),
        | Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
        | Err(why) => Err(SandboxError::Runtime(format!(
            "remove sandbox staging workspace {}: {why}",
            path.display()
        ))),
    }
}
pub(super) fn start(session: &OciSandboxSession, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError> {
    let workspace = request.staging_root().join(format!("input-{}", request.input_index()));
    session
        .policy
        .validate_workflow(request.config(), &[request.input().map(str::to_string)])
        .and_then(|()| {
            session
                .artifacts
                .prepare(request.config(), request.input(), request.input_index(), request.input_count())
        })
        .and_then(|artifact| {
            create_dir_all(request.staging_root())
                .map_err(|why| SandboxError::Runtime(format!("create sandbox staging root {}: {why}", request.staging_root().display())))
                .map(|()| artifact)
        })
        .and_then(|artifact| {
            create_dir(&workspace)
                .map_err(|why| SandboxError::Runtime(format!("create sandbox input workspace {}: {why}", workspace.display())))
                .map(|()| artifact)
        })
        .and_then(|artifact| {
            let archive_path = workspace.join("prepared.tar");
            let container_name = format!("xylem-{}-{}-prep", session.run_id, request.input_index());
            let resource = OciResourceSpec {
                artifact_staging: artifact.as_ref().and_then(|artifact| artifact.archive.parent()).and_then(Path::parent),
                input_index: request.input_index(),
                name: &container_name,
                phase: SandboxResourcePhase::Preparation,
                run_id: &session.run_id,
                tracker: Arc::clone(&session.resources),
            };
            archive::create_export(&archive_path)
                .map_err(|why| SandboxError::Runtime(format!("create prepared archive {}: {why}", archive_path.display())))
                .and_then(|()| session.policy.create_preparation_container(resource, &request, &archive_path))
                .map(|resource| {
                    let preparation = OciPreparation {
                        artifact,
                        artifacts: Arc::clone(&session.artifacts),
                        policy: session.policy.clone(),
                        prepared: BTreeSet::new(),
                        request,
                        resource,
                        run_id: session.run_id.clone(),
                        workspace: workspace.clone(),
                        workspace_transferred: false,
                    };
                    Box::new(preparation) as Box<dyn SandboxPreparation>
                })
        })
        .inspect_err(|_| {
            let _ = remove_dir_all(&workspace);
        })
}
