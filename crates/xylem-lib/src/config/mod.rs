extern crate alloc;

use crate::acquisition::{
    AcquisitionCache, AcquisitionError, AcquisitionEvidence, AcquisitionExecutor, AcquisitionLimits, AcquisitionPlan, AcquisitionPolicy,
    AcquisitionSource,
};
use crate::completion::{CompletionDecision, CompletionPlan};
use crate::error::{ProcessError, RequirementError};
use crate::execution::{ExecutionContext, InputBatch};
use crate::journal::{ExecutionState, RunJournal};
use crate::outcome::{ModuleOutcome, WorkflowOutcome};
use crate::plan::PlanError;
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState};
use crate::run::{ContextError, WorkflowContext};
use crate::sandbox::EffectiveSandboxPolicy;
use crate::script::{
    module_working_directory, Command, ContainerCleanup, ContainerEnvironment, ContainerRuntime, Module, ModuleLanguage, ModuleType, ModuleUri,
    Script, StringTemplate, TemplateAttribute, VirtualEnvironment, VirtualEnvironmentManager,
};
pub use crate::selection::{InputSelection, SelectionEngine, SelectionOptions, SelectionRequest};
use crate::workspace::{materialize, InputWorkspace};
use acorn_core::time::Seconds;
use acorn_core::util::{MimeType, ParseOptionsExt, SemanticVersion, StringConversion};
use acorn_core::{Location, Scheme};
use acorn_host::config::{read_jsonc, ConfigError as AcornConfigError};
use acorn_host::fs::{make_executable, SafePath};
use acorn_host::terminal::Label;
use acorn_macros::With;
use acorn_schema::hardware::Memory;
use alloc::collections::{BTreeMap, BTreeSet};
use bon::Builder;
use convert_case::{Case, Casing};
use core::fmt;
use core::iter::once;
use jsonc_parser::{parse_to_serde_value, ParseOptions};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs::{canonicalize, create_dir_all, read_to_string};
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use url::Url;

mod arguments;
pub(crate) mod interpolation;
mod overrides;
mod validation;
pub use arguments::WorkflowArgumentsError;
pub use overrides::{VariableOverrideError, VariableOverrides};
const RESERVED_VARIABLES: [&str; 5] = ["INPUT", "INPUT_BASENAME", "MODULE_LIST", "RUN_ID", "WORK_DIRECTORY"];
/// Executes one resolved module command for an orchestration adapter.
pub trait ModuleExecutor: Send + Sync {
    /// Execute a script for the stable input and module positions, applying the validated module timeout to the managed process when one is present.
    fn execute(&self, script: &Script, input_index: usize, module_index: usize, timeout: Option<Seconds>) -> Result<(), ProcessError>;

    /// Return whether the caller has requested cancellation.
    fn is_cancellation_requested(&self) -> bool {
        false
    }
}
/// Supported workflow configuration serialization formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFormat {
    /// JavaScript Object Notation.
    Json,
    /// JavaScript Object Notation with comments and trailing commas.
    Jsonc,
    /// YAML Ain't Markup Language.
    Yaml,
}
/// Failure to select or deserialize a workflow configuration file.
#[derive(Debug)]
pub enum ConfigReadError {
    /// The selected configuration could not be deserialized.
    Invalid {
        /// Configuration format selected from the path.
        format: ConfigFormat,
        /// Configuration path that failed to deserialize.
        path: PathBuf,
        /// Parser or semantic deserialization failure.
        source: Box<dyn core::error::Error + Send + Sync>,
    },
    /// The selected configuration could not be read.
    Read {
        /// Configuration format selected from the path.
        format: ConfigFormat,
        /// Configuration path that could not be read.
        path: PathBuf,
        /// Filesystem failure.
        source: std::io::Error,
    },
    /// The path does not use a supported configuration extension.
    UnsupportedExtension(PathBuf),
}
/// Failure while resolving nested configuration modules.
#[derive(Debug, thiserror::Error)]
pub enum ConfigurationModuleError {
    /// A module URI contains an invalid structural interpolation.
    #[error("{0}")]
    Interpolation(#[from] ConfigurationInterpolationError),
    /// A configuration module exceeds the supported nesting depth.
    #[error("configuration module nesting exceeds 16 levels at {}", path.join("::"))]
    Depth { path: Vec<String> },
    /// A configuration source recurs through its ancestor chain.
    #[error("configuration module cycle at {} ({location})", path.join("::"))]
    Cycle { path: Vec<String>, location: String },
    /// A nested workflow declares an invalid remote policy.
    #[error("configuration module {module} has invalid remote policy: {reason}")]
    Policy { module: String, reason: String },
    /// A configuration module uses an unsupported source.
    #[error("configuration module {module} has unsupported source {uri}")]
    Source { module: String, uri: String },
    /// A wrapper declares a field owned by executable leaves.
    #[error("configuration module {module} does not accept {field}")]
    Wrapper { module: String, field: &'static str },
    /// The referenced configuration could not be read.
    #[error("read configuration module {module}: {source}")]
    Read {
        module: String,
        #[source]
        source: ConfigReadError,
    },
    /// A remote nested configuration cannot be resolved in a network-free dry run.
    #[error("remote configuration module {module} cannot be resolved during dry run: {uri}")]
    RemoteDryRun { module: String, uri: String },
    /// Remote configuration acquisition failed before execution began.
    #[error("resolve remote configuration module {module}: {reason}")]
    Remote { module: String, reason: String },
    /// A direct HTTP configuration document declares a Git-only revision.
    #[error("configuration module {module} cannot apply revision to direct document {uri}")]
    Revision { module: String, uri: String },
}
#[derive(Clone, Debug)]
enum ConfigurationSource {
    File(PathBuf),
    Git { identity: String, path: PathBuf },
    Http { identity: String },
    Oci { identity: String, path: PathBuf },
}
/// Workflow behavior after an input fails.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Stop admitting inputs after the first failure.
    #[default]
    FailFast,
    /// Continue admitting independent inputs after failures.
    Continue,
}
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InputAttribute {
    Path(String),
    List(Vec<String>),
    Select(InputSelection),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// Glob string(s) describing files and folders to be checked
    pub files: Vec<String>,
    /// List of structs that contain details about the associated rule
    pub rules: Vec<Rule>,
    /// Glob string(s) describing files and folders to be ignored
    pub ignores: Option<Vec<String>>,
    /// Parent directory of the item being checked when not used within a workflow
    pub parent: Option<String>,
}
#[derive(Clone, Builder, Debug, Deserialize, Serialize)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Verified configuration-wrapper acquisitions retained for run metadata.
    #[builder(default)]
    #[serde(skip)]
    pub acquisitions: Vec<AcquisitionEvidence>,
    /// Ordered names for invocation-level positional values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<String>>,
    /// Unique workflow identifier
    pub id: Option<String>,
    /// Configuration version in semver format
    #[serde(default, deserialize_with = "deserialize_version")]
    pub version: Option<String>,
    #[serde(alias = "doc")]
    pub description: Option<String>,
    /// Software requirements for workflow (e.g. python, conda, etc.)
    pub requirements: Vec<String>,
    /// Hardward resources required for workflow (e.g. CPUs, GPUs, etc.)
    pub resources: Option<Vec<String>>,
    /// Keywords or tags used to describe this module
    pub keywords: Option<Vec<String>>,
    /// List of structs that contain details about the associated module
    pub modules: Vec<Module>,
    /// Environment variables set at the workflow level and available to all modules
    ///
    /// Note: Modules can override these variables
    pub variables: Option<Map<String, Value>>,
    /// Workflow checks configuration
    pub check: Option<Check>,
    /// Workflow input
    ///
    /// Can be:
    /// - a list of paths
    /// - a path to a folder that contains files (or folders)
    /// - a bounded CEL selection request over explicit local roots
    ///
    /// If input is not provided, the workflow will run similar to a 0-adic function.
    pub input: Option<InputAttribute>,
    /// Workflow output
    ///
    /// Designates a path to the workflow output artifacts for verification and export purposes
    #[serde(alias = "artifacts")]
    pub output: Option<String>,
    pub options: Option<ConfigOptions>,
    /// Require execution through an operator-selected sandbox policy.
    #[builder(default)]
    #[serde(default)]
    pub sandbox: bool,
}
#[derive(Builder, Clone, Debug, Deserialize, Serialize, With)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct ConfigOptions {
    /// Resource ceilings that nested configuration may only tighten.
    pub acquisition_limits: Option<AcquisitionLimits>,
    /// Runtime-only operator capability for cleartext HTTP.
    #[builder(default)]
    #[serde(skip)]
    pub allow_insecure_http: bool,
    /// Runtime-only immutable acquisition cache.
    #[serde(skip)]
    pub acquisition_cache: Option<AcquisitionCache>,
    /// Runtime-only trusted root acquisition policy.
    #[serde(skip)]
    pub acquisition_policy: Option<AcquisitionPolicy>,
    pub root: Option<PathBuf>,
    /// Maximum number of top-level workflow inputs that may run concurrently.
    pub batch: Option<InputBatch>,
    /// Module-agnostic memory-aware execution intent.
    pub memory: Option<MemoryOptions>,
    #[serde(alias = "dry-run")]
    pub dry_run: Option<bool>,
    /// Collect run-wide NVIDIA GPU telemetry with `nvidia-smi`.
    #[serde(alias = "gpu-monitor")]
    pub gpu_monitor: Option<bool>,
    /// Workflow behavior after an input fails.
    #[builder(default)]
    #[serde(default)]
    pub failure_policy: FailurePolicy,
    pub offline: Option<bool>,
    /// Skip modules whose declared outputs and marker are current.
    pub resume: Option<bool>,
    /// Emergency escape hatch for remote binary checksum verification.
    #[serde(skip)]
    pub skip_verify_checksum: Option<bool>,
    #[serde(alias = "skip-requirements", alias = "skip-verify-requirements")]
    pub skip_verify_requirements: Option<bool>,
    /// Exact remote module hosts permitted by this workflow.
    pub allowed: Option<Vec<String>>,
    /// Runtime-only cleanup override for managed containers.
    #[serde(skip)]
    pub container_cleanup: Option<ContainerCleanup>,
    /// Runtime-only policy for forwarding Xylem variables to containers.
    #[builder(default = true)]
    #[serde(skip, default = "default_container_env")]
    pub forward_container_env: bool,
    /// Runtime-only identifier used to derive managed container names.
    #[serde(skip)]
    pub container_run_id: Option<String>,
    /// Runtime-only root for input-local environment prefixes.
    #[serde(skip)]
    pub environment_root: Option<PathBuf>,
    /// Runtime-only validated sandbox policy.
    #[serde(skip)]
    pub sandbox_policy: Option<EffectiveSandboxPolicy>,
    /// Runtime-only input-local module paths keyed by stable module index.
    #[builder(default)]
    #[serde(skip, default)]
    pub workspace_modules: BTreeMap<usize, PathBuf>,
}
/// A structural workflow field could not be expanded from its configured bindings.
#[derive(Debug, thiserror::Error)]
#[error("{field}: {reason}")]
pub struct ConfigurationInterpolationError {
    /// Configuration field or indexed element that failed.
    pub field: String,
    /// Expansion failure without exposing binding values.
    pub reason: String,
}
/// Module-agnostic fixed-point estimator configuration.
#[derive(Builder, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct MemoryEstimatorOptions {
    /// Stable identity for telemetry and plan comparison.
    pub identity: String,
    /// Constant memory contribution before applying the safety multiplier.
    pub intercept: Memory,
    /// Largest aggregate input size covered by the estimator evidence.
    pub maximum_input: Memory,
    /// Smallest aggregate input size covered by the estimator evidence.
    pub minimum_input: Memory,
    /// Safety multiplier scaled by one million.
    pub safety_millionths: u64,
    /// Memory-per-input-byte slope scaled by one million.
    pub slope_millionths: u64,
}
/// Explicit memory budget and estimator definition.
#[derive(Builder, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct MemoryOptions {
    /// Maximum aggregate reservation intended for admitted inputs.
    pub budget: Memory,
    /// Estimator calibrated for this workflow's modules and inputs.
    pub estimator: MemoryEstimatorOptions,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    /// Path to save the results, when required
    pub output_path: Option<PathBuf>,
    /// Return all results (pass and fail)
    pub all: bool,
    /// Return only results that pass the check
    pub pass: bool,
    /// Return results with minimal formatting
    pub minimal: bool,
    /// Custom string data
    pub custom: Option<String>,
}
#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Name of the rule (displayed by the progress bar)
    #[builder(default)]
    pub name: String,
    pub uri: Option<String>,
    /// Options to pass to the checker
    pub parameters: Option<Parameters>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct VariableLookup(HashMap<String, String>);
