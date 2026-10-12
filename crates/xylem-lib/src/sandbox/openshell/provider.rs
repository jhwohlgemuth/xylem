extern crate alloc;

use super::super::artifact::{ArtifactInput, ArtifactManager};
use super::super::lifecycle::{combine_errors, remove_workspace, PREPARATION_ENVIRONMENT};
use super::super::{
    archive, private, reconcile, rewrite_path, EffectiveSandboxPolicy, IsolationClass, OciRuntime, PreparationNetwork, Prepared,
    PreparedSandboxBackend, Requested, SandboxCapabilities, SandboxError, SandboxInput, SandboxLease, SandboxPreparation, SandboxProvider,
    SandboxProviderClient, SandboxProviderKind, SandboxReconciliationOutcome, SandboxResourceDisposition, SandboxResourceEvidence,
    SandboxResourcePhase, SandboxResourceTracker, SandboxResources, SandboxRetention, SandboxSession, SandboxSessionOutcome, WorkloadAcquisition,
    SANDBOX_MANAGED_LABEL, SANDBOX_PROVIDER_LABEL, SANDBOX_RESOURCE_VERSION, SANDBOX_VERSION_LABEL,
};
use super::{
    OpenShellClient, OpenShellCreateRequest, OpenShellDriver, OpenShellIdentity, OpenShellNetworkAccess, OpenShellNetworkEndpoint, OpenShellPolicy,
    POLICY_DIGEST_FIRST_LABEL, POLICY_DIGEST_SECOND_LABEL,
};
use crate::config::Config;
use crate::error::ProcessError;
use crate::script::{Module, ModuleLanguage, ModuleType, ModuleUri, Script, VirtualEnvironmentManager};
use crate::util::timestamp;
use acorn_cmd::{ExecutionOptions, ExecutionResult};
use acorn_core::time::UnitOfTime;
use acorn_schema::hardware::Memory;
use acorn_schema::validation::Validate;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::time::Duration;
use std::fs::{create_dir, create_dir_all, read_dir, read_to_string, symlink_metadata, File};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tar::Builder;
use url::Url;

