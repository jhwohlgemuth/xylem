//! Deterministic, side-effect-free execution plans
extern crate alloc;

use crate::completion::{CompletionDecision, CompletionError, CompletionPlan};
use crate::config::{Config, InputAttribute};
use crate::script::{module_working_directory, Module, ModuleType, ModuleUri, Script, TemplateAttribute};
use acorn_schema::validation::ValidationReport;
use alloc::collections::BTreeMap;
use bon::Builder;
use core::fmt;
use serde::Serialize;
use std::fs::read_dir;
use std::path::PathBuf;
use tracing::debug;

/// Failure to validate or render an execution plan.
#[derive(Debug)]
pub enum PlanError {
    /// Completion resolution or observation failed.
    Completion(CompletionError),
    /// Input discovery could not produce a deterministic input set.
    Input(String),
    /// Semantic configuration validation failed.
    Validation(ValidationReport),
}
/// Versioned dry-run plan rendered without mutating the host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutionPlan {
    /// Configuration used to build the plan.
    pub configuration: PlanConfiguration,
    /// Ordered module decisions for every input.
    pub modules: Vec<PlannedModule>,
    /// Whether resume observations influenced decisions.
    pub resume: bool,
    /// Execution-plan schema version.
    pub schema_version: u32,
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
/// Acquisition operation required before a module can execute.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedAcquisition {
    /// Stable action vocabulary.
    pub action: String,
    /// Configured module source URI.
    pub source: String,
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
/// One planned module decision.
#[derive(Builder, Clone, Debug, Eq, PartialEq, Serialize)]
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
    /// Whether the decision includes a read-only completion observation.
    pub observed_completion: bool,
    /// Effective variables after workflow, output-default, module, and input precedence.
    pub variables: BTreeMap<String, String>,
}
/// Resolve CLI-over-configuration input precedence with deterministic duplicate handling.
impl Config {
    /// Resolve CLI-over-configuration input precedence with deterministic duplicate handling.
    pub fn resolve_inputs(&self, cli_inputs: Option<&[String]>) -> Result<Vec<Option<String>>, PlanError> {
        let values = match cli_inputs.filter(|values| !values.is_empty()) {
            | Some(values) => Ok(values.to_vec()),
            | None => match &self.input {
                | Some(InputAttribute::List(values)) => Ok(values.clone()),
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
                    }),
                | Some(InputAttribute::ShellScript(_)) => Err(PlanError::Input("shell input is not supported".to_string())),
                | None => Ok(Vec::new()),
            },
        };
        values.map(|mut values| {
            if matches!(self.input, Some(InputAttribute::Path(_))) && cli_inputs.filter(|values| !values.is_empty()).is_none() {
                values.sort();
            }
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
        })
    }
}
impl ExecutionPlan {
    /// Build a plan using only configuration and explicitly labeled read-only completion observations.
    pub fn build(config: &Config, path: String, inputs: &[Option<String>], resume: bool) -> Result<Self, PlanError> {
        config.validate_for_inputs(inputs).map_err(PlanError::from).and_then(|()| {
            inputs
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
                    configuration: PlanConfiguration {
                        id: config.id.clone(),
                        output: config.output.clone(),
                        path,
                        version: config.version.clone(),
                    },
                    modules,
                    resume,
                    schema_version: 1,
                })
        })
    }
}
impl Module {
    fn command(&self, variables: &BTreeMap<String, String>) -> PlannedCommand {
        let uri = match &self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri } | ModuleType::Script { uri, .. } => uri,
        };
        let working_directory = module_working_directory(uri, None);
        match &self.template {
            | Some(TemplateAttribute::Template(template)) => {
                let variables = variables.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
                let script = Script::from_template(template.clone()).expand_arguments_from(variables);
                PlannedCommand {
                    arguments: script.arguments,
                    program: Some(script.command.name),
                    template_source: None,
                    working_directory,
                }
            }
            | Some(TemplateAttribute::Path(path)) => PlannedCommand {
                arguments: Vec::new(),
                program: None,
                template_source: working_directory
                    .as_ref()
                    .map(|directory| PathBuf::from(directory).join(path).display().to_string()),
                working_directory,
            },
            | None => PlannedCommand {
                arguments: Vec::new(),
                program: None,
                template_source: working_directory
                    .as_ref()
                    .map(|directory| PathBuf::from(directory).join("template.json").display().to_string()),
                working_directory,
            },
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
            .clone()
            .get_environment_variables(Some(self.clone()), input.clone())
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let hash_variables = variables.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        CompletionPlan::resolve(self, &hash_variables, config.output.as_deref())
            .map_err(PlanError::from)
            .and_then(|resolved| {
                let decision = match resume {
                    | true => CompletionPlan::inspect(self, &hash_variables, true, config.output.as_deref())
                        .map_err(PlanError::from)
                        .map(|plan| match plan {
                            | Some(plan) if plan.decision == CompletionDecision::SkipExisting => "would_skip_existing",
                            | Some(_) | None => "would_run",
                        }),
                    | false => Ok("would_run"),
                };
                decision.map(|decision| {
                    let completion = resolved.map(|resolved| PlannedCompletion {
                        marker: resolved.marker.display().to_string(),
                        outputs: resolved.outputs.iter().map(|output| output.display().to_string()).collect(),
                    });
                    PlannedModule::init()
                        .acquisition(self.clone().into())
                        .command(self.command(&variables))
                        .maybe_completion(completion)
                        .decision(decision)
                        .input_index(input_index)
                        .maybe_input(input.clone())
                        .module_index(module_index)
                        .module(self.name.as_str())
                        .observed_completion(resume && self.completion.is_some())
                        .variables(variables)
                        .build()
                })
            })
    }
}
impl core::error::Error for PlanError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Completion(error) => Some(error),
            | Self::Input(_) => None,
            | Self::Validation(error) => Some(error),
        }
    }
}
impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Completion(error) => error.fmt(formatter),
            | Self::Input(error) => formatter.write_str(error),
            | Self::Validation(error) => error.fmt(formatter),
        }
    }
}
impl From<CompletionError> for PlanError {
    fn from(error: CompletionError) -> Self {
        Self::Completion(error)
    }
}
impl From<ValidationReport> for PlanError {
    fn from(error: ValidationReport) -> Self {
        Self::Validation(error)
    }
}
impl From<Module> for PlannedAcquisition {
    fn from(module: Module) -> Self {
        let (action, source) = match module.module_type {
            | ModuleType::Binary { uri, .. } => match ModuleUri::from(uri.as_str()) {
                | ModuleUri::File(_) => ("use_local", uri),
                | ModuleUri::Url(_) => ("download", uri),
                | ModuleUri::Unknown(_) => ("reject", uri),
            },
            | ModuleType::Configuration { uri } => ("reject", uri),
            | ModuleType::Script { uri, .. } => match ModuleUri::from(uri.as_str()) {
                | ModuleUri::File(_) => ("use_local", uri),
                | ModuleUri::Url(_) => ("clone", uri),
                | ModuleUri::Unknown(_) => ("reject", uri),
            },
        };
        Self {
            action: action.to_string(),
            source,
        }
    }
}