impl Config {
    fn expand_location(location: &mut Location, variables: Option<&Map<String, Value>>, field: &str) -> Result<(), ConfigurationModuleError> {
        let authored = <&str>::from(&*location);
        match authored.contains("{{") || authored.contains("}}") {
            | false => Ok(()),
            | true => interpolation::expand(authored, variables, field.to_string())
                .map_err(ConfigurationModuleError::from)
                .and_then(|effective| {
                    let parsed = Location::Simple(effective.clone());
                    match ModuleUri::from(&parsed) {
                        | ModuleUri::File(path) if path.as_os_str().is_empty() => Err(ConfigurationInterpolationError {
                            field: field.to_string(),
                            reason: "module URI must not resolve to an empty file path".to_string(),
                        }
                        .into()),
                        | ModuleUri::Unknown(_) => Err(ConfigurationInterpolationError {
                            field: field.to_string(),
                            reason: "module URI must resolve to a supported location".to_string(),
                        }
                        .into()),
                        | ModuleUri::File(_) | ModuleUri::Url(_) | ModuleUri::Oci(_) => {
                            match location {
                                | Location::Simple(uri) | Location::Detailed { uri, .. } => *uri = effective,
                            }
                            Ok(())
                        }
                    }
                }),
        }
    }
    /// Expand the configured artifact output path using workflow bindings.
    ///
    /// Call before normalizing paths or resolving modules.
    /// A caller-supplied artifact output may instead replace this field without expanding it.
    pub fn expanded_output(&self) -> Result<Option<String>, ConfigurationInterpolationError> {
        self.output
            .as_ref()
            .map(|output| {
                interpolation::expand(output, self.variables.as_ref(), "output".to_string()).and_then(|value| {
                    if value.trim().is_empty() {
                        Err(ConfigurationInterpolationError {
                            field: "output".to_string(),
                            reason: "expanded output must not be empty".to_string(),
                        })
                    } else {
                        Ok(value)
                    }
                })
            })
            .transpose()
    }
    /// Resolve local configuration modules into their executable leaf modules.
    pub fn resolve(self, source: &Path) -> Result<Self, ConfigurationModuleError> {
        let source = canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        self.resolve_at(
            ConfigurationSource::File(source),
            false,
            false,
            Vec::new(),
            Vec::new(),
            0,
            None,
            None,
            None,
            None,
            &VariableOverrides::default(),
        )
    }
    /// Resolve local and remote configuration modules before planning.
    ///
    /// Remote documents and repositories are loaded only from verified immutable cache entries.
    /// Use [`Self::resolve_with_policy_and_bindings`] when an invocation supplies overrides.
    pub fn resolve_with_policy(
        self,
        source: &Path,
        policy: &AcquisitionPolicy,
        cache: &AcquisitionCache,
        dry_run: bool,
    ) -> Result<Self, ConfigurationModuleError> {
        self.resolve_with_policy_and_bindings(source, policy, cache, dry_run, &VariableOverrides::default())
    }
    /// Resolve modules while retaining invocation bindings through nested configurations.
    pub fn resolve_with_policy_and_bindings(
        mut self,
        source: &Path,
        policy: &AcquisitionPolicy,
        cache: &AcquisitionCache,
        dry_run: bool,
        bindings: &VariableOverrides,
    ) -> Result<Self, ConfigurationModuleError> {
        let offline = policy.offline();
        let options = self.options.take().unwrap_or_else(|| ConfigOptions::init().build());
        let policy = policy
            .clone()
            .with_checksum_verification_disabled(options.skip_verify_checksum.unwrap_or_default());
        self.options = Some(
            options
                .with_acquisition_cache(Some(cache.clone()))
                .with_acquisition_policy(Some(policy.clone())),
        );
        self.resolve_at(
            ConfigurationSource::File(source.to_path_buf()),
            dry_run,
            offline,
            Vec::new(),
            Vec::new(),
            0,
            Some(policy),
            None,
            None,
            Some(cache.clone()),
            bindings,
        )
    }
    pub(crate) fn module_environment(&self, module: Option<&Module>, input: Option<&str>) -> HashMap<String, String> {
        let envs = VariableLookup::from_values(self.variables.as_ref())
            .with_output_default(self.output.as_deref())
            .with_values(module.and_then(|module| module.variables.as_ref()))
            .with_value("MODULE_LIST", self.get_module_list())
            .with_input(input)
            .expand()
            .namespaced()
            .into_hash_map();
        debug!("=> {} Environment variables: {:#?}", Label::using(), envs.dimmed());
        envs
    }
    pub(crate) fn static_module_variables(&self, module: &Module) -> Option<Map<String, Value>> {
        let mut variables = Self::merge(self.variables.as_ref(), module.variables.as_ref(), None);
        if let Some(output) = self.output.as_ref() {
            variables
                .get_or_insert_default()
                .entry("OUTPUT_DIRECTORY".to_string())
                .or_insert_with(|| Value::String(output.clone()));
        }
        variables
    }
    /// Get inputs from input attribute for concurrent workflow execution
    ///
    /// See [InputAttribute] for more details.
    ///
    pub fn get_inputs(self) -> Result<Option<Vec<String>>, PlanError> {
        self.resolve_inputs(None, Path::new(".")).map(|inputs| {
            let values = inputs.values.into_iter().flatten().collect::<Vec<_>>();
            match values.is_empty() {
                | true => None,
                | false => Some(values),
            }
        })
    }
    pub fn get_module_list(&self) -> String {
        let names = self
            .modules
            .iter()
            .map(|module| {
                module
                    .module_path
                    .iter()
                    .map(|segment| segment.to_case(Case::Snake))
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect::<Vec<String>>();
        names.join(",").to_string()
    }
    pub(crate) fn resolve_context_variables(
        &self,
        context: &WorkflowContext<'_>,
        module: Option<&Module>,
    ) -> Result<BTreeMap<String, String>, ContextError> {
        let run = context.run();
        let artifact_output = run.artifact_output().map_or(Ok(None), |output| {
            output
                .to_str()
                .map(|value| Some(value.to_string()))
                .ok_or_else(|| ContextError::NonUnicodePath {
                    field: "artifact output",
                    path: output.to_path_buf(),
                })
        });
        let work_directory = context
            .workspace()
            .root()
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| ContextError::NonUnicodePath {
                field: "input workspace",
                path: context.workspace().root().to_path_buf(),
            });
        artifact_output.and_then(|artifact_output| {
            work_directory.map(|work_directory| {
                VariableLookup::from_values(self.variables.as_ref())
                    .with_output_default(self.output.as_deref())
                    .with_values(module.and_then(|module| module.variables.as_ref()))
                    .with_optional_value("OUTPUT_DIRECTORY", artifact_output)
                    .with_value("MODULE_LIST", self.get_module_list())
                    .with_value("RUN_ID", run.run_id())
                    .with_value("WORK_DIRECTORY", work_directory)
                    .with_input(context.input())
                    .expand()
                    .into_ordered_map()
            })
        })
    }
    /// Prepare workflow modules for execution
    ///
    /// Module preparation is done in parallel and includes the following steps:
    /// - Download binary files and verify file integrity
    /// - Git clone script projects and setup virtual environments
    /// - Validate local and downloaded remote modules are ready for execution
    ///
    /// Prepare workflow modules and return failures to the orchestration boundary.
    pub fn prepare_modules(self, execution: &ExecutionContext, observer: Option<&dyn ProgressObserver>) -> Result<Config, AcquisitionError> {
        let modules = (0..self.modules.len()).collect::<BTreeSet<_>>();
        self.prepare_modules_selected(execution, &modules, observer)
    }
    /// Prepare only modules required by at least one planned input.
    pub fn prepare_modules_selected(
        mut self,
        execution: &ExecutionContext,
        selected: &BTreeSet<usize>,
        observer: Option<&dyn ProgressObserver>,
    ) -> Result<Config, AcquisitionError> {
        execution.install(|| {
            let options = match self.clone().options {
                | Some(value) => value,
                | None => {
                    warn!("=> {} Configuration options", Label::not_found());
                    ConfigOptions::init().build()
                }
            };
            options.root.clone().as_ref().map_or_else(
                || {
                    Err(AcquisitionError::Policy {
                        remote: "module preparation".to_string(),
                        reason: "requires an owned root directory".to_string(),
                    })
                },
                |root| {
                    self.modules
                        .par_iter()
                        .enumerate()
                        .filter(|(index, _)| selected.contains(index))
                        .map(|(index, module)| {
                            ProgressPhase::Preparation.emit(ProgressState::Started, &module.name, &module.module_path, observer, None);
                            let module_root = root.join(format!("module-{index:06}"));
                            let result = module
                                .prepare_at(&module_root, &options, true)
                                .map(|(module, path)| (index, module, path));
                            ProgressPhase::Preparation.emit(
                                match result.is_ok() {
                                    | true => ProgressState::Finished,
                                    | false => ProgressState::Failed,
                                },
                                &module.name,
                                &module.module_path,
                                observer,
                                None,
                            );
                            result.map_err(|why| AcquisitionError::Module {
                                module: module.name.clone(),
                                source: io::Error::other(why),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|prepared| {
                            let mut workspace_modules = BTreeMap::new();
                            prepared.into_iter().for_each(|(index, module, path)| {
                                if let Some(target) = self.modules.get_mut(index) {
                                    target.acquisition = module.acquisition;
                                }
                                workspace_modules.insert(index, path);
                            });
                            self.options = Some(options.with_workspace_modules(workspace_modules));
                            self
                        })
                },
            )
        })
    }
    /// Materialize and prepare selected modules inside one input workspace.
    pub fn prepare_modules_selected_in_workspace(
        self,
        selected: &BTreeSet<usize>,
        observer: Option<&dyn ProgressObserver>,
        workspace: &InputWorkspace,
    ) -> Result<Config, AcquisitionError> {
        self.prepare_modules_in_workspace(selected, observer, workspace, true)
    }
    /// Materialize selected modules for sandbox preparation without creating host environments.
    pub fn materialize_modules_selected_in_workspace(
        self,
        selected: &BTreeSet<usize>,
        observer: Option<&dyn ProgressObserver>,
        workspace: &InputWorkspace,
    ) -> Result<Config, AcquisitionError> {
        self.prepare_modules_in_workspace(selected, observer, workspace, false)
    }
    fn prepare_modules_in_workspace(
        mut self,
        selected: &BTreeSet<usize>,
        observer: Option<&dyn ProgressObserver>,
        workspace: &InputWorkspace,
        prepare_environment: bool,
    ) -> Result<Config, AcquisitionError> {
        let options = self
            .options
            .clone()
            .unwrap_or_else(|| ConfigOptions::init().build())
            .with_root(Some(workspace.modules().to_path_buf()))
            .with_environment_root(Some(workspace.environments().to_path_buf()));
        self.modules
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.contains(index))
            .map(|(index, module)| {
                ProgressPhase::Preparation.emit(ProgressState::Started, &module.name, &module.module_path, observer, None);
                let result = workspace.module(index).map_err(|why| why.to_string()).and_then(|module_root| {
                    let module_environment = workspace.environments().join(format!("module-{index:06}"));
                    module
                        .prepare_at(
                            &module_root,
                            &options
                                .clone()
                                .with_root(Some(module_root.clone()))
                                .with_environment_root(Some(module_environment)),
                            prepare_environment,
                        )
                        .map(|(module, path)| (index, module, path))
                });
                ProgressPhase::Preparation.emit(
                    match result.is_ok() {
                        | true => ProgressState::Finished,
                        | false => ProgressState::Failed,
                    },
                    &module.name,
                    &module.module_path,
                    observer,
                    None,
                );
                result.map_err(|why| AcquisitionError::Module {
                    module: module.name.clone(),
                    source: io::Error::other(why),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|prepared| {
                let mut workspace_modules = BTreeMap::new();
                prepared.into_iter().for_each(|(index, module, path)| {
                    if let Some(target) = self.modules.get_mut(index) {
                        target.acquisition = module.acquisition;
                        if !prepare_environment {
                            target.set_uri(path.clone());
                        }
                    }
                    workspace_modules.insert(index, path);
                });
                self.options = Some(options.with_workspace_modules(workspace_modules));
                self
            })
    }
    /// Return modules that must execute for at least one input.
    pub fn required_module_indices(&self, inputs: &[Option<String>], resume: bool) -> Result<BTreeSet<usize>, PlanError> {
        inputs
            .iter()
            .try_fold((BTreeSet::new(), BTreeSet::new()), |state, input| {
                self.modules
                    .iter()
                    .enumerate()
                    .try_fold(state, |(mut selected, mut markers), (index, module)| {
                        let variables = self.module_environment(Some(module), input.as_deref());
                        CompletionPlan::inspect(module, &variables, resume, self.output.as_deref())
                            .map_err(PlanError::from)
                            .and_then(|completion| {
                                completion
                                    .as_ref()
                                    .map_or(Ok(()), |plan| match markers.insert(plan.marker.clone()) {
                                        | true => Ok(()),
                                        | false => Err(PlanError::Input(format!("completion marker collision at {}", plan.marker.display()))),
                                    })
                                    .map(|()| {
                                        if !matches!(completion.map(|plan| plan.decision), Some(CompletionDecision::SkipExisting)) {
                                            selected.insert(index);
                                        }
                                        (selected, markers)
                                    })
                            })
                    })
            })
            .map(|(selected, _)| selected)
    }
    pub fn read(path: PathBuf) -> Result<Config, ConfigReadError> {
        Self::try_from(path)
    }
    /// Import Xylem configuration JSON file
    ///
    /// See the [Xylem configuration syntax wiki page](https://code.ornl.gov/groups/research-enablement/xylem/-/wikis/Xylem-Standards/Configuration-Syntax)
    /// for more information configuration files
    ///
    pub fn read_json(path: PathBuf) -> Result<Config, ConfigReadError> {
        let data = read_configuration_content(&path, ConfigFormat::Json).and_then(|content| {
            parse_json_configuration(&content).map_err(|source| ConfigReadError::Invalid {
                format: ConfigFormat::Json,
                path: path.clone(),
                source: Box::new(source),
            })
        });
        let label = match &data {
            | Ok(_) => Label::using(),
            | Err(_) => Label::invalid(),
        };
        debug!("=> {} Configuration = {:#?}", label, data.dimmed());
        data
    }
    /// Import a strict JSON-with-comments Xylem configuration file.
    pub fn read_jsonc(path: PathBuf) -> Result<Config, ConfigReadError> {
        let data = read_jsonc(&path).map_err(|source| match source {
            | AcornConfigError::Read { source, .. } => ConfigReadError::Read {
                format: ConfigFormat::Jsonc,
                path: path.clone(),
                source,
            },
            | source => ConfigReadError::Invalid {
                format: ConfigFormat::Jsonc,
                path: path.clone(),
                source: Box::new(source),
            },
        });
        let label = match &data {
            | Ok(_) => Label::using(),
            | Err(_) => Label::invalid(),
        };
        debug!("=> {} Configuration = {:#?}", label, data.dimmed());
        data
    }
    /// Import Xylem configuration YAML file
    ///
    /// See the [Xylem configuration syntax wiki page](https://code.ornl.gov/groups/research-enablement/xylem/-/wikis/Xylem-Standards/Configuration-Syntax)
    /// for more information configuration files
    ///
    pub fn read_yaml(path: PathBuf) -> Result<Config, ConfigReadError> {
        let data = read_configuration_content(&path, ConfigFormat::Yaml).and_then(|content| {
            parse_yaml_configuration(&content).map_err(|source| ConfigReadError::Invalid {
                format: ConfigFormat::Yaml,
                path: path.clone(),
                source: Box::new(source),
            })
        });
        let label = match &data {
            | Ok(_) => Label::output(),
            | Err(_) => Label::fail(),
        };
        debug!("=> {} Configuration = {:#?}", label, data.dimmed());
        data
    }
    /// Run workflow modules
    ///
    /// Modules are run in the order they are defined in the configuration
    ///
    pub fn run_workflow(self, input: Option<String>, options: &ConfigOptions) -> WorkflowOutcome {
        self.run_workflow_with_progress(input, options, None)
    }
    /// Run workflow modules while emitting progress updates.
    pub fn run_workflow_with_progress(
        self,
        input: Option<String>,
        options: &ConfigOptions,
        observer: Option<&dyn ProgressObserver>,
    ) -> WorkflowOutcome {
        self.run_workflow_recorded(input, options, observer, None, None, 0)
    }
    /// Run workflow modules while emitting progress updates and durable module transitions.
    pub fn run_workflow_recorded(
        self,
        input: Option<String>,
        options: &ConfigOptions,
        observer: Option<&dyn ProgressObserver>,
        executor: Option<&dyn ModuleExecutor>,
        journal: Option<&RunJournal>,
        input_index: usize,
    ) -> WorkflowOutcome {
        self.run_workflow_recorded_with_context(input, options, observer, executor, journal, input_index, None)
    }
    /// Run workflow modules using one immutable input context.
    pub fn run_workflow_in_context(
        self,
        context: &WorkflowContext<'_>,
        options: &ConfigOptions,
        observer: Option<&dyn ProgressObserver>,
        executor: Option<&dyn ModuleExecutor>,
        journal: Option<&RunJournal>,
    ) -> WorkflowOutcome {
        self.run_workflow_recorded_with_context(
            context.input().map(str::to_string),
            options,
            observer,
            executor,
            journal,
            context.input_index(),
            Some(context),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn run_workflow_recorded_with_context(
        self,
        input: Option<String>,
        options: &ConfigOptions,
        observer: Option<&dyn ProgressObserver>,
        executor: Option<&dyn ModuleExecutor>,
        journal: Option<&RunJournal>,
        input_index: usize,
        context: Option<&WorkflowContext<'_>>,
    ) -> WorkflowOutcome {
        let execution = self.modules.iter().enumerate().try_fold(
            Vec::new(),
            |mut outcomes, (module_index, module)| -> Result<Vec<ModuleOutcome>, (Vec<ModuleOutcome>, String)> {
                let envs = match context {
                    | Some(context) => match context.command_environment(&self, module) {
                        | Ok(environment) => environment.into_iter().collect::<HashMap<_, _>>(),
                        | Err(why) => return Err((outcomes, why.to_string())),
                    },
                    | None => self.module_environment(Some(module), input.as_deref()),
                };
                let completion_envs = VariableLookup::from(envs.clone()).completion().into_hash_map();
                let result = CompletionPlan::inspect(module, &completion_envs, options.resume.unwrap_or_default(), self.output.as_deref())
                    .map_err(|why| why.to_string())
                    .and_then(|completion| {
                        let skip_existing = matches!(completion.as_ref().map(|plan| plan.decision), Some(CompletionDecision::SkipExisting))
                            && !options.dry_run.unwrap_or_default();
                        match skip_existing {
                            | true => match journal {
                                | Some(journal) => journal
                                    .transition_module(input_index, module_index, ExecutionState::SkippedExisting, None)
                                    .map(|()| ExecutionState::SkippedExisting)
                                    .map_err(|why| why.to_string()),
                                | None => Ok(ExecutionState::SkippedExisting),
                            },
                            | false => {
                                ProgressPhase::Execution.emit(ProgressState::Started, &module.name, &module.module_path, observer, input.as_deref());
                                let executable = options.workspace_modules.get(&module_index).map_or_else(
                                    || module.clone(),
                                    |path| {
                                        let mut executable = module.clone();
                                        executable.set_uri(path.clone());
                                        executable
                                    },
                                );
                                match journal {
                                    | Some(journal) => executable
                                        .acquisition
                                        .clone()
                                        .map_or(Ok(()), |acquisition| {
                                            journal.record_module_acquisition(input_index, module_index, acquisition)
                                        })
                                        .and_then(|()| journal.transition_module(input_index, module_index, ExecutionState::Running, None))
                                        .map_err(|why| why.to_string()),
                                    | None => Ok(()),
                                }
                                .and_then(|()| match &executable.module_type {
                                    | ModuleType::Binary { uri, .. } => options
                                        .root
                                        .clone()
                                        .ok_or_else(|| "root directory is not configured".to_string())
                                        .and_then(|root| {
                                            let root = root.to_absolute_path();
                                            let working_dir = match ModuleUri::from(uri) {
                                                | ModuleUri::File(path) => path
                                                    .parent()
                                                    .map(|path| path.to_absolute_path())
                                                    .ok_or_else(|| format!("binary module URI has no parent directory: {uri}")),
                                                | ModuleUri::Url(_) | ModuleUri::Oci(_) => Ok(root.clone()),
                                                | ModuleUri::Unknown(_) => Err(format!("module URI is invalid: {uri}")),
                                            };
                                            working_dir.and_then(|working_dir| {
                                                Script::from_module(executable.clone(), Some(PathBuf::from(root)))
                                                    .map_err(|why| why.to_string())
                                                    .and_then(|loaded| {
                                                        let script = loaded
                                                            .ensure_relative()
                                                            .expand_arguments_from(envs.clone())
                                                            .with_current_dir(working_dir)
                                                            .with_envs(envs)
                                                            .with_container_run(options.container_run_id.as_deref())
                                                            .with_container_env(options.forward_container_env);
                                                        match options.dry_run.unwrap_or_default() {
                                                            | true => {
                                                                info!(
                                                                    module = module.name,
                                                                    script = script.to_string(),
                                                                    cwd = script.current_dir,
                                                                    "=> {}",
                                                                    Label::dry_run()
                                                                );
                                                                Ok(())
                                                            }
                                                            | false => executor
                                                                .map_or_else(
                                                                    || script.run_checked_with_timeout(executable.timeout).map(|_| ()),
                                                                    |executor| {
                                                                        executor.execute(&script, input_index, module_index, executable.timeout)
                                                                    },
                                                                )
                                                                .map_err(|why| why.to_string()),
                                                        }
                                                    })
                                            })
                                        }),
                                    | ModuleType::Script { programming_language, uri } => options
                                        .root
                                        .clone()
                                        .ok_or_else(|| "root directory is not configured".to_string())
                                        .and_then(|root_path| {
                                            let root = Some(root_path);
                                            module_working_directory(uri.into(), root.as_ref())
                                                .ok_or_else(|| format!("module URI is invalid: {uri}"))
                                                .and_then(|working_dir| {
                                                    Script::from_module(executable.clone(), root)
                                                        .map_err(|why| why.to_string())
                                                        .and_then(|loaded| {
                                                            let script = loaded
                                                                .expand_arguments_from(envs.clone())
                                                                .with_current_dir(working_dir.clone())
                                                                .with_envs(envs)
                                                                .with_container_run(options.container_run_id.as_deref())
                                                                .with_container_env(options.forward_container_env);
                                                            let configured = match programming_language {
                                                                | Some(ModuleLanguage::Python) => executable
                                                                    .resolve_virtual_environment(&working_dir)
                                                                    .map(|virtual_environment| {
                                                                        script.with_virtual_environment(VirtualEnvironment::input_local(
                                                                            virtual_environment,
                                                                            options
                                                                                .environment_root
                                                                                .as_ref()
                                                                                .map(|root| root.join(format!("module-{module_index:06}"))),
                                                                        ))
                                                                    })
                                                                    .map_err(|why| why.to_string()),
                                                                | Some(ModuleLanguage::Generic) | None => Ok(script),
                                                                | Some(language) => Err(format!("unsupported script language: {language}")),
                                                            };
                                                            configured.and_then(|script| match options.dry_run.unwrap_or_default() {
                                                                | true => {
                                                                    info!(
                                                                        module = module.name,
                                                                        script = script.to_string(),
                                                                        cwd = script.current_dir,
                                                                        "=> {}",
                                                                        Label::dry_run()
                                                                    );
                                                                    Ok(())
                                                                }
                                                                | false => executor
                                                                    .map_or_else(
                                                                        || script.run_checked_with_timeout(executable.timeout).map(|_| ()),
                                                                        |executor| {
                                                                            executor.execute(&script, input_index, module_index, executable.timeout)
                                                                        },
                                                                    )
                                                                    .map_err(|why| why.to_string()),
                                                            })
                                                        })
                                                })
                                        }),
                                    | ModuleType::Configuration { .. } => Err("configuration modules are not executable".to_string()),
                                })
                                .and_then(|()| match executor.is_some_and(ModuleExecutor::is_cancellation_requested) {
                                    | true => Err("workflow cancellation requested".to_string()),
                                    | false => Ok(()),
                                })
                                .and_then(|()| match (options.dry_run.unwrap_or_default(), completion.as_ref()) {
                                    | (false, Some(plan)) => plan.write().map_err(|why| why.to_string()),
                                    | _ => Ok(()),
                                })
                                .map(|()| match options.dry_run.unwrap_or_default() {
                                    | true => ExecutionState::Skipped,
                                    | false => ExecutionState::Succeeded,
                                })
                                .and_then(|terminal| match journal {
                                    | Some(journal) => journal
                                        .transition_module(input_index, module_index, terminal, None)
                                        .map(|()| terminal)
                                        .map_err(|why| why.to_string()),
                                    | None => Ok(terminal),
                                })
                            }
                        }
                    });
                match result {
                    | Ok(terminal) => {
                        let progress_state = ProgressState::from(terminal);
                        ProgressPhase::Execution.emit(progress_state, &module.name, &module.module_path, observer, input.as_deref());
                        outcomes.push(ModuleOutcome {
                            index: module_index,
                            name: module.name.clone(),
                            module_path: module.module_path.clone(),
                            state: terminal,
                            error: None,
                        });
                        Ok(outcomes)
                    }
                    | Err(why) => {
                        let state = match executor.is_some_and(ModuleExecutor::is_cancellation_requested) {
                            | true => ExecutionState::Cancelled,
                            | false => ExecutionState::Failed,
                        };
                        let error = match journal {
                            | Some(journal) => match journal.transition_module(input_index, module_index, state, Some(&why)) {
                                | Ok(()) => why,
                                | Err(journal_error) => format!("{why}; additionally, record terminal module state: {journal_error}"),
                            },
                            | None => why,
                        };
                        ProgressPhase::Execution.emit(ProgressState::from(state), &module.name, &module.module_path, observer, input.as_deref());
                        outcomes.push(ModuleOutcome {
                            index: module_index,
                            name: module.name.clone(),
                            module_path: module.module_path.clone(),
                            state,
                            error: Some(error.clone()),
                        });
                        Err((outcomes, error))
                    }
                }
            },
        );
        match execution {
            | Ok(outcomes) => {
                let dry_run = options.dry_run.unwrap_or_default();
                let all_skipped_existing = outcomes.iter().all(|outcome| outcome.state.is_skipped_existing());
                let state = match (dry_run, all_skipped_existing) {
                    | (true, _) => ExecutionState::Skipped,
                    | (false, true) => ExecutionState::SkippedExisting,
                    | (false, false) => ExecutionState::Succeeded,
                };
                WorkflowOutcome::init().maybe_input(input).state(state).modules(outcomes).build()
            }
            | Err((outcomes, error)) => {
                let state = match outcomes.last().is_some_and(|outcome| outcome.state.is_cancelled()) {
                    | true => ExecutionState::Cancelled,
                    | false => ExecutionState::Failed,
                };
                WorkflowOutcome::init()
                    .maybe_input(input)
                    .state(state)
                    .modules(outcomes)
                    .error(error)
                    .build()
            }
        }
    }
    /// Verify software requirements are met on host system (ex. Python, conda, etc...)
    ///
    /// Uses process similar to Linux `which` to determine if a given command is available on the host system.
    ///
    pub fn verify_requirements(self) -> Result<Config, RequirementError> {
        match &self.options {
            | Some(options) if options.skip_verify_requirements.unwrap_or_default() => {
                warn!("Skipping requirements verification");
                Ok(self)
            }
            | Some(_) => match self.requirements.iter().find(|name| {
                let command = Command::init().name((*name).clone()).build();
                !command.test()
            }) {
                | Some(name) => Err(RequirementError { command: name.clone() }),
                | None => Ok(self),
            },
            | None => Ok(self),
        }
    }
    /// Verify hardware resources (ex. GPU) are available for the workflow on host system
    ///
    /// Under construction
    ///
    pub fn verify_resources(self) -> Config {
        self
    }
    pub fn with_options(mut self, options: ConfigOptions) -> Config {
        self.options = Some(options);
        self
    }
    fn merge(
        parent: Option<&Map<String, Value>>,
        wrapper: Option<&Map<String, Value>>,
        child: Option<&Map<String, Value>>,
    ) -> Option<Map<String, Value>> {
        let variables = parent
            .into_iter()
            .chain(wrapper)
            .chain(child)
            .flat_map(|variables| variables.iter())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>();
        match variables.is_empty() {
            | true => None,
            | false => Some(variables),
        }
    }
    fn identity(location: &Location, source: &ConfigurationSource) -> Option<String> {
        let uri = location.uri().unwrap_or_else(|| <&str>::from(location).to_string());
        match ModuleUri::from(location) {
            | ModuleUri::File(path) => source.path().map(|parent| {
                let child = match path.is_absolute() {
                    | true => path,
                    | false => parent.parent().map_or(path.clone(), |directory| directory.join(path)),
                };
                canonicalize(&child).unwrap_or(child).display().to_string()
            }),
            | ModuleUri::Url(value)
                if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS) && Self::is_remote_document(&uri) =>
            {
                Some(uri)
            }
            | ModuleUri::Oci(reference) => {
                let reference = source
                    .path()
                    .and_then(|path| ModuleUri::Oci(reference.clone()).absolutize(path))
                    .map_or(reference, |location| <&str>::from(&location).to_string());
                Some(format!("oci:{reference}"))
            }
            | ModuleUri::Url(_) => Some(format!("git:{uri}#{}", Self::revision(location).map_or("unpinned", String::as_str))),
            | ModuleUri::Unknown(_) => None,
        }
    }
    fn load(
        location: &Location,
        checksum: Option<&str>,
        source: &ConfigurationSource,
        dry_run: bool,
        module: &str,
        policy: Option<&AcquisitionPolicy>,
        cache: Option<&AcquisitionCache>,
    ) -> Result<(Self, ConfigurationSource), ConfigurationModuleError> {
        let uri = location.uri().unwrap_or_else(|| <&str>::from(location).to_string());
        match ModuleUri::from(location) {
            | ModuleUri::File(_) | ModuleUri::Oci(_) if checksum.is_some() => Err(ConfigurationModuleError::Source {
                module: module.to_string(),
                uri,
            }),
            | ModuleUri::File(path) => match source.path() {
                | Some(parent) => {
                    let child = match path.is_absolute() {
                        | true => path,
                        | false => parent.parent().map_or(path.clone(), |directory| directory.join(path)),
                    };
                    let child = canonicalize(&child).unwrap_or(child);
                    Config::read(child.clone())
                        .map(|config| (config, ConfigurationSource::File(child)))
                        .map_err(|source| ConfigurationModuleError::Read {
                            module: module.to_string(),
                            source,
                        })
                }
                | None => Err(ConfigurationModuleError::Source {
                    module: module.to_string(),
                    uri,
                }),
            },
            | ModuleUri::Url(value) => {
                let scheme = Scheme::from(value.scheme().as_str());
                let direct_document = matches!(scheme, Scheme::HTTP | Scheme::HTTPS) && Self::is_remote_document(&uri);
                match (policy, cache, direct_document, Self::revision(location), checksum) {
                    | (None, _, _, _, _) | (_, None, _, _, _) | (Some(_), Some(_), false, _, Some(_)) => Err(ConfigurationModuleError::Source {
                        module: module.to_string(),
                        uri,
                    }),
                    | (Some(_), Some(_), true, Some(_), _) => Err(ConfigurationModuleError::Revision {
                        module: module.to_string(),
                        uri,
                    }),
                    | (Some(policy), Some(cache), direct_document, revision, checksum) => {
                        let source = match direct_document {
                            | true => AcquisitionSource::HttpDocument {
                                sha256: checksum.unwrap_or_default().to_string(),
                                url: uri.clone(),
                            },
                            | false => AcquisitionSource::Git {
                                commit: revision.cloned().unwrap_or_default(),
                                url: uri.clone(),
                            },
                        };
                        let effective_policy = policy.restrict_offline(dry_run);
                        AcquisitionPlan::build(module.split("::").map(str::to_string).collect(), source, &effective_policy)
                            .and_then(|plan| AcquisitionExecutor::new(effective_policy, cache.clone()).execute(&plan))
                            .map_err(|source| match (dry_run, source) {
                                | (true, AcquisitionError::Offline { .. }) => ConfigurationModuleError::RemoteDryRun {
                                    module: module.to_string(),
                                    uri: uri.clone(),
                                },
                                | (_, source) => ConfigurationModuleError::Remote {
                                    module: module.to_string(),
                                    reason: source.to_string(),
                                },
                            })
                            .and_then(|result| {
                                let loaded = match direct_document {
                                    | true => read_to_string(&result.path)
                                        .map_err(|source| ConfigurationModuleError::Read {
                                            module: module.to_string(),
                                            source: ConfigReadError::Read {
                                                format: Self::remote_format(&uri),
                                                path: result.path.clone(),
                                                source,
                                            },
                                        })
                                        .and_then(|content| Self::read_remote_document(&content, &uri, module))
                                        .map(|config| (config, ConfigurationSource::Http { identity: uri.clone() })),
                                    | false => {
                                        let entrypoint = result.path.join("config.json");
                                        Config::read(entrypoint.clone())
                                            .map(|config| {
                                                (
                                                    config,
                                                    ConfigurationSource::Git {
                                                        identity: format!("git:{uri}#{}", revision.map_or("unpinned", String::as_str)),
                                                        path: entrypoint,
                                                    },
                                                )
                                            })
                                            .map_err(|source| ConfigurationModuleError::Read {
                                                module: module.to_string(),
                                                source,
                                            })
                                    }
                                };
                                loaded.map(|(mut config, source)| {
                                    config.acquisitions.push(result.evidence);
                                    (config, source)
                                })
                            })
                    }
                }
            }
            | ModuleUri::Oci(reference) => match (policy, cache) {
                | (Some(policy), Some(cache)) => {
                    let reference = source
                        .path()
                        .and_then(|path| ModuleUri::Oci(reference.clone()).absolutize(path))
                        .map_or(reference, |location| <&str>::from(&location).to_string());
                    let local_layout = reference.starts_with("oci-layout:");
                    AcquisitionSource::oci(reference)
                        .and_then(|source| {
                            let effective_policy = policy.restrict_offline(dry_run);
                            AcquisitionPlan::build(module.split("::").map(str::to_string).collect(), source, &effective_policy).and_then(|plan| {
                                let executor = AcquisitionExecutor::new(effective_policy, cache.clone());
                                match (dry_run, local_layout) {
                                    | (true, true) => executor.cached(&plan).and_then(|cached| {
                                        cached.ok_or_else(|| AcquisitionError::Offline {
                                            remote: plan.source.to_string(),
                                        })
                                    }),
                                    | _ => executor.execute(&plan),
                                }
                            })
                        })
                        .map_err(|source| match (dry_run, source) {
                            | (true, AcquisitionError::Offline { .. }) => ConfigurationModuleError::RemoteDryRun {
                                module: module.to_string(),
                                uri: uri.clone(),
                            },
                            | (_, source) => ConfigurationModuleError::Remote {
                                module: module.to_string(),
                                reason: source.to_string(),
                            },
                        })
                        .and_then(|result| {
                            let entrypoint = result.path.join("config.json");
                            Config::read(entrypoint.clone())
                                .map(|config| {
                                    (
                                        config,
                                        ConfigurationSource::Oci {
                                            identity: format!("oci:{uri}"),
                                            path: entrypoint,
                                        },
                                    )
                                })
                                .map_err(|source| ConfigurationModuleError::Read {
                                    module: module.to_string(),
                                    source,
                                })
                                .map(|(mut config, source)| {
                                    config.acquisitions.push(result.evidence);
                                    (config, source)
                                })
                        })
                }
                | _ => Err(ConfigurationModuleError::Source {
                    module: module.to_string(),
                    uri,
                }),
            },
            | ModuleUri::Unknown(_) => Err(ConfigurationModuleError::Source {
                module: module.to_string(),
                uri,
            }),
        }
    }
    fn is_remote_document(uri: &str) -> bool {
        Url::parse(uri).ok().as_ref().is_some_and(Self::is_remote_document_url)
    }
    fn is_remote_document_url(url: &Url) -> bool {
        let mime = MimeType::from(url.path());
        matches!(Scheme::from(url.scheme()), Scheme::HTTP | Scheme::HTTPS) && (mime.is_json() || mime.is_jsonc() || mime.is_yaml())
    }
    fn read_remote_document(content: &str, uri: &str, module: &str) -> Result<Self, ConfigurationModuleError> {
        let path = PathBuf::from(uri);
        let format = Self::remote_format(uri);
        let parsed = match format {
            | ConfigFormat::Json => parse_json_configuration(content).map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>),
            | ConfigFormat::Jsonc => parse_jsonc_configuration(content),
            | ConfigFormat::Yaml => parse_yaml_configuration(content).map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>),
        };
        parsed.map_err(|source| ConfigurationModuleError::Read {
            module: module.to_string(),
            source: ConfigReadError::Invalid { format, path, source },
        })
    }
    fn remote_format(uri: &str) -> ConfigFormat {
        match Url::parse(uri).ok().map(|url| MimeType::from(url.path())) {
            | Some(MimeType::Jsonc) => ConfigFormat::Jsonc,
            | Some(MimeType::Yaml) => ConfigFormat::Yaml,
            | _ => ConfigFormat::Json,
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn resolve_at(
        self,
        source: ConfigurationSource,
        dry_run: bool,
        offline: bool,
        ancestors: Vec<String>,
        path: Vec<String>,
        depth: usize,
        policy: Option<AcquisitionPolicy>,
        inherited_variables: Option<Map<String, Value>>,
        inherited_output: Option<String>,
        cache: Option<AcquisitionCache>,
        bindings: &VariableOverrides,
    ) -> Result<Self, ConfigurationModuleError> {
        match depth > 16 {
            | true => Err(ConfigurationModuleError::Depth { path }),
            | false => {
                let identity = source.identity();
                if ancestors.contains(&identity) {
                    return Err(ConfigurationModuleError::Cycle { path, location: identity });
                }
                let ancestors = ancestors.into_iter().chain(once(identity)).collect::<Vec<_>>();
                let mut variables = Self::merge(inherited_variables.as_ref(), self.variables.as_ref(), None);
                bindings.apply_values(&mut variables);
                let output = self.output.clone().or(inherited_output);
                self.modules
                    .into_iter()
                    .try_fold(
                        (Vec::new(), self.requirements.clone(), self.acquisitions.clone()),
                        |(mut modules, mut requirements, mut acquisitions), module| match module.module_type {
                            | ModuleType::Configuration { checksum, mut uri } => {
                                let nested_path = path.iter().cloned().chain(once(module.name.clone())).collect::<Vec<_>>();
                                match (module.template.is_some(), module.completion.is_some()) {
                                    | (true, _) => Err(ConfigurationModuleError::Wrapper {
                                        module: nested_path.join("::"),
                                        field: "template",
                                    }),
                                    | (_, true) => Err(ConfigurationModuleError::Wrapper {
                                        module: nested_path.join("::"),
                                        field: "completion",
                                    }),
                                    | (false, false) => {
                                        let mut module_variables = Self::merge(variables.as_ref(), module.variables.as_ref(), None);
                                        bindings.apply_values(&mut module_variables);
                                        if let Some(output) = output.as_ref() {
                                            module_variables
                                                .get_or_insert_default()
                                                .entry("OUTPUT_DIRECTORY".to_string())
                                                .or_insert_with(|| Value::String(output.clone()));
                                        }
                                        let field = format!("modules.{}.uri", nested_path.join("::"));
                                        Self::expand_location(&mut uri, module_variables.as_ref(), &field).and_then(|()| {
                                            match Self::identity(&uri, &source).filter(|identity| ancestors.contains(identity)) {
                                                | Some(location) => Err(ConfigurationModuleError::Cycle {
                                                    path: nested_path.clone(),
                                                    location,
                                                }),
                                                | None => Self::load(
                                                    &uri,
                                                    checksum.as_deref(),
                                                    &source,
                                                    dry_run,
                                                    &nested_path.join("::"),
                                                    policy.as_ref(),
                                                    cache.as_ref(),
                                                ),
                                            }
                                        })
                                    }
                                    .and_then(|(child, child_source)| {
                                        let child_offline = offline || child.options.as_ref().and_then(|options| options.offline).unwrap_or_default();
                                        let child_policy = policy.as_ref().map_or(Ok(None), |policy| {
                                            let limits = child
                                                .options
                                                .as_ref()
                                                .and_then(|options| options.acquisition_limits.as_ref())
                                                .unwrap_or_else(|| policy.limits());
                                            policy
                                                .restrict(child.options.as_ref().and_then(|options| options.allowed.as_deref()), limits)
                                                .map(|policy| policy.restrict_offline(child_offline))
                                                .map(Some)
                                                .map_err(|source| ConfigurationModuleError::Policy {
                                                    module: nested_path.join("::"),
                                                    reason: source.to_string(),
                                                })
                                        });
                                        child_policy.and_then(|child_policy| {
                                            let inherited = Self::merge(variables.as_ref(), module.variables.as_ref(), None);
                                            child.resolve_at(
                                                child_source,
                                                dry_run,
                                                child_offline,
                                                ancestors.clone(),
                                                nested_path.clone(),
                                                depth.saturating_add(1),
                                                child_policy,
                                                inherited,
                                                output.clone(),
                                                cache.clone(),
                                                bindings,
                                            )
                                        })
                                    })
                                    .map(|child| {
                                        modules.extend(child.modules);
                                        acquisitions.extend(child.acquisitions);
                                        child.requirements.into_iter().for_each(|requirement| {
                                            if !requirements.contains(&requirement) {
                                                requirements.push(requirement);
                                            }
                                        });
                                        (modules, requirements, acquisitions)
                                    }),
                                }
                            }
                            | module_type => {
                                let mut module = Module { module_type, ..module };
                                let mut module_variables = Self::merge(variables.as_ref(), module.variables.as_ref(), None);
                                bindings.apply_values(&mut module_variables);
                                if let Some(output) = output.as_ref() {
                                    module_variables
                                        .get_or_insert_default()
                                        .entry("OUTPUT_DIRECTORY".to_string())
                                        .or_insert_with(|| Value::String(output.clone()));
                                }
                                let location = match &mut module.module_type {
                                    | ModuleType::Binary { uri, .. } | ModuleType::Script { uri, .. } => Some(uri),
                                    | ModuleType::Configuration { .. } => None,
                                };
                                let expanded = location.map_or(Ok(()), |location| {
                                    let field = format!("modules.{}.uri", path.iter().chain(once(&module.name)).cloned().collect::<Vec<_>>().join("::"));
                                    Self::expand_location(location, module_variables.as_ref(), &field)
                                });
                                expanded.and_then(|()| {
                                if let Some(source) = source.path() {
                                    if matches!(
                                        &module.module_type,
                                        ModuleType::Script { uri, .. } if matches!(ModuleUri::from(uri), ModuleUri::Oci(value) if value.starts_with("oci-layout:"))
                                    ) {
                                        module.absolutize(source);
                                    }
                                }
                                Self::validate_nested_leaf(&module, &source, offline, &path, policy.as_ref()).map(|()| {
                                    if let Some(source) = source.path() {
                                        module.absolutize_container_mounts(source);
                                        if !path.is_empty() {
                                            module.absolutize(source);
                                        }
                                    }
                                    module.module_path = path.iter().chain(once(&module.name)).cloned().collect();
                                    module.acquisition_policy = policy.clone();
                                    module.variables = module_variables;
                                    modules.push(module);
                                    (modules, requirements, acquisitions)
                                })
                                })
                            }
                        },
                    )
                    .map(|(modules, requirements, acquisitions)| Self {
                        acquisitions,
                        modules,
                        requirements,
                        ..self
                    })
            }
        }
    }
    fn revision(location: &Location) -> Option<&String> {
        match location {
            | Location::Detailed { revision, .. } => revision.as_ref(),
            | Location::Simple(_) => None,
        }
    }
    fn validate_nested_leaf(
        module: &Module,
        source: &ConfigurationSource,
        _offline: bool,
        path: &[String],
        policy: Option<&AcquisitionPolicy>,
    ) -> Result<(), ConfigurationModuleError> {
        let location = match &module.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri, .. } | ModuleType::Script { uri, .. } => uri,
        };
        let module_path = path.iter().chain(once(&module.name)).cloned().collect::<Vec<_>>().join("::");
        match (path.is_empty(), source, ModuleUri::from(location), policy) {
            | (true, _, _, _)
            | (_, ConfigurationSource::File(_) | ConfigurationSource::Git { .. } | ConfigurationSource::Oci { .. }, ModuleUri::File(_), _) => Ok(()),
            | (false, ConfigurationSource::Http { .. }, ModuleUri::File(_), _)
            | (false, _, ModuleUri::Unknown(_), _)
            | (false, _, ModuleUri::Url(_) | ModuleUri::Oci(_), None) => Err(ConfigurationModuleError::Source {
                module: module_path,
                uri: <&str>::from(location).to_string(),
            }),
            | (false, _, ModuleUri::Url(_) | ModuleUri::Oci(_), Some(policy)) => {
                module
                    .acquisition_plan(policy)
                    .map(|_| ())
                    .map_err(|source| ConfigurationModuleError::Remote {
                        module: module_path,
                        reason: source.to_string(),
                    })
            }
        }
    }
}
impl TryFrom<&Path> for Config {
    type Error = ConfigReadError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        match MimeType::from(path.to_string_lossy().as_ref()) {
            | MimeType::Json => Self::read_json(path.to_path_buf()),
            | MimeType::Jsonc => Self::read_jsonc(path.to_path_buf()),
            | MimeType::Yaml => Self::read_yaml(path.to_path_buf()),
            | _ => Err(ConfigReadError::UnsupportedExtension(path.to_path_buf())),
        }
    }
}
impl TryFrom<PathBuf> for Config {
    type Error = ConfigReadError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}
impl fmt::Display for ConfigFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        MimeType::from(*self).fmt(formatter)
    }
}
impl core::error::Error for ConfigReadError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Invalid { source, .. } => Some(source.as_ref()),
            | Self::Read { source, .. } => Some(source),
            | Self::UnsupportedExtension(_) => None,
        }
    }
}
impl fmt::Display for ConfigReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Invalid { format, path, source } => {
                write!(formatter, "parse {format} configuration {}: {source}", path.display())
            }
            | Self::Read { format, path, source } => {
                write!(formatter, "read {format} configuration {}: {source}", path.display())
            }
            | Self::UnsupportedExtension(path) => {
                write!(
                    formatter,
                    "configuration {} must use a .json, .jsonc, .yaml, or .yml extension",
                    path.display()
                )
            }
        }
    }
}
impl ConfigurationModuleError {
    /// Whether this failure represents unavailable remote acquisition rather than invalid configuration data.
    pub fn is_availability(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }
}
impl ConfigurationSource {
    fn identity(&self) -> String {
        match self {
            | Self::File(path) => canonicalize(path).unwrap_or_else(|_| path.clone()).display().to_string(),
            | Self::Git { identity, .. } | Self::Http { identity } | Self::Oci { identity, .. } => identity.clone(),
        }
    }
    fn path(&self) -> Option<&Path> {
        match self {
            | Self::File(path) | Self::Git { path, .. } | Self::Oci { path, .. } => Some(path),
            | Self::Http { .. } => None,
        }
    }
}
impl fmt::Display for FailurePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::Continue => "continue",
            | Self::FailFast => "fail_fast",
        })
    }
}
impl From<ConfigFormat> for MimeType {
    fn from(format: ConfigFormat) -> Self {
        match format {
            | ConfigFormat::Json => Self::Json,
            | ConfigFormat::Jsonc => Self::Jsonc,
            | ConfigFormat::Yaml => Self::Yaml,
        }
    }
}
impl Module {
    pub(crate) fn acquisition_plan(&self, policy: &AcquisitionPolicy) -> Result<AcquisitionPlan, AcquisitionError> {
        let source = match &self.module_type {
            | ModuleType::Binary { checksum, uri } => match ModuleUri::from(uri) {
                | ModuleUri::File(path) => Ok(AcquisitionSource::Local { path }),
                | ModuleUri::Url(url) => match policy.checksum_verification_disabled() {
                    | true => Ok(AcquisitionSource::HttpBinaryUnverified { url: url.to_string() }),
                    | false => Ok(AcquisitionSource::HttpBinary {
                        sha256: checksum.clone().unwrap_or_default(),
                        url: url.to_string(),
                    }),
                },
                | ModuleUri::Oci(value) => Err(AcquisitionError::Identity {
                    value,
                    reason: "binary modules must use a local file or a digest-pinned HTTP(S) URL",
                }),
                | ModuleUri::Unknown(value) => Err(AcquisitionError::Identity {
                    value,
                    reason: "module URI must be a local file, HTTP(S) binary, or commit-pinned Git repository",
                }),
            },
            | ModuleType::Configuration { checksum, uri } => match ModuleUri::from(uri) {
                | ModuleUri::File(path) => Ok(AcquisitionSource::Local { path }),
                | ModuleUri::Url(url) if Config::is_remote_document(&url.to_string()) => Ok(AcquisitionSource::HttpDocument {
                    sha256: checksum.clone().unwrap_or_default(),
                    url: url.to_string(),
                }),
                | ModuleUri::Oci(reference) => AcquisitionSource::oci(reference),
                | ModuleUri::Url(url) => Ok(AcquisitionSource::Git {
                    commit: Config::revision(uri).cloned().unwrap_or_default(),
                    url: url.to_string(),
                }),
                | ModuleUri::Unknown(value) => Err(AcquisitionError::Identity {
                    value,
                    reason: "configuration URI must be a local file, digest-pinned HTTP(S) document, or commit-pinned Git repository",
                }),
            },
            | ModuleType::Script { uri, .. } => match ModuleUri::from(uri) {
                | ModuleUri::File(path) => Ok(AcquisitionSource::Local { path }),
                | ModuleUri::Oci(reference) => AcquisitionSource::oci(reference),
                | ModuleUri::Url(url) => Ok(AcquisitionSource::Git {
                    commit: Config::revision(uri).cloned().unwrap_or_default(),
                    url: url.to_string(),
                }),
                | ModuleUri::Unknown(value) => Err(AcquisitionError::Identity {
                    value,
                    reason: "script URI must be a local file or commit-pinned Git repository",
                }),
            },
        };
        source.and_then(|source| AcquisitionPlan::build(self.module_path.clone(), source, policy))
    }
    pub(crate) fn effective_acquisition_policy(&self, options: &ConfigOptions) -> Result<AcquisitionPolicy, AcquisitionError> {
        self.acquisition_policy
            .clone()
            .or_else(|| options.acquisition_policy.clone())
            .map_or_else(
                || {
                    AcquisitionPolicy::from_hosts(
                        options.allowed.as_deref(),
                        options.allow_insecure_http,
                        None,
                        options.offline.unwrap_or_default(),
                        options.acquisition_limits.clone().unwrap_or_default(),
                    )
                },
                Ok,
            )
            .map(|policy| policy.with_checksum_verification_disabled(options.skip_verify_checksum.unwrap_or_default()))
    }
    fn materialize_source(&self, source: &Path, module_root: &Path) -> Result<PathBuf, String> {
        match &self.module_type {
            | ModuleType::Binary { .. } => create_dir_all(module_root)
                .map_err(|why| format!("create module workspace {}: {why}", module_root.display()))
                .and_then(|()| SafePath::new("module").map_err(|why| why.to_string()))
                .and_then(|destination| materialize(source, module_root, &destination).map_err(|why| why.to_string())),
            | ModuleType::Script { .. } => module_root.parent().zip(module_root.file_name()).map_or_else(
                || Err(format!("module {} has no workspace-relative materialization path", self.name)),
                |(root, name)| {
                    SafePath::new(name)
                        .map_err(|why| why.to_string())
                        .and_then(|destination| materialize(source, root, &destination).map_err(|why| why.to_string()))
                },
            ),
            | ModuleType::Configuration { .. } => Err(format!("module {} uses reserved configuration module type", self.name)),
        }
    }
    fn prepare_at(&self, module_root: &Path, options: &ConfigOptions, prepare_environment: bool) -> Result<(Self, PathBuf), String> {
        self.effective_acquisition_policy(options)
            .map_err(|why| why.to_string())
            .and_then(|policy| self.acquisition_plan(&policy).map(|plan| (policy, plan)).map_err(|why| why.to_string()))
            .and_then(|(policy, plan)| match options.dry_run.unwrap_or_default() {
                | true => {
                    info!(module = self.name, source = %plan.source, action = ?plan.action, "=> {}", Label::dry_run());
                    let path = match &plan.source {
                        | AcquisitionSource::Local { path } => path.clone(),
                        | AcquisitionSource::Git { url, .. }
                        | AcquisitionSource::Oci { reference: url, .. }
                        | AcquisitionSource::HttpBinary { url, .. }
                        | AcquisitionSource::HttpBinaryUnverified { url }
                        | AcquisitionSource::HttpDocument { url, .. } => PathBuf::from(url),
                    };
                    Ok((self.clone(), path))
                }
                | false => {
                    let acquired = match &plan.source {
                        | AcquisitionSource::Local { path } => Ok((path.clone(), None)),
                        | AcquisitionSource::Git { .. }
                        | AcquisitionSource::Oci { .. }
                        | AcquisitionSource::HttpBinary { .. }
                        | AcquisitionSource::HttpBinaryUnverified { .. }
                        | AcquisitionSource::HttpDocument { .. } => options
                            .acquisition_cache
                            .clone()
                            .map_or_else(AcquisitionCache::system, Ok)
                            .and_then(|cache| AcquisitionExecutor::new(policy, cache).execute(&plan))
                            .map(|result| (result.path, Some(result.evidence))),
                    };
                    acquired
                        .map_err(|why| why.to_string())
                        .and_then(|(source, evidence)| self.materialize_source(&source, module_root).map(|path| (path, evidence)))
                        .and_then(|(path, evidence)| match &self.module_type {
                            | ModuleType::Binary { .. } => make_executable(path.clone())
                                .map_err(|why| format!("module {} binary is not executable at {}: {why}", self.name, path.display()))
                                .map(|()| (path, evidence)),
                            | ModuleType::Script { programming_language, .. }
                                if prepare_environment && matches!(programming_language, Some(ModuleLanguage::Python)) =>
                            {
                                self.prepare_python_environment(&path, options).map(|()| (path, evidence))
                            }
                            | ModuleType::Configuration { .. } | ModuleType::Script { .. } => Ok((path, evidence)),
                        })
                        .map(|(path, evidence)| {
                            let mut prepared = self.clone();
                            prepared.acquisition = evidence;
                            prepared.set_uri(path.clone());
                            (prepared, path)
                        })
                }
            })
    }
    fn prepare_python_environment(&self, module_root: &Path, options: &ConfigOptions) -> Result<(), String> {
        let module_root = module_root.display().to_string();
        self.resolve_virtual_environment(&module_root)
            .map_err(|why| why.to_string())
            .and_then(|environment| {
                let environment = VirtualEnvironment::input_local(environment, options.environment_root.clone());
                let requires_prefix = environment
                    .as_ref()
                    .is_some_and(|environment| environment.manager.is_conda() || environment.manager.is_mamba());
                Script::maybe_create_virtual_environment(module_root, environment)
                    .map_err(|why| why.to_string())
                    .map(|script| (requires_prefix, script))
            })
            .and_then(|(requires_prefix, script)| {
                let prepared = match script {
                    | Some(script) => match options.dry_run.unwrap_or_default() {
                        | true => {
                            info!(
                                module = self.name,
                                script = script.to_string(),
                                cwd = script.current_dir,
                                "=> {}",
                                Label::dry_run()
                            );
                            Ok(())
                        }
                        | false => script.run_checked().map(|_| ()).map_err(|why| why.to_string()),
                    },
                    | None => Ok(()),
                };
                prepared.and_then(|()| match (requires_prefix, options.environment_root.as_deref()) {
                    | (true, Some(prefix)) if !prefix.is_dir() => Err(format!(
                        "module {} did not create its input-local environment prefix at {}",
                        self.name,
                        prefix.display()
                    )),
                    | _ => Ok(()),
                })
            })
    }
    fn set_uri(&mut self, path: PathBuf) {
        let uri = Location::from(format!("file:{}", path.display()).as_str());
        match &mut self.module_type {
            | ModuleType::Binary { uri: value, .. } | ModuleType::Configuration { uri: value, .. } | ModuleType::Script { uri: value, .. } => {
                *value = uri
            }
        }
    }
}
impl Module {
    fn absolutize(&mut self, source: &Path) {
        let uri = match &mut self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri, .. } | ModuleType::Script { uri, .. } => uri,
        };
        if let Some(value) = ModuleUri::from(uri).absolutize(source) {
            *uri = value;
        }
    }
    fn absolutize_container_mounts(&mut self, source: &Path) {
        if let Some(TemplateAttribute::Template(template)) = &mut self.template {
            template.absolutize_container_mounts(source);
        }
    }
}
impl From<HashMap<String, String>> for VariableLookup {
    fn from(values: HashMap<String, String>) -> Self {
        Self(values)
    }
}
impl VariableLookup {
    fn completion(mut self) -> Self {
        self.0.retain(|key, _| !matches!(key.as_str(), "_XYLEM_RUN_ID" | "_XYLEM_WORK_DIRECTORY"));
        self
    }
    fn expand(self) -> Self {
        let raw = self.0;
        let mut expanded = raw.clone();
        for _ in 0..=raw.len() {
            let variables = expanded.iter().map(|(key, value)| (key.clone(), value.clone())).collect::<Vec<_>>();
            let next = raw
                .iter()
                .map(|(key, value)| (key.clone(), StringTemplate::new(value).expand(variables.clone())))
                .collect::<HashMap<_, _>>();
            if next == expanded {
                break;
            }
            expanded = next;
        }
        Self(expanded)
    }
    fn from_values(values: Option<&Map<String, Value>>) -> Self {
        Self(
            values
                .into_iter()
                .flat_map(Map::iter)
                .map(|(key, value)| {
                    let value = match value {
                        | Value::String(value) => value.clone(),
                        | value => value.to_string(),
                    };
                    (key.clone(), value)
                })
                .collect(),
        )
    }
    fn into_hash_map(self) -> HashMap<String, String> {
        self.0
    }
    fn into_ordered_map(self) -> BTreeMap<String, String> {
        self.0.into_iter().collect()
    }
    fn namespaced(self) -> Self {
        Self(self.0.into_iter().map(|(key, value)| (format!("_XYLEM_{key}"), value)).collect())
    }
    fn with_input(self, input: Option<&str>) -> Self {
        match input {
            | Some(input) => {
                let basename = Path::new(input).file_name().and_then(|name| name.to_str()).unwrap_or(input);
                self.with_value("INPUT", input).with_value("INPUT_BASENAME", basename)
            }
            | None => self,
        }
    }
    fn with_optional_value(self, name: &str, value: Option<String>) -> Self {
        match value {
            | Some(value) => self.with_value(name, value),
            | None => self,
        }
    }
    fn with_output_default(self, output: Option<&str>) -> Self {
        match (self.0.contains_key("OUTPUT_DIRECTORY"), output) {
            | (false, Some(output)) => self.with_value("OUTPUT_DIRECTORY", output),
            | _ => self,
        }
    }
    fn with_value(mut self, name: &str, value: impl Into<String>) -> Self {
        self.0.insert(name.to_string(), value.into());
        self
    }
    fn with_values(mut self, values: Option<&Map<String, Value>>) -> Self {
        self.0.extend(Self::from_values(values).0);
        self
    }
}
fn default_container_env() -> bool {
    true
}
fn deserialize_version<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let version = Option::<String>::deserialize(deserializer)?;
    if let Some(value) = version.as_deref() {
        SemanticVersion::parse_exact(value).map_err(D::Error::custom)?;
    }
    Ok(version)
}
fn parse_json_configuration(content: &str) -> serde_json::Result<Config> {
    serde_json::from_str::<Value>(content).and_then(serde_json::from_value)
}
fn parse_jsonc_configuration(content: &str) -> Result<Config, Box<dyn core::error::Error + Send + Sync>> {
    parse_to_serde_value::<Value>(content, &ParseOptions::strict_jsonc())
        .map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>)
        .and_then(|value| serde_json::from_value(value).map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>))
}
fn parse_yaml_configuration(content: &str) -> serde_norway::Result<Config> {
    serde_norway::from_str::<serde_norway::Value>(content)
        .and_then(|mut value| value.apply_merge().map(|()| value))
        .and_then(serde_norway::from_value)
}
fn read_configuration_content(path: &Path, format: ConfigFormat) -> Result<String, ConfigReadError> {
    read_to_string(path)
        .map_err(|source| ConfigReadError::Read {
            format,
            path: path.to_path_buf(),
            source,
        })
        .and_then(|content| match content.trim().is_empty() {
            | true => Err(ConfigReadError::Invalid {
                format,
                path: path.to_path_buf(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "configuration is empty")),
            }),
            | false => Ok(content),
        })
}
#[cfg(test)]
mod tests;
