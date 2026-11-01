//! Deterministic, side-effect-free execution plans
extern crate alloc;

use crate::acquisition::{AcquisitionError, AcquisitionPlan};
use crate::completion::{CompletionDecision, CompletionError, CompletionPlan};
use crate::config::{Config, ConfigOptions, FailurePolicy, InputAttribute, MemoryEstimatorOptions, MemoryOptions};
use crate::error::TemplateError;
use crate::execution::{ExecutionLimits, InputBatch};
use crate::gpu::GPU_SAMPLE_INTERVAL_MS;
use crate::memory::{MemoryAdmissionError, MemoryCalculationError, MemoryEstimator, MemoryEstimatorError, PlannedMemoryInput};
use crate::sandbox::{IsolationClass, OciRuntime, PreparationNetwork, ProcessGuard, SandboxLifecycle, SandboxProviderKind, SandboxResources};
use crate::script::{
    module_working_directory, ContainerCleanup, ContainerEnvironment, ContainerRuntime, ContainerStartup, Module, ModuleType, Script,
    TemplateAttribute,
};
use crate::selection::{InputSource, ResolvedInputs, SelectionError};
use acorn_core::time::{Milliseconds, Seconds};
use acorn_schema::hardware::Memory;
use acorn_schema::validation::ValidationReport;
use alloc::collections::BTreeMap;
use bon::Builder;
use core::fmt;
use serde::Serialize;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Memory behavior represented by an execution plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPlanMode {
    /// Retain fixed batching because at least one input cannot be estimated safely.
    FixedFallback,
    /// Admit inputs using their planned memory reservations.
    MemoryAware,
}
/// Failure to validate or render an execution plan.
#[derive(Debug)]
pub enum PlanError {
    /// Remote source policy or immutable identity planning failed.
    Acquisition(AcquisitionError),
    /// Completion resolution or observation failed.
    Completion(CompletionError),
    /// Input discovery could not produce a deterministic input set.
    Input(String),
    /// A planned reservation exceeds the configured budget.
    MemoryAdmission(MemoryAdmissionError),
    /// A reservation could not be calculated safely.
    MemoryCalculation(MemoryCalculationError),
    /// The configured estimator could not be constructed.
    MemoryEstimator(MemoryEstimatorError),
    /// Required sandbox policy was not resolved by the trusted caller.
    Sandbox(String),
    /// Scripted input selection failed.
    Selection(SelectionError),
    /// An inline module command template could not be rendered.
    Template(TemplateError),
    /// Semantic configuration validation failed.
    Validation(ValidationReport),
}
impl PlanError {
    /// Whether planning failed because an otherwise valid remote source is unavailable under policy.
    pub fn is_availability(&self) -> bool {
        matches!(self, Self::Acquisition(error) if !matches!(error, AcquisitionError::Identity { .. }))
    }
}
/// Versioned dry-run plan rendered without mutating the host.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExecutionPlan {
    /// Ordered input admission and concurrency intent.
    pub admission: PlannedAdmission,
    /// Whole-run sandbox artifact publication intent.
    pub artifacts: Option<PlannedArtifactPublication>,
    /// Configuration used to build the plan.
    pub configuration: PlanConfiguration,
    /// Deduplicated containers required by modules that would run.
    pub containers: Vec<PlannedContainer>,
    /// Stable input context identities without disposable workspace paths.
    pub contexts: Vec<PlannedContext>,
    /// Effective workflow failure policy.
    pub failure_policy: FailurePolicy,
    /// GPU monitoring that would run during execution.
    pub gpu_monitoring: Option<PlannedGpuMonitoring>,
    /// Ordered resolved input values and their source provenance.
    pub inputs: ResolvedInputs,
    /// Validated memory intent, when configured.
    pub memory: Option<PlannedMemory>,
    /// Ordered module decisions for every input.
    pub modules: Vec<PlannedModule>,
    /// Whether resume observations influenced decisions.
    pub resume: bool,
    /// Effective sandbox intent and policy identity.
    pub sandbox: PlannedSandbox,
    /// Execution-plan schema version.
    pub schema_version: u32,
}
/// Planned input admission without executing or creating workspaces.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedAdmission {
    /// Configured maximum number of active inputs.
    pub batch: Option<InputBatch>,
    /// Effective ceiling when the caller supplies its thread limit.
    pub effective_ceiling: Option<usize>,
    /// Input positions in admission order.
    pub input_indices: Vec<usize>,
}
/// Stable identity for an input-local execution context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedContext {
    /// Configured input position, including the zero-input case.
    pub input_index: usize,
    /// Workspace allocation policy rather than a disposable path.
    pub workspace: &'static str,
}
/// Configuration identity embedded in an execution plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanConfiguration {
    /// Optional configured workflow identifier.
    pub id: Option<String>,
    /// Resolved workflow artifact root.
    pub output: Option<String>,
    /// Configuration path selected by the caller.
    pub path: String,
    /// Optional configured workflow version.
    pub version: Option<String>,
}
/// Fully validated acquisition intent required before a module can execute.
pub type PlannedAcquisition = AcquisitionPlan;
/// Bounded whole-run artifact publication intent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlannedArtifactPublication {
    /// Maximum artifact bytes accepted from each input.
    pub bytes_per_input: Memory,
    /// Resolved final destination.
    pub destination: String,
    /// Maximum artifact entries accepted from each input.
    pub entries_per_input: u64,
    /// Stable publication strategy.
    pub mode: String,
}
/// Command information known without acquiring or mutating the host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedCommand {
    /// Expanded arguments for inline templates.
    pub arguments: Vec<String>,
    /// Program for inline templates, or null when template acquisition is deferred.
    pub program: Option<String>,
    /// Deferred template path, when applicable.
    pub template_source: Option<String>,
    /// Deterministic module working directory before a run root is allocated.
    pub working_directory: Option<String>,
}
/// Resolved completion declaration included in the plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedCompletion {
    /// Exact marker path.
    pub marker: String,
    /// Sorted output path or glob patterns.
    pub outputs: Vec<String>,
}
/// One container operation required by the plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedContainer {
    /// Whether Xylem would create or verify the container.
    pub action: String,
    /// Effective cleanup policy for a managed container.
    pub cleanup: Option<ContainerCleanup>,
    /// Configured image for a managed container.
    pub image: Option<String>,
    /// Configured logical name.
    pub name: String,
    /// Modules referencing this declaration.
    pub module_indices: Vec<usize>,
    /// Explicit bind mounts.
    pub mounts: Vec<PlannedMount>,
    /// Runtime used for delegation.
    pub runtime: ContainerRuntime,
    /// Readiness timeout in seconds.
    pub startup_timeout_seconds: u64,
    /// Startup strategy for a managed container, or null for an external container.
    pub startup: Option<ContainerStartup>,
}
/// GPU monitoring selected for a planned run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedGpuMonitoring {
    /// Fixed sampling interval in milliseconds.
    pub interval: Milliseconds,
    /// Telemetry provider executable.
    pub source: String,
}
/// Validated memory intent included in an execution plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlannedMemory {
    /// Maximum aggregate reservation requested by the workflow.
    pub budget: Memory,
    /// Validated module-agnostic estimator definition.
    pub estimator: MemoryEstimatorOptions,
    /// Ordered read-only sizing and reservation decisions.
    pub inputs: Vec<PlannedMemoryInput>,
    /// Runtime behavior for this plan.
    pub mode: MemoryPlanMode,
}
/// One planned module decision.
#[derive(Builder, Clone, Debug, PartialEq, Serialize)]
#[builder(on(String, into), start_fn(name = init))]
pub struct PlannedModule {
    /// Read-only acquisition intent.
    pub acquisition: PlannedAcquisition,
    /// Command and template information available before acquisition.
    pub command: PlannedCommand,
    /// Resolved completion declaration, when configured.
    pub completion: Option<PlannedCompletion>,
    /// Human-readable decision vocabulary.
    pub decision: String,
    /// Input position in deterministic execution order.
    pub input_index: usize,
    /// Input value, or null for a zero-input workflow.
    pub input: Option<String>,
    /// Module position in configured order.
    pub module_index: usize,
    /// Configured module name.
    pub module: String,
    /// Stable ancestry from the root workflow to the executable leaf.
    pub module_path: Vec<String>,
    /// Whether the decision includes a read-only completion observation.
    pub observed_completion: bool,
    /// Configured execution timeout, when present.
    pub timeout_seconds: Option<Seconds>,
    /// Effective variables after workflow, output-default, module, and input precedence.
    pub variables: BTreeMap<String, String>,
}
/// One bind mount shown in a dry-run plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedMount {
    /// Whether the mount is read-only.
    pub read_only: bool,
    /// Resolved host source.
    pub source: String,
    /// Absolute guest target.
    pub target: String,
}
/// Sandbox intent included in a dry-run execution plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedSandbox {
    /// Cleanup, retention, and stale-resource policy, absent for host mode.
    pub lifecycle: Option<SandboxLifecycle>,
    /// Minimum required isolation class, absent for host mode.
    pub minimum_isolation: Option<IsolationClass>,
    /// `off` for compatibility host execution or `required` for sandbox execution.
    pub mode: String,
    /// Canonical effective-policy digest, absent for host mode.
    pub policy_digest: Option<String>,
    /// Network available only during dependency preparation.
    pub preparation_network: Option<PreparationNetwork>,
    /// Required process guards.
    pub process_guards: Option<Vec<ProcessGuard>>,
    /// Selected provider, absent for host mode.
    pub provider: Option<SandboxProviderKind>,
    /// Per-input resource ceilings, absent for host mode.
    pub resources: Option<SandboxResources>,
    /// Selected OCI runtime, when applicable.
    pub runtime: Option<OciRuntime>,
    /// Provider lifecycle model, absent for host mode.
    pub session_model: Option<String>,
    /// Immutable provider workload, absent for host mode.
    pub workload: Option<String>,
}
/// Resolve CLI-over-configuration input precedence with deterministic duplicate handling.
impl Config {
    /// Resolve inputs and retain deterministic source provenance for planning.
    pub fn resolve_inputs(&self, cli_inputs: Option<&[String]>, configuration_path: &Path) -> Result<ResolvedInputs, PlanError> {
        match cli_inputs.filter(|values| !values.is_empty()) {
            | Some(values) => Ok(ResolvedInputs::configured(InputSource::Cli, normalize_inputs(values.to_vec()))),
            | None => match &self.input {
                | Some(InputAttribute::List(values)) => Ok(ResolvedInputs::configured(InputSource::List, normalize_inputs(values.clone()))),
                | Some(InputAttribute::Path(value)) => read_dir(value)
                    .map_err(|error| PlanError::Input(format!("inspect input directory {value}: {error}")))
                    .and_then(|entries| {
                        entries
                            .map(|entry| {
                                entry
                                    .map_err(|error| PlanError::Input(format!("inspect input directory entry in {value}: {error}")))
                                    .and_then(|entry| {
                                        entry.path().into_os_string().into_string().map_err(|path| {
                                            PlanError::Input(format!("input directory entry is not valid UTF-8: {}", path.to_string_lossy()))
                                        })
                                    })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .map(|mut values| {
                        values.sort();
                        ResolvedInputs::configured(InputSource::Directory, normalize_inputs(values))
                    }),
                | Some(InputAttribute::Select(selection)) => selection.resolve(self.variables.as_ref(), configuration_path).map_err(PlanError::from),
                | None => Ok(ResolvedInputs::configured(InputSource::None, vec![None])),
            },
        }
    }
}
impl ContainerEnvironment {
    fn planned(self, module_index: usize) -> PlannedContainer {
        let startup = self.create.then(|| self.resolved_startup().0);
        PlannedContainer {
            action: match self.create {
                | true => "would_create".to_string(),
                | false => "would_verify_existing".to_string(),
            },
            cleanup: self.create.then_some(self.cleanup.unwrap_or_default()),
            image: self.image,
            module_indices: vec![module_index],
            mounts: self
                .mounts
                .into_iter()
                .map(|mount| PlannedMount {
                    read_only: mount.read_only,
                    source: mount.source,
                    target: mount.target,
                })
                .collect(),
            name: self.name,
            runtime: self.runtime,
            startup_timeout_seconds: self.startup_timeout_seconds.unwrap_or(30),
            startup,
        }
    }
}
impl ExecutionPlan {
    /// Build a plan using only configuration and explicitly labeled read-only filesystem observations.
    pub fn build(config: &Config, path: String, inputs: &[Option<String>], resume: bool) -> Result<Self, PlanError> {
        let source = match config.input.clone() {
            | Some(InputAttribute::Select(_)) => Err(PlanError::Input(
                "selection inputs must be resolved with provenance before building a plan".to_string(),
            )),
            | Some(input) => Ok(InputSource::from(input)),
            | None => Ok(InputSource::None),
        };
        source.and_then(|source| {
            let inputs = ResolvedInputs::configured(source, inputs.to_vec());
            Self::build_resolved(config, path, &inputs, resume)
        })
    }
    /// Build a plan from inputs resolved by the provenance-aware planning boundary.
    pub fn build_resolved(config: &Config, path: String, inputs: &ResolvedInputs, resume: bool) -> Result<Self, PlanError> {
        Self::build_resolved_with_limits(config, path, inputs, resume, None)
    }
    /// Build a plan with the caller's effective thread limit for admission evidence.
    pub fn build_resolved_with_limits(
        config: &Config,
        path: String,
        inputs: &ResolvedInputs,
        resume: bool,
        limits: Option<ExecutionLimits>,
    ) -> Result<Self, PlanError> {
        PlannedSandbox::try_from(config.clone())
            .and_then(|sandbox| config.validate_for_inputs(&inputs.values).map_err(PlanError::from).map(|()| sandbox))
            .and_then(|sandbox| {
                config
                    .options
                    .as_ref()
                    .and_then(|options| options.memory.as_ref())
                    .map(|memory| PlannedMemory::build(memory, &inputs.values))
                    .transpose()
                    .and_then(|memory| {
                        inputs
                            .values
                            .iter()
                            .enumerate()
                            .try_fold(Vec::new(), |modules, (input_index, input)| {
                                config
                                    .modules
                                    .iter()
                                    .enumerate()
                                    .try_fold(modules, |mut modules, (module_index, module)| {
                                        module.plan(config, input, input_index, module_index, resume).map(|planned| {
                                            modules.push(planned);
                                            modules
                                        })
                                    })
                            })
                            .map(|modules| Self {
                                admission: PlannedAdmission {
                                    batch: config.options.as_ref().and_then(|options| options.batch),
                                    effective_ceiling: limits.map(|limits| {
                                        config
                                            .options
                                            .as_ref()
                                            .and_then(|options| options.batch)
                                            .map_or_else(|| limits.threads(), |batch| batch.get().min(limits.threads()))
                                    }),
                                    input_indices: (0..inputs.values.len()).collect(),
                                },
                                artifacts: match (&config.output, sandbox.resources.as_ref()) {
                                    | (Some(destination), Some(resources)) => Some(PlannedArtifactPublication {
                                        bytes_per_input: Memory::kb(resources.artifact_bytes as f64 / 1024.0),
                                        destination: destination.clone(),
                                        entries_per_input: resources.artifact_files,
                                        mode: "whole_run_atomic".to_string(),
                                    }),
                                    | _ => None,
                                },
                                containers: PlannedContainer::from_modules(config, &modules),
                                contexts: (0..inputs.values.len())
                                    .map(|input_index| PlannedContext {
                                        input_index,
                                        workspace: "input_local",
                                    })
                                    .collect(),
                                configuration: PlanConfiguration {
                                    id: config.id.clone(),
                                    output: config.output.clone(),
                                    path,
                                    version: config.version.clone(),
                                },
                                gpu_monitoring: config
                                    .options
                                    .as_ref()
                                    .and_then(|options| options.gpu_monitor)
                                    .unwrap_or_default()
                                    .then(|| PlannedGpuMonitoring {
                                        interval: *GPU_SAMPLE_INTERVAL_MS,
                                        source: "nvidia-smi".to_string(),
                                    }),
                                failure_policy: config
                                    .options
                                    .as_ref()
                                    .map_or_else(FailurePolicy::default, |options| options.failure_policy),
                                inputs: inputs.clone(),
                                memory,
                                modules,
                                resume,
                                sandbox,
                                schema_version: 15,
                            })
                    })
            })
    }
}
impl From<InputAttribute> for InputSource {
    fn from(input: InputAttribute) -> Self {
        match input {
            | InputAttribute::List(_) => Self::List,
            | InputAttribute::Path(_) => Self::Directory,
            | InputAttribute::Select(_) => unreachable!("selection input provenance is produced by selection resolution"),
        }
    }
}
impl Module {
    fn command(&self, variables: &BTreeMap<String, String>) -> Result<PlannedCommand, TemplateError> {
        let uri = match &self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri, .. } | ModuleType::Script { uri, .. } => uri,
        };
        let working_directory = module_working_directory(uri.into(), None);
        match &self.template {
            | Some(TemplateAttribute::Template(template)) => {
                let variables = variables.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
                Script::from_template(template.as_ref().clone())
                    .map(|script| script.expand_arguments_from(variables))
                    .map(|script| PlannedCommand {
                        arguments: script.arguments,
                        program: Some(script.command.name),
                        template_source: None,
                        working_directory,
                    })
            }
            | Some(TemplateAttribute::Path(path)) => Ok(PlannedCommand {
                arguments: Vec::new(),
                program: None,
                template_source: working_directory
                    .as_ref()
                    .map(|directory| PathBuf::from(directory).join(path).display().to_string()),
                working_directory,
            }),
            | None => Ok(PlannedCommand {
                arguments: Vec::new(),
                program: None,
                template_source: working_directory
                    .as_ref()
                    .map(|directory| PathBuf::from(directory).join("template.json").display().to_string()),
                working_directory,
            }),
        }
    }
    fn plan(
        &self,
        config: &Config,
        input: &Option<String>,
        input_index: usize,
        module_index: usize,
        resume: bool,
    ) -> Result<PlannedModule, PlanError> {
        if resume && self.completion.is_none() {
            debug!(
                module = self.name,
                input = input.as_deref().unwrap_or("<no input>"),
                "Resume plan will run module because no completion declaration is configured"
            );
        }
        let variables = config
            .module_environment(Some(self), input.as_deref())
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let hash_variables = variables.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        let options = config.options.clone().unwrap_or_else(|| ConfigOptions::init().build());
        self.effective_acquisition_policy(&options)
            .map_err(PlanError::from)
            .and_then(|policy| self.acquisition_plan(&policy).map_err(PlanError::from))
            .and_then(|acquisition| {
                CompletionPlan::resolve(self, &hash_variables, config.output.as_deref())
                    .map(|resolved| (acquisition, resolved))
                    .map_err(PlanError::from)
            })
            .and_then(|(acquisition, resolved)| {
                let decision = match resume {
                    | true => CompletionPlan::inspect(self, &hash_variables, true, config.output.as_deref())
                        .map_err(PlanError::from)
                        .map(|plan| match plan {
                            | Some(plan) if plan.decision == CompletionDecision::SkipExisting => "would_skip_existing",
                            | Some(_) | None => "would_run",
                        }),
                    | false => Ok("would_run"),
                };
                decision.and_then(|decision| {
                    self.command(&variables).map_err(PlanError::from).map(|command| {
                        let completion = resolved.map(|resolved| PlannedCompletion {
                            marker: resolved.marker.display().to_string(),
                            outputs: resolved.outputs.iter().map(|output| output.display().to_string()).collect(),
                        });
                        PlannedModule::init()
                            .acquisition(acquisition)
                            .command(command)
                            .maybe_completion(completion)
                            .decision(decision)
                            .input_index(input_index)
                            .maybe_input(input.clone())
                            .module_index(module_index)
                            .module(self.name.as_str())
                            .module_path(self.module_path.clone())
                            .observed_completion(resume && self.completion.is_some())
                            .maybe_timeout_seconds(self.timeout)
                            .variables(variables)
                            .build()
                    })
                })
            })
    }
}
impl core::error::Error for PlanError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Acquisition(error) => Some(error),
            | Self::Completion(error) => Some(error),
            | Self::Input(_) | Self::Sandbox(_) => None,
            | Self::MemoryAdmission(error) => Some(error),
            | Self::MemoryCalculation(error) => Some(error),
            | Self::MemoryEstimator(error) => Some(error),
            | Self::Selection(error) => Some(error),
            | Self::Template(error) => Some(error),
            | Self::Validation(error) => Some(error),
        }
    }
}
impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Acquisition(error) => error.fmt(formatter),
            | Self::Completion(error) => error.fmt(formatter),
            | Self::Input(error) | Self::Sandbox(error) => formatter.write_str(error),
            | Self::MemoryAdmission(error) => error.fmt(formatter),
            | Self::MemoryCalculation(error) => error.fmt(formatter),
            | Self::MemoryEstimator(error) => error.fmt(formatter),
            | Self::Selection(error) => error.fmt(formatter),
            | Self::Template(error) => error.fmt(formatter),
            | Self::Validation(error) => error.fmt(formatter),
        }
    }
}
impl From<AcquisitionError> for PlanError {
    fn from(error: AcquisitionError) -> Self {
        Self::Acquisition(error)
    }
}
impl From<CompletionError> for PlanError {
    fn from(error: CompletionError) -> Self {
        Self::Completion(error)
    }
}
impl From<MemoryAdmissionError> for PlanError {
    fn from(error: MemoryAdmissionError) -> Self {
        Self::MemoryAdmission(error)
    }
}
impl From<MemoryCalculationError> for PlanError {
    fn from(error: MemoryCalculationError) -> Self {
        Self::MemoryCalculation(error)
    }
}
impl From<MemoryEstimatorError> for PlanError {
    fn from(error: MemoryEstimatorError) -> Self {
        Self::MemoryEstimator(error)
    }
}
impl From<SelectionError> for PlanError {
    fn from(error: SelectionError) -> Self {
        Self::Selection(error)
    }
}
impl From<TemplateError> for PlanError {
    fn from(error: TemplateError) -> Self {
        Self::Template(error)
    }
}
impl From<ValidationReport> for PlanError {
    fn from(error: ValidationReport) -> Self {
        Self::Validation(error)
    }
}
impl PlannedContainer {
    fn from_modules(config: &Config, modules: &[PlannedModule]) -> Vec<Self> {
        modules
            .iter()
            .filter(|module| module.decision == "would_run")
            .map(|module| module.module_index)
            .filter_map(|module_index| {
                config.modules.get(module_index).and_then(|module| {
                    Script::from_module(module.clone(), None)
                        .ok()
                        .and_then(|script| script.command.container)
                        .map(|container| (module_index, container))
                })
            })
            .fold(
                BTreeMap::<(ContainerRuntime, String), Self>::new(),
                |mut planned, (module_index, container)| {
                    let key = (container.runtime, container.name.clone());
                    planned
                        .entry(key)
                        .and_modify(|value| value.module_indices.push(module_index))
                        .or_insert_with(|| container.planned(module_index));
                    planned
                },
            )
            .into_values()
            .collect()
    }
}
impl PlannedMemory {
    fn build(memory: &MemoryOptions, inputs: &[Option<String>]) -> Result<Self, PlanError> {
        let estimator = MemoryEstimator::new(
            memory.estimator.identity.clone(),
            memory.estimator.intercept.clone(),
            memory.estimator.minimum_input.clone(),
            memory.estimator.maximum_input.clone(),
            memory.estimator.slope_millionths,
            memory.estimator.safety_millionths,
        );
        estimator
            .map_err(PlanError::from)
            .and_then(|estimator| estimator.plan(inputs).map_err(PlanError::from))
            .and_then(|inputs| {
                let budget_bytes = memory.budget.checked_bytes().unwrap_or_default();
                inputs
                    .iter()
                    .find_map(|input| {
                        input
                            .reservation
                            .as_ref()
                            .filter(|reservation| reservation.checked_bytes().is_some_and(|reservation| reservation > budget_bytes))
                            .map(|reservation| (input.input_index, reservation.clone()))
                    })
                    .map_or_else(
                        || {
                            let mode = match inputs.iter().any(|input| input.fallback.is_some()) {
                                | true => MemoryPlanMode::FixedFallback,
                                | false => MemoryPlanMode::MemoryAware,
                            };
                            Ok(Self {
                                budget: memory.budget.clone(),
                                estimator: memory.estimator.clone(),
                                inputs,
                                mode,
                            })
                        },
                        |(index, reservation)| {
                            Err(PlanError::from(MemoryAdmissionError::Oversized {
                                budget: memory.budget.clone(),
                                index,
                                reservation,
                            }))
                        },
                    )
            })
    }
    /// Return ordered reservations when memory-aware admission is active.
    pub fn reservations(&self) -> Option<Vec<Memory>> {
        (self.mode == MemoryPlanMode::MemoryAware)
            .then(|| self.inputs.iter().map(|input| input.reservation.clone()).collect::<Option<Vec<_>>>())
            .flatten()
    }
}
impl TryFrom<Config> for PlannedSandbox {
    type Error = PlanError;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        match (
            config.sandbox,
            config.options.as_ref().and_then(|options| options.sandbox_policy.as_ref()),
        ) {
            | (false, _) => Ok(Self {
                lifecycle: None,
                minimum_isolation: None,
                mode: "off".to_string(),
                policy_digest: None,
                preparation_network: None,
                process_guards: None,
                provider: None,
                resources: None,
                runtime: None,
                session_model: None,
                workload: None,
            }),
            | (true, Some(policy)) => Ok(Self {
                lifecycle: Some(policy.policy.lifecycle.clone()),
                minimum_isolation: Some(policy.policy.minimum_isolation),
                mode: "required".to_string(),
                policy_digest: Some(policy.digest.clone()),
                preparation_network: Some(policy.policy.preparation_network),
                process_guards: Some(policy.policy.process_guards.clone()),
                provider: Some(policy.provider),
                resources: Some(policy.policy.resources.clone()),
                runtime: policy.runtime,
                session_model: Some("per_input".to_string()),
                workload: Some(policy.workload.clone()),
            }),
            | (true, None) => Err(PlanError::Sandbox(
                "sandbox-enabled configuration requires a resolved operator policy".to_string(),
            )),
        }
    }
}
fn normalize_inputs(values: Vec<String>) -> Vec<Option<String>> {
    let values = values.into_iter().fold(Vec::new(), |mut unique, value| {
        if !unique.contains(&value) {
            unique.push(value);
        }
        unique
    });
    match values.is_empty() {
        | true => vec![None],
        | false => values.into_iter().map(Some).collect(),
    }
}