struct OpenShellExecutionRequest {
    command: Vec<String>,
    environment: BTreeMap<String, String>,
    workdir: String,
}
struct OpenShellPreparation {
    artifact: Option<ArtifactInput>,
    artifacts: Arc<ArtifactManager>,
    client: OpenShellClient,
    policy: EffectiveSandboxPolicy,
    prepared: BTreeSet<usize>,
    request: SandboxInput<Requested>,
    resource: OpenShellResource,
    run_id: String,
    workspace: PathBuf,
    workspace_transferred: bool,
}
struct OpenShellPreparedInput {
    artifact: Option<ArtifactInput>,
    artifacts: Arc<ArtifactManager>,
    client: OpenShellClient,
    config: Config,
    input: Option<String>,
    policy: EffectiveSandboxPolicy,
    prepared_root: PathBuf,
    resource: OpenShellResource,
    workspace: PathBuf,
}
pub(crate) struct OpenShellProvider<'a> {
    policy: &'a EffectiveSandboxPolicy,
}
struct OpenShellResource {
    client: OpenShellClient,
    identity: OpenShellIdentity,
    retention: SandboxRetention,
    terminal: Mutex<Option<SandboxResourceDisposition>>,
    tracker: Arc<SandboxResourceTracker>,
}
struct OpenShellSession {
    artifacts: Arc<ArtifactManager>,
    client: OpenShellClient,
    policy: EffectiveSandboxPolicy,
    resources: Arc<SandboxResourceTracker>,
    run_id: String,
}
impl EffectiveSandboxPolicy {
    fn openshell_driver(&self) -> Result<OpenShellDriver, SandboxError> {
        match self.runtime {
            | Some(OciRuntime::Docker) => Ok(OpenShellDriver::Docker),
            | Some(OciRuntime::Podman) => Ok(OpenShellDriver::Podman),
            | None => Err(SandboxError::Validation(
                "OpenShell sandbox policy does not select a compute driver".to_string(),
            )),
        }
    }
    fn openshell_policy_selector(&self) -> Result<String, SandboxError> {
        super::policy_digest_hex(&self.digest).map(|digest| {
            [
                (SANDBOX_MANAGED_LABEL, "true".to_string()),
                (SANDBOX_PROVIDER_LABEL, "openshell".to_string()),
                (SANDBOX_VERSION_LABEL, SANDBOX_RESOURCE_VERSION.to_string()),
                (POLICY_DIGEST_FIRST_LABEL, digest[..32].to_string()),
                (POLICY_DIGEST_SECOND_LABEL, digest[32..].to_string()),
            ]
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",")
        })
    }
    fn openshell_preparation_access(&self, request: &SandboxInput<Requested>) -> Result<OpenShellNetworkAccess, SandboxError> {
        match self.policy.preparation_network {
            | PreparationNetwork::Deny => Ok(OpenShellNetworkAccess::Deny),
            | PreparationNetwork::Unrestricted => request
                .module_indices()
                .iter()
                .filter_map(|index| request.config().modules.get(*index))
                .filter_map(|module| match &module.module_type {
                    | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri, .. } | ModuleType::Script { uri, .. } => {
                        match ModuleUri::from(uri) {
                            | ModuleUri::Url(url) => Some(url.to_string()),
                            | ModuleUri::File(_) | ModuleUri::Unknown(_) => None,
                        }
                    }
                })
                .map(|value| {
                    Url::parse(&value)
                        .map_err(|why| SandboxError::Validation(format!("parse OpenShell preparation URL {value}: {why}")))
                        .and_then(|url| {
                            url.host_str()
                                .map(str::to_string)
                                .zip(url.port_or_known_default())
                                .map(|(host, port)| OpenShellNetworkEndpoint { host, port })
                                .ok_or_else(|| SandboxError::Validation(format!("OpenShell preparation URL {value} has no host or port")))
                        })
                })
                .collect::<Result<BTreeSet<_>, _>>()
                .map(|endpoints| match endpoints.is_empty() {
                    | true => OpenShellNetworkAccess::Deny,
                    | false => OpenShellNetworkAccess::ExactEndpoints(endpoints.into_iter().collect()),
                }),
        }
    }
    fn reconcile_openshell_resources(&self, leases: &[SandboxLease]) -> SandboxReconciliationOutcome {
        let lease_states = leases
            .iter()
            .map(|lease| (lease.run_id.as_str(), lease.state))
            .collect::<BTreeMap<_, _>>();
        let cutoff = timestamp()
            .get()
            .saturating_sub(self.policy.lifecycle.stale_after_seconds.saturating_mul(1000));
        let sandboxes = self.openshell_driver().map(OpenShellClient::new).and_then(|client| {
            self.openshell_policy_selector()
                .and_then(|selector| client.list_sandboxes_by_selector(&selector))
                .map(|sandboxes| (client, sandboxes))
        });
        match sandboxes {
            | Err(why) => SandboxReconciliationOutcome {
                error: Some(why.to_string()),
                resources: Vec::new(),
            },
            | Ok((client, sandboxes)) => {
                let mut errors = Vec::new();
                let mut resources = Vec::new();
                sandboxes.into_iter().for_each(|sandbox| match sandbox.identity(self) {
                    | Err(why) => errors.push(why.to_string()),
                    | Ok((identity, created_at, run_id, input_index, phase))
                        if reconcile::is_eligible(created_at, cutoff, lease_states.get(run_id.as_str()).copied()) =>
                    {
                        match client.delete_sandbox_and_wait(&identity) {
                            | Ok(()) => resources.push(SandboxResourceEvidence {
                                error: None,
                                identifier: identity.name,
                                input_index,
                                phase,
                                status: SandboxResourceDisposition::Reconciled,
                            }),
                            | Err(why) => {
                                errors.push(why.to_string());
                                resources.push(SandboxResourceEvidence {
                                    error: Some(why.to_string()),
                                    identifier: identity.name,
                                    input_index,
                                    phase,
                                    status: SandboxResourceDisposition::Failed,
                                });
                            }
                        }
                    }
                    | Ok(_) => {}
                });
                resources.sort_by(|left, right| {
                    (left.input_index, left.phase, &left.identifier).cmp(&(right.input_index, right.phase, &right.identifier))
                });
                SandboxReconciliationOutcome {
                    error: (!errors.is_empty()).then(|| errors.join("; additionally, ")),
                    resources,
                }
            }
        }
    }
}
impl OpenShellClient {
    fn execute_command(&self, sandbox: &str, workdir: &str, environment: &BTreeMap<String, String>, command: &[&str]) -> Result<(), SandboxError> {
        let command = command.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();
        self.execute_in_sandbox_checked_output(sandbox, workdir, environment, &command)
    }
    fn prepare_module(&self, sandbox: &str, workspace: &Path, module_index: usize, module: &Module) -> Result<(), SandboxError> {
        let destination = format!("/xylem/prepared/modules/{module_index}");
        match &module.module_type {
            | ModuleType::Configuration { .. } => Err(SandboxError::Validation(format!(
                "configuration module {} cannot be prepared in sandbox mode",
                module.name
            ))),
            | ModuleType::Binary { uri, .. } => Ok((uri, true, false)),
            | ModuleType::Script { programming_language, uri } => Ok((uri, false, matches!(programming_language, Some(ModuleLanguage::Python)))),
        }
        .and_then(|(uri, binary, python)| {
            let acquisition = match ModuleUri::from(uri) {
                | ModuleUri::File(_) => module.sandbox_local_source().and_then(|source| {
                    source
                        .ok_or_else(|| SandboxError::Validation(format!("module {} has no local source", module.name)))
                        .and_then(|source| {
                            let archive = workspace.join(format!("module-{module_index}.tar"));
                            archive_local_module(&source, &archive, binary)
                                .and_then(|()| self.execute_command(sandbox, "/xylem/prepared", &BTreeMap::new(), &["mkdir", "-p", &destination]))
                                .and_then(|()| self.upload_file(sandbox, &archive, &format!("/xylem/input/module-{module_index}.tar")))
                                .and_then(|()| {
                                    self.execute_command(
                                        sandbox,
                                        "/xylem/prepared",
                                        &BTreeMap::new(),
                                        &["tar", "-C", &destination, "-xf", &format!("/xylem/input/module-{module_index}.tar")],
                                    )
                                })
                        })
                }),
                | ModuleUri::Url(_) => Err(SandboxError::Validation(format!(
                    "module {} remote source was not materialized before sandbox preparation",
                    module.name
                ))),
                | ModuleUri::Unknown(value) => Err(SandboxError::Validation(format!("module {} has invalid URI {value}", module.name))),
            };
            acquisition
                .and_then(|()| match binary {
                    | true => self.execute_command(
                        sandbox,
                        "/xylem/prepared",
                        &BTreeMap::new(),
                        &["chmod", "0755", &format!("{destination}/module")],
                    ),
                    | false => Ok(()),
                })
                .and_then(|()| match python {
                    | true => self.execute_command(sandbox, &destination, &BTreeMap::new(), &["sh", "-c", PREPARATION_ENVIRONMENT]),
                    | false => Ok(()),
                })
        })
    }
    fn upload_tree(&self, sandbox: &str, source: &Path, remote: &str) -> Result<(), SandboxError> {
        transfer_files(source).and_then(|files| match files.is_empty() {
            | true => Err(SandboxError::Validation(format!(
                "OpenShell cannot transfer empty directory {}",
                source.display()
            ))),
            | false => files.into_iter().try_for_each(|(local, relative)| {
                let destination = match relative.as_os_str().is_empty() {
                    | true => remote.to_string(),
                    | false => format!("{}/{}", remote.trim_end_matches('/'), relative.to_string_lossy()),
                };
                destination
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .filter(|parent| !parent.is_empty())
                    .map_or(Ok(()), |parent| {
                        self.execute_command(sandbox, "/xylem", &BTreeMap::new(), &["mkdir", "-p", parent])
                    })
                    .and_then(|()| self.upload_file(sandbox, &local, &destination))
            }),
        })
    }
}
impl Drop for OpenShellPreparation {
    fn drop(&mut self) {
        if !self.workspace_transferred && matches!(self.resource.finish(false), Ok(SandboxResourceDisposition::Removed)) {
            let _ = remove_workspace(&self.workspace);
        }
    }
}
impl private::Sealed for OpenShellPreparation {}
impl SandboxPreparation for OpenShellPreparation {
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
            | (true, false, Some(module)) => self
                .client
                .prepare_module(&self.resource.identity.name, &self.workspace, module_index, module)
                .map(|()| {
                    self.prepared.insert(module_index);
                }),
        }
    }
    fn seal(mut self: Box<Self>) -> Result<SandboxInput<Prepared>, SandboxError> {
        match self.prepared.len() == self.request.module_indices().len() {
            | false => Err(SandboxError::Validation("not every required sandbox module was prepared".to_string())),
            | true => {
                let archive_path = self.workspace.join("prepared.tar");
                self.client
                    .execute_command(
                        &self.resource.identity.name,
                        "/xylem/prepared",
                        &BTreeMap::new(),
                        &["tar", "-C", "/xylem/prepared", "-cf", "/xylem/tmp/prepared.tar", "."],
                    )
                    .and_then(|()| {
                        self.client
                            .download_file(&self.resource.identity.name, "/xylem/tmp/prepared.tar", &archive_path)
                    })
                    .and_then(|()| self.resource.finish(true).map(|_| ()))
                    .and_then(|()| {
                        let prepared_root = self.workspace.join("prepared");
                        archive::extract(
                            &archive_path,
                            &prepared_root,
                            self.policy.policy.resources.scratch_bytes,
                            self.policy.policy.resources.artifact_files,
                        )
                        .map_err(|why| SandboxError::Runtime(format!("validate prepared OpenShell archive: {why}")))
                        .map(|()| prepared_root)
                    })
                    .and_then(|prepared_root| {
                        self.request.verify_binary_checksums(&prepared_root).and_then(|()| {
                            let config = self.request.config().clone().with_sandbox_paths(
                                &prepared_root,
                                self.artifact.is_some(),
                                self.request.module_indices(),
                            );
                            let execution_policy = self.workspace.join("execution-policy.yaml");
                            OpenShellPolicy::write(&execution_policy, SandboxResourcePhase::Execution, OpenShellNetworkAccess::Deny)
                                .and_then(|()| {
                                    let identity = OpenShellIdentity::new(
                                        &self.run_id,
                                        self.request.input_index(),
                                        SandboxResourcePhase::Execution,
                                        &self.policy.digest,
                                        timestamp().get(),
                                    );
                                    identity.and_then(|identity| {
                                        create_resource(
                                            &self.client,
                                            &self.policy,
                                            identity,
                                            &execution_policy,
                                            Arc::clone(&self.resource.tracker),
                                            self.request.input_index(),
                                            SandboxResourcePhase::Execution,
                                        )
                                    })
                                })
                                .and_then(|resource| {
                                    self.client
                                        .upload_tree(&resource.identity.name, &prepared_root, "/xylem/prepared")
                                        .and_then(|()| {
                                            self.request.input().map_or(Ok(()), |input| {
                                                self.client.upload_tree(&resource.identity.name, Path::new(input), "/xylem/input/current")
                                            })
                                        })
                                        .map(|()| resource)
                                })
                                .map(|resource| {
                                    self.workspace_transferred = true;
                                    SandboxInput::from_backend(Arc::new(OpenShellPreparedInput {
                                        artifact: self.artifact.clone(),
                                        artifacts: Arc::clone(&self.artifacts),
                                        client: self.client.clone(),
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
                self.client
                    .execute_command(
                        &self.resource.identity.name,
                        "/xylem/prepared",
                        &BTreeMap::new(),
                        &["sh", "-c", "command -v \"$1\" >/dev/null 2>&1", "xylem-requirement", requirement],
                    )
                    .map_err(|why| SandboxError::Runtime(format!("sandbox requirement {requirement} is unavailable: {why}")))
            }),
        }
    }
}
impl Drop for OpenShellPreparedInput {
    fn drop(&mut self) {
        if matches!(self.resource.finish(false), Ok(SandboxResourceDisposition::Removed)) {
            let _ = remove_workspace(&self.workspace);
        }
    }
}
impl OpenShellPreparedInput {
    fn execution_request(&self, script: &Script, module_index: usize) -> Result<OpenShellExecutionRequest, SandboxError> {
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
            .map(|manager| VirtualEnvironmentManager::from(manager.trim()))
            .filter(VirtualEnvironmentManager::supports_sandbox);
        let mut script = script.clone();
        if let (Some(manager), Some(environment)) = (manager.as_ref(), script.command.virtual_environment.as_mut()) {
            environment.manager = manager.clone();
        }
        script.validate().map_err(|why| SandboxError::Validation(why.to_string())).map(|()| {
            let mut environment = script
                .envs
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|(name, _)| self.policy.policy.environment.allow.contains(name))
                .map(|(name, value)| (name, rewrite_path(&value, &replacements)))
                .collect::<BTreeMap<_, _>>();
            environment.insert("HOME".to_string(), "/xylem/tmp".to_string());
            environment.insert("TMPDIR".to_string(), "/xylem/tmp".to_string());
            if matches!(manager, Some(VirtualEnvironmentManager::Conda | VirtualEnvironmentManager::Mamba)) {
                environment.insert(
                    "CONDA_ENVS_PATH".to_string(),
                    format!("/xylem/prepared/modules/{module_index}/.xylem-conda/envs"),
                );
                environment.insert("CONDA_PKGS_DIRS".to_string(), "/xylem/tmp/conda-pkgs".to_string());
            }
            let command = core::iter::once(rewrite_path(&script.get_command(), &replacements))
                .chain(script.get_arguments().into_iter().map(|argument| rewrite_path(&argument, &replacements)))
                .collect();
            OpenShellExecutionRequest {
                command,
                environment,
                workdir: rewrite_path(&script.current_dir, &replacements),
            }
        })
    }
}
impl PreparedSandboxBackend for OpenShellPreparedInput {
    fn execute(&self, script: &Script, module_index: usize, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError> {
        let working_directory = PathBuf::from(&script.current_dir);
        self.execution_request(script, module_index)
            .map_err(|why| ProcessError::Spawn {
                script: script.to_string(),
                working_directory: working_directory.clone(),
                source: std::io::Error::other(why),
            })
            .and_then(|request| {
                self.client
                    .execute_in_sandbox_checked(
                        &self.resource.identity.name,
                        &request.workdir,
                        &request.environment,
                        &request.command,
                        options,
                    )
                    .map_err(|source| script.sandbox_process_error(&working_directory, source))
            })
    }
    fn finish(&self, succeeded: bool) -> Result<(), SandboxError> {
        let artifacts = match (&self.artifact, succeeded) {
            | (Some(artifact), true) => self
                .client
                .execute_command(
                    &self.resource.identity.name,
                    "/xylem/output",
                    &BTreeMap::new(),
                    &["tar", "-C", "/xylem/output", "-cf", "/xylem/tmp/output.tar", "."],
                )
                .and_then(|()| {
                    self.client
                        .download_file(&self.resource.identity.name, "/xylem/tmp/output.tar", &artifact.archive)
                })
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
        Ok(None)
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
impl private::Sealed for OpenShellProvider<'_> {}
impl SandboxProvider for OpenShellProvider<'_> {
    fn discover(&self) -> Result<SandboxCapabilities, SandboxError> {
        self.policy
            .openshell_driver()
            .map(OpenShellClient::new)
            .and_then(|client| client.discover())
            .map(|_| SandboxCapabilities {
                isolation: IsolationClass::SharedKernel,
                network_deny: true,
                provider: SandboxProviderKind::OpenShell,
                read_only_root: true,
                resource_limits: false,
                rootless: false,
            })
            .and_then(|capabilities| match capabilities.isolation >= self.policy.policy.minimum_isolation {
                | true => Ok(capabilities),
                | false => Err(SandboxError::Runtime(format!(
                    "provider isolation {:?} does not satisfy required {:?}",
                    capabilities.isolation, self.policy.policy.minimum_isolation
                ))),
            })
    }
    fn reconcile(&self, leases: &[SandboxLease]) -> SandboxReconciliationOutcome {
        self.policy.reconcile_openshell_resources(leases)
    }
    fn open(&self, run_id: &str, _acquisition: WorkloadAcquisition) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        match run_id.is_empty() {
            | true => Err(SandboxError::Validation("sandbox run identifier must not be empty".to_string())),
            | false => self.policy.openshell_driver().map(|driver| {
                Arc::new(OpenShellSession {
                    artifacts: ArtifactManager::new(
                        run_id.to_string(),
                        self.policy.policy.paths.artifact_roots.clone(),
                        self.policy.policy.resources.clone(),
                    ),
                    client: OpenShellClient {
                        execution_timeout: Duration::from_secs(self.policy.policy.resources.wall_time_seconds),
                        ..OpenShellClient::new(driver)
                    },
                    policy: self.policy.clone(),
                    resources: Arc::new(SandboxResourceTracker::default()),
                    run_id: run_id.to_string(),
                }) as Arc<dyn SandboxSession>
            }),
        }
    }
}
impl<'a> OpenShellProvider<'a> {
    pub(crate) fn new(policy: &'a EffectiveSandboxPolicy) -> Self {
        Self { policy }
    }
}
impl Drop for OpenShellResource {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}
impl OpenShellResource {
    fn finish(&self, succeeded: bool) -> Result<SandboxResourceDisposition, SandboxError> {
        self.terminal
            .lock()
            .map_err(|_| SandboxError::Runtime(format!("sandbox resource {} lock was poisoned", self.identity.name)))
            .and_then(|mut terminal| match *terminal {
                | Some(status) => Ok(status),
                | None => {
                    let cleanup = match succeeded || self.retention == SandboxRetention::Delete {
                        | true => self
                            .client
                            .delete_sandbox_and_wait(&self.identity)
                            .map(|()| SandboxResourceDisposition::Removed),
                        | false => Ok(SandboxResourceDisposition::Retained),
                    };
                    let (status, error) = match &cleanup {
                        | Ok(status) => (*status, None),
                        | Err(why) => (SandboxResourceDisposition::Failed, Some(why.to_string())),
                    };
                    let tracked = self.tracker.finalize(&self.identity.name, status, error);
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
impl OpenShellSession {
    fn start_input(&self, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError> {
        let workspace = request.staging_root().join(format!("input-{}", request.input_index()));
        self.policy
            .validate_workflow(request.config(), &[request.input().map(str::to_string)])
            .and_then(|()| {
                self.artifacts
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
                let policy_path = workspace.join("preparation-policy.yaml");
                self.policy
                    .openshell_preparation_access(&request)
                    .and_then(|access| OpenShellPolicy::write(&policy_path, SandboxResourcePhase::Preparation, access))
                    .and_then(|()| {
                        OpenShellIdentity::new(
                            &self.run_id,
                            request.input_index(),
                            SandboxResourcePhase::Preparation,
                            &self.policy.digest,
                            timestamp().get(),
                        )
                    })
                    .and_then(|identity| {
                        create_resource(
                            &self.client,
                            &self.policy,
                            identity,
                            &policy_path,
                            Arc::clone(&self.resources),
                            request.input_index(),
                            SandboxResourcePhase::Preparation,
                        )
                    })
                    .map(|resource| {
                        Box::new(OpenShellPreparation {
                            artifact,
                            artifacts: Arc::clone(&self.artifacts),
                            client: self.client.clone(),
                            policy: self.policy.clone(),
                            prepared: BTreeSet::new(),
                            request,
                            resource,
                            run_id: self.run_id.clone(),
                            workspace: workspace.clone(),
                            workspace_transferred: false,
                        }) as Box<dyn SandboxPreparation>
                    })
            })
            .inspect_err(|_| {
                let _ = remove_workspace(&workspace);
            })
    }
}
impl private::Sealed for OpenShellSession {}
impl SandboxSession for OpenShellSession {
    fn start_input(&self, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError> {
        OpenShellSession::start_input(self, request)
    }
    fn finish(&self, succeeded: bool) -> SandboxSessionOutcome {
        let artifacts = self.artifacts.finish(succeeded);
        let resources = self.resources.evidence();
        let errors = [
            artifacts.as_ref().err().map(ToString::to_string),
            resources.as_ref().err().map(ToString::to_string),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        SandboxSessionOutcome {
            artifacts: artifacts.ok().flatten(),
            error: (!errors.is_empty()).then(|| errors.join("; additionally, ")),
            resources: resources.unwrap_or_default(),
        }
    }
    fn resources(&self) -> &SandboxResources {
        &self.policy.policy.resources
    }
}

fn archive_local_module(source: &Path, destination: &Path, binary: bool) -> Result<(), SandboxError> {
    File::create(destination)
        .map_err(|why| SandboxError::Runtime(format!("create OpenShell module archive {}: {why}", destination.display())))
        .and_then(|file| {
            let mut builder = Builder::new(file);
            builder.follow_symlinks(false);
            match binary {
                | true => builder.append_path_with_name(source, "module"),
                | false => builder.append_dir_all(".", source),
            }
            .and_then(|()| builder.finish())
            .map_err(|why| SandboxError::Runtime(format!("write OpenShell module archive {}: {why}", destination.display())))
        })
}
fn create_resource(
    client: &OpenShellClient,
    policy: &EffectiveSandboxPolicy,
    identity: OpenShellIdentity,
    policy_path: &Path,
    tracker: Arc<SandboxResourceTracker>,
    input_index: usize,
    phase: SandboxResourcePhase,
) -> Result<OpenShellResource, SandboxError> {
    let command = ["sleep".to_string(), "infinity".to_string()];
    let request = OpenShellCreateRequest {
        command: &command,
        identity: &identity,
        policy: policy_path,
        resources: &policy.policy.resources,
        workload: &policy.workload,
    };
    let created = client.create_sandbox(&request).and_then(|created| match created.name == identity.name {
        | true => client.wait_ready(&identity),
        | false => Err(SandboxError::Runtime("OpenShell create returned an unexpected sandbox name".to_string())),
    });
    let ready = created.and_then(|sandbox| match sandbox.policy.is_some() {
        | true => Ok(sandbox),
        | false => Err(SandboxError::Runtime("OpenShell sandbox inspection omitted effective policy".to_string())),
    });
    ready
        .and_then(|_| tracker.register(identity.name.clone(), input_index, phase))
        .map(|()| OpenShellResource {
            client: client.clone(),
            identity: identity.clone(),
            retention: policy.policy.lifecycle.retention,
            terminal: Mutex::new(None),
            tracker,
        })
        .inspect_err(|_| {
            let _ = client.delete_sandbox_and_wait(&identity);
        })
}
fn transfer_files(source: &Path) -> Result<Vec<(PathBuf, PathBuf)>, SandboxError> {
    fn collect(root: &Path, path: &Path) -> Result<Vec<(PathBuf, PathBuf)>, SandboxError> {
        symlink_metadata(path)
            .map_err(|why| SandboxError::Runtime(format!("inspect OpenShell transfer path {}: {why}", path.display())))
            .and_then(|metadata| {
                if metadata.file_type().is_symlink() {
                    Err(SandboxError::Validation(format!(
                        "OpenShell transfer path {} contains a symbolic link",
                        path.display()
                    )))
                } else if metadata.is_file() {
                    path.strip_prefix(root)
                        .map(|relative| vec![(path.to_path_buf(), relative.to_path_buf())])
                        .map_err(|why| SandboxError::Runtime(format!("resolve OpenShell transfer path {}: {why}", path.display())))
                } else if metadata.is_dir() {
                    read_dir(path)
                        .map_err(|why| SandboxError::Runtime(format!("read OpenShell transfer directory {}: {why}", path.display())))
                        .and_then(|entries| {
                            entries
                                .map(|entry| entry.map(|entry| entry.path()).map_err(|why| SandboxError::Runtime(why.to_string())))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .and_then(|mut paths| {
                            paths.sort();
                            paths.into_iter().try_fold(Vec::new(), |mut files, path| {
                                collect(root, &path).map(|nested| {
                                    files.extend(nested);
                                    files
                                })
                            })
                        })
                } else {
                    Err(SandboxError::Validation(format!(
                        "OpenShell transfer path {} is not a regular file or directory",
                        path.display()
                    )))
                }
            })
    }
    collect(source, source)
}
