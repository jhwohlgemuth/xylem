extern crate alloc;

use acorn::util::Label;
use acorn::With;
use acorn_core::util::alloc::StringConversion;
use acorn_core::util::{Checksum, MimeType, SemanticVersion};
use acorn_core::{Location, Scheme};
use acorn_host::fs::{file_checksum, make_executable};
use alloc::collections::BTreeSet;
use bon::Builder;
use convert_case::{Case, Casing};
use core::fmt;
use core::iter::once;
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs::{canonicalize, create_dir_all, read_to_string};
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::completion::{CompletionDecision, CompletionPlan};
use crate::error::{AcquisitionError, RequirementError};
use crate::execution::{ExecutionContext, InputBatch};
use crate::journal::{ExecutionState, RunJournal};
use crate::outcome::{ModuleOutcome, WorkflowOutcome};
use crate::plan::PlanError;
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState};
use crate::remote::{redact_url, RemotePolicy};
use crate::script::*;

mod validation;

/// Supported workflow configuration serialization formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFormat {
    /// JavaScript Object Notation.
    Json,
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
    /// A configuration module exceeds the supported nesting depth.
    #[error("configuration module nesting exceeds 16 levels at {}", path.join("::"))]
    Depth { path: Vec<String> },
    /// A configuration source recurs through its ancestor chain.
    #[error("configuration module cycle at {} ({location})", path.join("::"))]
    Cycle { path: Vec<String>, location: PathBuf },
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
}
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InputAttribute {
    Path(String),
    List(Vec<String>),
    ShellScript(ScriptTemplate),
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
    /// - a script that returns a list of file (or folder) paths (executed in the current working directory)
    ///
    /// If input is not provided, the workflow will run similar to a 0-adic function.
    pub input: Option<InputAttribute>,
    /// Workflow output
    ///
    /// Designates a path to the workflow output artifacts for verification and export purposes
    #[serde(alias = "artifacts")]
    pub output: Option<String>,
    pub options: Option<ConfigOptions>,
}
#[derive(Builder, Clone, Debug, Deserialize, Serialize, With)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct ConfigOptions {
    pub root: Option<PathBuf>,
    /// Maximum number of top-level workflow inputs that may run concurrently.
    pub batch: Option<InputBatch>,
    #[serde(alias = "dry-run")]
    pub dry_run: Option<bool>,
    pub offline: Option<bool>,
    /// Skip modules whose declared outputs and marker are current.
    pub resume: Option<bool>,
    #[serde(alias = "skip-checksum", alias = "skip-verify-checksum")]
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
impl Config {
    /// Resolve local configuration modules into their executable leaf modules.
    pub fn resolve(self, source: &Path) -> Result<Self, ConfigurationModuleError> {
        let source = canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        self.resolve_at(&source, Vec::new(), Vec::new(), 0)
    }
    /// Resolve local and remote configuration modules before planning.
    ///
    /// Remote documents are fetched only for a real run.
    /// Git sources are cloned under the caller-provided managed source root and must expose `config.json` at repository root.
    pub fn resolve_with_policy(
        self,
        source: &Path,
        policy: &RemotePolicy,
        source_root: &Path,
        dry_run: bool,
    ) -> Result<Self, ConfigurationModuleError> {
        self.resolve_with_policy_at(source, policy, source_root, dry_run, false)
    }
    fn resolve_with_policy_at(
        self,
        source: &Path,
        policy: &RemotePolicy,
        source_root: &Path,
        dry_run: bool,
        nested: bool,
    ) -> Result<Self, ConfigurationModuleError> {
        self.modules
            .into_iter()
            .try_fold(
                (Vec::new(), self.requirements.clone()),
                |(mut modules, mut requirements), module| match module.module_type {
                    | ModuleType::Configuration { uri } => {
                        let path = vec![module.name.clone()];
                        let uri_text = uri.uri().unwrap_or_else(|| <&str>::from(&uri).to_string());
                        let child = match ModuleUri::from(&uri) {
                            | ModuleUri::File(_) => ModuleUri::resolve_configuration(&uri, source)
                                .ok_or_else(|| ConfigurationModuleError::Source {
                                    module: module.name.clone(),
                                    uri: uri_text.clone(),
                                })
                                .and_then(|child_source| {
                                    Config::read(child_source.clone())
                                        .map_err(|source| ConfigurationModuleError::Read {
                                            module: module.name.clone(),
                                            source,
                                        })
                                        .and_then(|child| child.resolve_with_policy_at(&child_source, policy, source_root, dry_run, true))
                                }),
                            | ModuleUri::Url(value) if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS) => match dry_run
                            {
                                | true => Err(ConfigurationModuleError::RemoteDryRun {
                                    module: module.name.clone(),
                                    uri: uri_text.clone(),
                                }),
                                | false => policy
                                    .read_text(&uri_text)
                                    .map_err(|source| ConfigurationModuleError::Remote {
                                        module: module.name.clone(),
                                        reason: source.to_string(),
                                    })
                                    .and_then(|content| Self::read_remote_document(&content, &uri_text, &module.name)),
                            },
                            | ModuleUri::Url(_) => match dry_run {
                                | true => Err(ConfigurationModuleError::RemoteDryRun {
                                    module: module.name.clone(),
                                    uri: uri_text.clone(),
                                }),
                                | false => policy
                                    .probe_git(&uri_text)
                                    .map_err(|source| ConfigurationModuleError::Remote {
                                        module: module.name.clone(),
                                        reason: source.to_string(),
                                    })
                                    .and_then(|()| Self::clone_configuration(&uri, &uri_text, source_root, &module.name)),
                            },
                            | ModuleUri::Unknown(_) => Err(ConfigurationModuleError::Source {
                                module: module.name.clone(),
                                uri: uri_text,
                            }),
                        }?;
                        let variables = Self::merge(self.variables.as_ref(), module.variables.as_ref(), child.variables.as_ref());
                        modules.extend(child.modules.into_iter().map(|child| child.qualify(&path, variables.as_ref())));
                        child.requirements.into_iter().for_each(|requirement| {
                            if !requirements.contains(&requirement) {
                                requirements.push(requirement);
                            }
                        });
                        Ok((modules, requirements))
                    }
                    | module_type => {
                        let mut module = Module { module_type, ..module };
                        module.absolutize_container_mounts(source);
                        if nested {
                            module.absolutize(source);
                        }
                        modules.push(module);
                        Ok((modules, requirements))
                    }
                },
            )
            .map(|(modules, requirements)| Self {
                modules,
                requirements,
                ..self
            })
    }
    pub fn get_environment_variables(self, module: Option<Module>, input: Option<String>) -> HashMap<String, String> {
        let mut variables = self.variables.clone().unwrap_or_default();
        if !variables.contains_key("OUTPUT_DIRECTORY") {
            if let Some(output) = &self.output {
                variables.insert("OUTPUT_DIRECTORY".to_string(), Value::String(output.clone()));
            }
        }
        if let Some(module) = module {
            variables.extend(module.variables.unwrap_or_default());
        }
        variables.insert("MODULE_LIST".to_string(), Value::String(self.clone().get_module_list()));
        if let Some(value) = input {
            let parent = PathBuf::from(&value)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&value)
                .to_string();
            variables.insert("INPUT".to_string(), Value::String(value));
            variables.insert("PARENT_DIRECTORY".to_string(), Value::String(parent));
        }
        let envs = EnvironmentValue::expand(EnvironmentValue::get_from_serde(&variables));
        debug!("=> {} Environment variables: {:#?}", Label::using(), envs.dimmed());
        envs
    }
    /// Get inputs from input attribute for concurrent workflow execution
    ///
    /// See [InputAttribute](crate::InputAttribute) for more details
    ///
    pub fn get_inputs(self) -> Result<Option<Vec<String>>, PlanError> {
        self.resolve_inputs(None).map(|inputs| {
            let values = inputs.into_iter().flatten().collect::<Vec<_>>();
            match values.is_empty() {
                | true => None,
                | false => Some(values),
            }
        })
    }
    pub fn get_module_list(self) -> String {
        let names = self
            .modules
            .iter()
            .map(|module| module.module_path.join("::").to_case(Case::Snake))
            .collect::<Vec<String>>();
        names.join(",").to_string()
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
        self,
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
            RemotePolicy::from_hosts(options.allowed.as_deref())
                .map_err(|why| AcquisitionError::Policy {
                    source: io::Error::other(why),
                })
                .and_then(|policy| {
                    self.modules
                        .par_iter()
                        .enumerate()
                        .filter(|(index, _)| selected.contains(index))
                        .map(|(_, module)| {
                            ProgressPhase::Preparation.emit(observer, ProgressState::Started, &module.name, None);
                            let result = match &module.module_type {
                                | ModuleType::Binary { uri, checksum } => module.prepare_binary(uri.into(), checksum.as_deref(), &options, &policy),
                                | ModuleType::Script { programming_language, uri } => match programming_language {
                                    | Some(ModuleLanguage::Generic) | None => module.prepare_script(uri.into(), false, &options, &policy),
                                    | Some(ModuleLanguage::Python) => module.prepare_script(uri.into(), true, &options, &policy),
                                    | Some(language) => Err(format!("module {} uses unsupported script language {language}", module.name)),
                                },
                                | ModuleType::Configuration { .. } => Err(format!("module {} uses reserved configuration module type", module.name)),
                            };
                            match result {
                                | Ok(()) => ProgressPhase::Preparation.emit(observer, ProgressState::Finished, &module.name, None),
                                | Err(_) => ProgressPhase::Preparation.emit(observer, ProgressState::Failed, &module.name, None),
                            }
                            result.map_err(|why| AcquisitionError::Module {
                                module: module.name.clone(),
                                source: io::Error::other(why),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|_| self)
                })
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
                        let variables = self.clone().get_environment_variables(Some(module.clone()), input.clone());
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
        self.run_workflow_recorded(input, options, observer, None, 0)
    }
    /// Run workflow modules while emitting progress updates and durable module transitions.
    pub fn run_workflow_recorded(
        self,
        input: Option<String>,
        options: &ConfigOptions,
        observer: Option<&dyn ProgressObserver>,
        journal: Option<&RunJournal>,
        input_index: usize,
    ) -> WorkflowOutcome {
        EnvironmentValue::print_all();
        let execution = self.modules.iter().enumerate().try_fold(
            Vec::new(),
            |mut outcomes, (module_index, module)| -> Result<Vec<ModuleOutcome>, (Vec<ModuleOutcome>, String)> {
                let envs = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                let result = CompletionPlan::inspect(module, &envs, options.resume.unwrap_or_default(), self.output.as_deref())
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
                                ProgressPhase::Execution.emit(observer, ProgressState::Started, &module.name, input.as_deref());
                                match journal {
                                    | Some(journal) => journal
                                        .transition_module(input_index, module_index, ExecutionState::Running, None)
                                        .map_err(|why| why.to_string()),
                                    | None => Ok(()),
                                }
                                .and_then(|()| match &module.module_type {
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
                                                | ModuleUri::Url(_) => Ok(root.clone()),
                                                | ModuleUri::Unknown(_) => Err(format!("module URI is invalid: {uri}")),
                                            };
                                            working_dir.and_then(|working_dir| {
                                                Script::from_module(module.clone(), Some(PathBuf::from(root)))
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
                                                            | false => script.run_checked().map(|_| ()).map_err(|why| why.to_string()),
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
                                                    Script::from_module(module.clone(), root)
                                                        .map_err(|why| why.to_string())
                                                        .and_then(|loaded| {
                                                            let script = loaded
                                                                .expand_arguments_from(envs.clone())
                                                                .with_current_dir(working_dir.clone())
                                                                .with_envs(envs)
                                                                .with_container_run(options.container_run_id.as_deref())
                                                                .with_container_env(options.forward_container_env);
                                                            let configured = match programming_language {
                                                                | Some(ModuleLanguage::Python) => module
                                                                    .resolve_virtual_environment(&working_dir)
                                                                    .map(|virtual_environment| script.with_virtual_environment(virtual_environment))
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
                                                                | false => script.run_checked().map(|_| ()).map_err(|why| why.to_string()),
                                                            })
                                                        })
                                                })
                                        }),
                                    | ModuleType::Configuration { .. } => Err("configuration modules are not executable".to_string()),
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
                        ProgressPhase::Execution.emit(observer, progress_state, &module.name, input.as_deref());
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
                        let error = match journal {
                            | Some(journal) => match journal.transition_module(input_index, module_index, ExecutionState::Failed, Some(&why)) {
                                | Ok(()) => why,
                                | Err(journal_error) => format!("{why}; additionally, record failed module state: {journal_error}"),
                            },
                            | None => why,
                        };
                        ProgressPhase::Execution.emit(observer, ProgressState::Failed, &module.name, input.as_deref());
                        outcomes.push(ModuleOutcome {
                            index: module_index,
                            name: module.name.clone(),
                            module_path: module.module_path.clone(),
                            state: ExecutionState::Failed,
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
            | Err((outcomes, error)) => WorkflowOutcome::init()
                .maybe_input(input)
                .state(ExecutionState::Failed)
                .modules(outcomes)
                .error(error)
                .build(),
        }
    }
    /// Set environment variables from Xylem configuration "variables" attribute
    pub fn set_environment_variables(self) -> Config {
        let variables = self.clone().get_environment_variables(None, None);
        EnvironmentValue::set_namespaced_variables(variables);
        self
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
    fn clone_configuration(location: &Location, uri: &str, source_root: &Path, module: &str) -> Result<Self, ConfigurationModuleError> {
        let destination = source_root.join(location.hash());
        create_dir_all(source_root)
            .map_err(|source| ConfigurationModuleError::Remote {
                module: module.to_string(),
                reason: source.to_string(),
            })
            .and_then(|()| {
                Script::maybe_git_clone(uri.to_string(), source_root.display().to_string()).ok_or_else(|| ConfigurationModuleError::Source {
                    module: module.to_string(),
                    uri: uri.to_string(),
                })
            })
            .and_then(|script| {
                script.run_checked().map(|_| ()).map_err(|source| ConfigurationModuleError::Remote {
                    module: module.to_string(),
                    reason: source.to_string(),
                })
            })
            .and_then(|()| {
                let revision = match location {
                    | Location::Detailed { revision: Some(value), .. } => Some(value),
                    | Location::Simple(_) | Location::Detailed { revision: None, .. } => None,
                };
                revision.map_or(Ok(()), |revision| {
                    Script::init()
                        .command(Command::init().name("git".to_string()).build())
                        .arguments(vec!["checkout".to_string(), revision.clone()])
                        .current_dir(destination.display().to_string())
                        .build()
                        .run_checked()
                        .map(|_| ())
                        .map_err(|source| ConfigurationModuleError::Remote {
                            module: module.to_string(),
                            reason: source.to_string(),
                        })
                })
            })
            .and_then(|()| {
                let entrypoint = destination.join("config.json");
                Config::read(entrypoint).map_err(|source| ConfigurationModuleError::Read {
                    module: module.to_string(),
                    source,
                })
            })
    }
    fn read_remote_document(content: &str, uri: &str, module: &str) -> Result<Self, ConfigurationModuleError> {
        let path = PathBuf::from(uri);
        let format = match uri.rsplit_once('.') {
            | Some((_, "yaml" | "yml")) => ConfigFormat::Yaml,
            | _ => ConfigFormat::Json,
        };
        let parsed = match format {
            | ConfigFormat::Json => parse_json_configuration(content).map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>),
            | ConfigFormat::Yaml => parse_yaml_configuration(content).map_err(|source| Box::new(source) as Box<dyn core::error::Error + Send + Sync>),
        };
        parsed.map_err(|source| ConfigurationModuleError::Read {
            module: module.to_string(),
            source: ConfigReadError::Invalid { format, path, source },
        })
    }
    fn resolve_at(self, source: &Path, ancestors: Vec<PathBuf>, path: Vec<String>, depth: usize) -> Result<Self, ConfigurationModuleError> {
        match depth > 16 {
            | true => Err(ConfigurationModuleError::Depth { path }),
            | false => {
                let ancestors = ancestors.into_iter().chain(once(source.to_path_buf())).collect::<Vec<_>>();
                self.modules
                    .into_iter()
                    .try_fold(
                        (Vec::new(), self.requirements.clone()),
                        |(mut modules, mut requirements), module| match module.module_type {
                            | ModuleType::Configuration { uri } => {
                                let nested_path = path.iter().cloned().chain(once(module.name.clone())).collect::<Vec<_>>();
                                match (module.template.is_some(), module.completion.is_some()) {
                                    | (true, _) => Err(ConfigurationModuleError::Wrapper {
                                        module: module.name,
                                        field: "template",
                                    }),
                                    | (_, true) => Err(ConfigurationModuleError::Wrapper {
                                        module: module.name,
                                        field: "completion",
                                    }),
                                    | (false, false) => ModuleUri::resolve_configuration(&uri, source)
                                        .ok_or_else(|| ConfigurationModuleError::Source {
                                            module: module.name.clone(),
                                            uri: uri.uri().unwrap_or_else(|| <&str>::from(&uri).to_string()),
                                        })
                                        .and_then(|child_source| {
                                            let child_source = canonicalize(&child_source).unwrap_or(child_source);
                                            match ancestors.contains(&child_source) {
                                                | true => Err(ConfigurationModuleError::Cycle {
                                                    path: nested_path,
                                                    location: child_source,
                                                }),
                                                | false => Config::read(child_source.clone())
                                                    .map_err(|source| ConfigurationModuleError::Read {
                                                        module: module.name.clone(),
                                                        source,
                                                    })
                                                    .and_then(|child| {
                                                        child.resolve_at(&child_source, ancestors.clone(), nested_path.clone(), depth + 1)
                                                    })
                                                    .map(|child| {
                                                        let variables =
                                                            Self::merge(self.variables.as_ref(), module.variables.as_ref(), child.variables.as_ref());
                                                        let children = child
                                                            .modules
                                                            .into_iter()
                                                            .map(move |child_module| child_module.qualify(&nested_path, variables.as_ref()));
                                                        (children, child.requirements)
                                                    })
                                                    .map(|(children, child_requirements)| {
                                                        modules.extend(children);
                                                        child_requirements.into_iter().for_each(|requirement| {
                                                            if !requirements.contains(&requirement) {
                                                                requirements.push(requirement);
                                                            }
                                                        });
                                                        (modules, requirements)
                                                    }),
                                            }
                                        }),
                                }
                            }
                            | module_type => {
                                let mut module = Module { module_type, ..module };
                                if depth > 0 {
                                    module.absolutize(source);
                                }
                                modules.push(module);
                                Ok((modules, requirements))
                            }
                        },
                    )
                    .map(|(modules, requirements)| Self {
                        modules,
                        requirements,
                        ..self
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
                write!(formatter, "configuration {} must use a .json, .yaml, or .yml extension", path.display())
            }
        }
    }
}
impl From<ConfigFormat> for MimeType {
    fn from(format: ConfigFormat) -> Self {
        match format {
            | ConfigFormat::Json => Self::Json,
            | ConfigFormat::Yaml => Self::Yaml,
        }
    }
}
impl Module {
    fn prepare_binary(&self, uri: &str, checksum: Option<&str>, options: &ConfigOptions, policy: &RemotePolicy) -> Result<(), String> {
        match ModuleUri::from(&acorn_core::Location::from(uri)) {
            | ModuleUri::Url(value) if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS) => {
                let url = value.to_string();
                policy
                    .ensure_allowed(&url)
                    .map_err(|why| self.remote_error(&url, &why.to_string()))
                    .and_then(|_| self.skip_or_reject_offline(&url, options))
                    .and_then(|skip| match skip {
                        | true => Ok(()),
                        | false => policy
                            .probe_http(&url)
                            .map_err(|why| self.remote_error(&url, &why.to_string()))
                            .and_then(|()| {
                                debug!(module = self.name, url = redact_url(&url), "=> {} Remote binary", Label::found());
                                match options.dry_run.unwrap_or_default() {
                                    | true => {
                                        info!(
                                            module = self.name,
                                            url = redact_url(&url),
                                            "=> {} Download remote binary",
                                            Label::dry_run()
                                        );
                                        Ok(())
                                    }
                                    | false => options
                                        .root
                                        .as_deref()
                                        .ok_or_else(|| format!("module {} has no run root directory", self.name))
                                        .and_then(|root| policy.download(&url, root).map_err(|why| self.remote_error(&url, &why.to_string())))
                                        .and_then(|output| self.verify_downloaded_binary(&url, checksum, output, options)),
                                }
                            }),
                    })
            }
            | ModuleUri::Url(_) => Err(format!("module {} uses unsupported remote binary URI {}", self.name, redact_url(uri))),
            | ModuleUri::File(path) => match path.try_exists() {
                | Ok(true) if options.dry_run.unwrap_or_default() => {
                    info!(
                        module = self.name,
                        path = path.display().to_string(),
                        "=> {} Validate local binary",
                        Label::dry_run()
                    );
                    Ok(())
                }
                | Ok(true) => {
                    debug!(
                        module = self.name,
                        path = path.display().to_string(),
                        "=> {} Local binary",
                        Label::found()
                    );
                    match make_executable(path.clone()) {
                        | Ok(()) => {
                            info!(
                                module = self.name,
                                path = path.display().to_string(),
                                "=> {} Binary is executable",
                                Label::pass()
                            );
                            Ok(())
                        }
                        | Err(why) => Err(format!("module {} binary is not executable at {}: {why}", self.name, path.display())),
                    }
                }
                | Ok(false) => Err(format!("module {} local binary was not found: {}", self.name, path.display())),
                | Err(why) => Err(format!(
                    "module {} local binary cannot be inspected at {}: {why}",
                    self.name,
                    path.display()
                )),
            },
            | ModuleUri::Unknown(_) => Err(format!("module {} has invalid binary URI: {uri}", self.name)),
        }
    }
    fn verify_downloaded_binary(&self, url: &str, checksum: Option<&str>, output: PathBuf, options: &ConfigOptions) -> Result<(), String> {
        let verification = match options.skip_verify_checksum.unwrap_or_default() {
            | true => {
                warn!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Skipping checksum verification",
                    Label::pass()
                );
                Ok(())
            }
            | false => {
                let calculated = file_checksum(output.clone(), None);
                let expected = checksum.map(Checksum::from);
                match (checksum, expected.as_ref(), calculated.as_ref()) {
                    | (Some(_), Some(expected), Ok(calculated))
                        if expected.algorithm == calculated.algorithm && expected.checksum_value == calculated.checksum_value =>
                    {
                        info!(
                            module = self.name,
                            url = redact_url(url),
                            algorithm = %calculated.algorithm,
                            "=> {} Checksum verified",
                            Label::pass()
                        );
                        Ok(())
                    }
                    | (Some(value), _, Ok(calculated)) => Err(format!(
                        "module {} checksum {value} does not match {}:{} for {}",
                        self.name,
                        calculated.algorithm,
                        calculated.checksum_value,
                        redact_url(url)
                    )),
                    | (Some(value), _, Err(why)) => Err(format!(
                        "module {} could not calculate checksum {value} for {}: {why}",
                        self.name,
                        redact_url(url)
                    )),
                    | (None, _, _) => Err(format!("module {} requires a checksum for {}", self.name, redact_url(url))),
                }
            }
        };
        verification.and_then(|()| match make_executable(output) {
            | Ok(()) => {
                info!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Downloaded binary is executable",
                    Label::pass()
                );
                Ok(())
            }
            | Err(why) => Err(format!(
                "module {} downloaded binary is not executable for {}: {why}",
                self.name,
                redact_url(url)
            )),
        })
    }
    fn prepare_script(&self, uri: &str, python: bool, options: &ConfigOptions, policy: &RemotePolicy) -> Result<(), String> {
        match ModuleUri::from(&acorn_core::Location::from(uri)) {
            | ModuleUri::Url(value)
                if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS)
                    || value.scheme().as_str().eq_ignore_ascii_case("ssh") =>
            {
                let url = value.to_string();
                policy
                    .ensure_allowed(&url)
                    .map_err(|why| self.remote_error(&url, &why.to_string()))
                    .and_then(|_| self.skip_or_reject_offline(&url, options))
                    .and_then(|skip| match skip {
                        | true => Ok(()),
                        | false => policy
                            .probe_git(&url)
                            .map_err(|why| self.remote_error(&url, &why.to_string()))
                            .and_then(|()| {
                                debug!(
                                    module = self.name,
                                    url = redact_url(&url),
                                    "=> {} Remote script repository",
                                    Label::found()
                                );
                                options
                                    .root
                                    .as_ref()
                                    .ok_or_else(|| format!("module {} has no run root directory", self.name))
                                    .and_then(|root| {
                                        let current_dir = root.to_absolute_path();
                                        module_working_directory(uri, Some(root))
                                            .ok_or_else(|| format!("module {} has invalid URI: {}", self.name, redact_url(uri)))
                                            .map(|module_root| (current_dir, module_root))
                                    })
                                    .and_then(|(current_dir, module_root)| {
                                        match Script::maybe_git_clone(uri.to_string(), current_dir) {
                                            | Some(script) if options.dry_run.unwrap_or_default() => {
                                                info!(
                                                    module = self.name,
                                                    script = script.to_string(),
                                                    cwd = script.current_dir,
                                                    "=> {}",
                                                    Label::dry_run()
                                                );
                                                Ok(())
                                            }
                                            | Some(script) => script.run_checked().map(|_| ()).map_err(|why| why.to_string()),
                                            | None => Ok(()),
                                        }
                                        .and_then(|()| match python {
                                            | true => self.prepare_python_environment(module_root, options),
                                            | false => Ok(()),
                                        })
                                    })
                            }),
                    })
            }
            | ModuleUri::Url(_) => Err(format!("module {} uses unsupported remote script URI {}", self.name, redact_url(uri))),
            | ModuleUri::File(path) => match path.try_exists() {
                | Ok(true) => {
                    let path = path.to_absolute_path();
                    debug!(module = self.name, path, "=> {} Local script repository", Label::found());
                    match python {
                        | true => self.prepare_python_environment(path, options),
                        | false => Ok(()),
                    }
                }
                | Ok(false) => Err(format!("module {} local script was not found: {}", self.name, path.display())),
                | Err(why) => Err(format!(
                    "module {} local script cannot be inspected at {}: {why}",
                    self.name,
                    path.display()
                )),
            },
            | ModuleUri::Unknown(_) => Err(format!("module {} has invalid script URI: {uri}", self.name)),
        }
    }
    fn prepare_python_environment(&self, module_root: String, options: &ConfigOptions) -> Result<(), String> {
        self.resolve_virtual_environment(&module_root)
            .map_err(|why| why.to_string())
            .and_then(|environment| Script::maybe_create_virtual_environment(module_root, environment).map_err(|why| why.to_string()))
            .and_then(|script| match script {
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
            })
    }
    fn skip_or_reject_offline(&self, url: &str, options: &ConfigOptions) -> Result<bool, String> {
        match (options.offline.unwrap_or_default(), options.dry_run.unwrap_or_default()) {
            | (false, _) => Ok(false),
            | (true, true) => {
                info!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Remote module skipped in offline dry run",
                    Label::dry_run()
                );
                Ok(true)
            }
            | (true, false) => Err(format!(
                "module {} remote source is unavailable in offline mode: {}",
                self.name,
                redact_url(url)
            )),
        }
    }
    fn remote_error(&self, url: &str, why: &str) -> String {
        format!("module {} remote source {} failed: {why}", self.name, redact_url(url))
    }
}
impl Module {
    fn absolutize(&mut self, source: &Path) {
        let uri = match &mut self.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri } | ModuleType::Script { uri, .. } => uri,
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
    fn qualify(mut self, path: &[String], variables: Option<&Map<String, Value>>) -> Self {
        self.module_path = path.iter().chain(once(&self.name)).cloned().collect();
        self.variables = Config::merge(variables, self.variables.as_ref(), None);
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
