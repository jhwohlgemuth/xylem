use acorn::io::{file_checksum, make_executable, read_file};
use acorn::util::Label;
use acorn_core::util::alloc::StringConversion;
use acorn_core::util::{file_extension, Checksum};
use acorn_core::Scheme;
use bon::Builder;
use convert_case::{Case, Casing};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::exit;
use tracing::{debug, error, info, warn};

use crate::execution::ExecutionContext;
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState};
use crate::remote::{redact_url, RemotePolicy};
use crate::script::*;

enum Extension {
    Json,
    Yaml,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InputAttribute {
    Path(String),
    List(Vec<String>),
    ShellScript(ScriptTemplate),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
pub struct Config {
    /// Unique workflow identifier
    pub id: Option<String>,
    /// Configuration version in semver format
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
#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[builder(start_fn(name = init))]
pub struct ConfigOptions {
    pub root: Option<PathBuf>,
    #[serde(alias = "dry-run")]
    pub dry_run: Option<bool>,
    pub offline: Option<bool>,
    #[serde(alias = "skip-checksum", alias = "skip-verify-checksum")]
    pub skip_verify_checksum: Option<bool>,
    #[serde(alias = "skip-requirements", alias = "skip-verify-requirements")]
    pub skip_verify_requirements: Option<bool>,
    /// Exact remote module hosts permitted by this workflow.
    pub allowed: Option<Vec<String>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
pub struct Rule {
    /// Name of the rule (displayed by the progress bar)
    #[builder(default)]
    pub name: String,
    pub uri: Option<String>,
    /// Options to pass to the checker
    pub parameters: Option<Parameters>,
}
impl Config {
    pub fn get_environment_variables(self, module: Option<Module>, input: Option<String>) -> HashMap<String, String> {
        let mut variables = self.variables.clone().unwrap_or_default();
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
    pub fn get_inputs(self) -> Option<Vec<String>> {
        match self.input {
            | Some(value) => match value {
                | InputAttribute::Path(value) => {
                    let path = PathBuf::from(value);
                    match path.try_exists() {
                        | Ok(true) => {
                            if path.is_dir() {
                                let children = get_children(path.to_absolute_path());
                                Some(children)
                            } else {
                                error!(path = path.to_str().unwrap(), "=> {} Input directory", Label::not_found());
                                None
                            }
                        }
                        | _ => {
                            error!(path = path.to_str().unwrap(), "=> {} Input directory", Label::not_found());
                            None
                        }
                    }
                }
                | InputAttribute::List(value) => Some(value),
                | InputAttribute::ShellScript(_script) => {
                    // TODO: Execute shell script and return Vec<String> of files (or folders)
                    unimplemented!("Shell script workflow input");
                }
            },
            | None => None,
        }
    }
    pub fn get_module_list(self) -> String {
        let names = self
            .modules
            .iter()
            .map(|module| module.name.to_case(Case::Snake))
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
    pub fn prepare_modules(self, execution: &ExecutionContext, observer: Option<&dyn ProgressObserver>) -> Config {
        execution.install(|| self.prepare_modules_in_pool(observer))
    }
    fn prepare_modules_in_pool(self, observer: Option<&dyn ProgressObserver>) -> Config {
        let options = match self.clone().options {
            | Some(value) => value,
            | None => {
                warn!("=> {} Configuration options", Label::not_found());
                ConfigOptions::init().build()
            }
        };
        let policy = RemotePolicy::from_hosts(options.allowed.as_deref()).unwrap_or_else(|why| {
            error!(error = why.to_string(), "=> {} Remote module allowlist", Label::invalid());
            exit(exitcode::USAGE);
        });
        self.modules.par_iter().for_each(|module| {
            ProgressPhase::Preparation.emit(observer, ProgressState::Started, &module.name, None);
            match &module.module_type {
                | ModuleType::Binary { uri, checksum } => module.prepare_binary(uri, checksum.as_deref(), &options, &policy),
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | Some(ModuleLanguage::Generic) | None => module.prepare_script(uri, false, &options, &policy),
                    | Some(ModuleLanguage::Python) => module.prepare_script(uri, true, &options, &policy),
                    | Some(language) => {
                        error!(
                            module = module.name,
                            language = language.to_string(),
                            "=> {} Script language",
                            Label::invalid()
                        );
                        exit(exitcode::USAGE);
                    }
                },
                | ModuleType::Configuration { .. } => {
                    error!(module = module.name, "=> {} Configuration modules are reserved", Label::invalid());
                    exit(exitcode::USAGE);
                }
            }
            ProgressPhase::Preparation.emit(observer, ProgressState::Finished, &module.name, None);
        });
        self
    }
    pub fn read(path: PathBuf) -> Option<Config> {
        let content = match Extension::from_string(file_extension(path.to_string_lossy().to_string()).unwrap_or_default()) {
            | Extension::Json => match Config::read_json(path.clone()) {
                | Ok(value) => Some(value),
                | Err(_) => None,
            },
            | Extension::Yaml => match Config::read_yaml(path.clone()) {
                | Ok(value) => Some(value),
                | Err(_) => None,
            },
            | Extension::Unknown => unimplemented!("Unknown configuration file extension"),
        };
        if let Some(content) = content {
            Some(content)
        } else {
            error!(path = path.to_str().unwrap(), "=> {} Cannot import configuration", Label::fail());
            exit(exitcode::UNAVAILABLE);
        }
    }
    /// Import Xylem configuration JSON file
    ///
    /// See the [Xylem configuration syntax wiki page](https://code.ornl.gov/groups/research-enablement/xylem/-/wikis/Xylem-Standards/Configuration-Syntax)
    /// for more information configuration files
    ///
    pub fn read_json(path: PathBuf) -> serde_json::Result<Config> {
        let content = match read_file(path.clone()) {
            | Ok(value) if !value.is_empty() => value,
            | Ok(_) | Err(_) => {
                error!(path = path.to_str().unwrap(), "=> {} Configuration content is not valid", Label::fail());
                "{}".to_owned()
            }
        };
        let data: serde_json::Result<Config> = serde_json::from_str(&content);
        let label = match data {
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
    pub fn read_yaml(path: PathBuf) -> serde_yml::Result<Config> {
        let content = match read_file(path.clone()) {
            | Ok(value) => value,
            | Err(_) => {
                error!(path = path.to_str().unwrap(), "=> {} Configuration content is not valid", Label::fail());
                "".to_owned()
            }
        };
        let data: serde_yml::Result<Config> = serde_yml::from_str(&content);
        let label = match data {
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
    pub fn run_workflow(self, input: Option<String>, options: &ConfigOptions) {
        self.run_workflow_with_progress(input, options, None);
    }
    /// Run workflow modules while emitting progress updates.
    pub fn run_workflow_with_progress(self, input: Option<String>, options: &ConfigOptions, observer: Option<&dyn ProgressObserver>) {
        EnvironmentValue::print_all();
        self.modules.iter().for_each(|module| {
            ProgressPhase::Execution.emit(observer, ProgressState::Started, &module.name, input.as_deref());
            match &module.module_type {
                | ModuleType::Binary { .. } => {
                    let envs = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                    let working_dir = match options.root.clone() {
                        | Some(value) => value.to_absolute_path(),
                        | None => {
                            error!(module = module.name, "=> {} Root directory", Label::not_found());
                            exit(exitcode::UNAVAILABLE);
                        }
                    };
                    let loaded = Script::from_module(module.clone(), Some(PathBuf::from(working_dir.clone())));
                    let script = loaded
                        .ensure_relative()
                        .expand_arguments_from(envs.clone())
                        .with_current_dir(working_dir)
                        .with_envs(envs);
                    if options.dry_run.unwrap_or_default() {
                        info!(
                            module = module.name,
                            script = script.to_string(),
                            cwd = script.current_dir,
                            "=> {}",
                            Label::dry_run()
                        );
                    } else {
                        script.run();
                    }
                }
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | Some(ModuleLanguage::Generic) | None => {
                        let envs = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                        let root = match options.root.clone() {
                            | Some(value) => Some(value),
                            | None => {
                                error!(module = module.name, "=> {} Root directory", Label::not_found());
                                exit(exitcode::UNAVAILABLE);
                            }
                        };
                        let working_dir = module_working_directory(uri, root.as_ref()).unwrap_or_else(|| {
                            error!(module = module.name, uri, "=> {} Module URI", Label::invalid());
                            exit(exitcode::USAGE);
                        });
                        let loaded = Script::from_module(module.clone(), root);
                        let script = loaded.expand_arguments_from(envs.clone()).with_current_dir(working_dir).with_envs(envs);
                        if options.dry_run.unwrap_or_default() {
                            info!(
                                module = module.name,
                                script = script.to_string(),
                                cwd = script.current_dir,
                                "=> {}",
                                Label::dry_run()
                            );
                        } else {
                            script.run();
                        }
                    }
                    | Some(ModuleLanguage::Python) => {
                        let envs = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                        let root = match options.root.clone() {
                            | Some(value) => Some(value),
                            | None => {
                                error!(module = module.name, "=> {} Root directory", Label::not_found());
                                exit(exitcode::UNAVAILABLE);
                            }
                        };
                        let working_dir = module_working_directory(uri, root.as_ref()).unwrap_or_else(|| {
                            error!(module = module.name, uri, "=> {} Module URI", Label::invalid());
                            exit(exitcode::USAGE);
                        });
                        let virtual_environment = module.resolve_virtual_environment(&working_dir);
                        let script = Script::from_module(module.clone(), root)
                            .expand_arguments_from(envs.clone())
                            .with_current_dir(working_dir)
                            .with_virtual_environment(virtual_environment)
                            .with_envs(envs);
                        if options.dry_run.unwrap_or_default() {
                            info!(
                                module = module.name,
                                script = script.to_string(),
                                cwd = script.current_dir,
                                "=> {}",
                                Label::dry_run()
                            );
                        } else {
                            script.run();
                        }
                    }
                    | _ => unimplemented!("Only Generic and Python script modules are currently supported"),
                },
                | _ => unimplemented!("Only binary and script modules are currently supported"),
            }
            ProgressPhase::Execution.emit(observer, ProgressState::Finished, &module.name, input.as_deref());
        });
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
    pub fn verify_requirements(self) -> Config {
        if let Some(options) = &self.options {
            if !options.skip_verify_requirements.unwrap_or_default() {
                self.requirements.iter().for_each(|name| {
                    let command = Command::init().name(name.clone()).build();
                    if !command.test() {
                        error!(name, "=> {} Requirement not met", Label::not_found());
                        exit(exitcode::UNAVAILABLE);
                    }
                });
            } else {
                warn!("Skipping requirements verification");
            }
        }
        self
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
}

impl Module {
    fn prepare_binary(&self, uri: &str, checksum: Option<&str>, options: &ConfigOptions, policy: &RemotePolicy) {
        match ModuleUri::from(uri) {
            | ModuleUri::Url(value) if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS) => {
                let url = value.to_string();
                policy
                    .ensure_allowed(&url)
                    .unwrap_or_else(|why| self.remote_failure(&url, why.to_string()));
                match self.skip_or_reject_offline(&url, options) {
                    | true => (),
                    | false => {
                        policy.probe_http(&url).unwrap_or_else(|why| self.remote_failure(&url, why.to_string()));
                        debug!(module = self.name, url = redact_url(&url), "=> {} Remote binary", Label::found());
                        match options.dry_run.unwrap_or_default() {
                            | true => info!(
                                module = self.name,
                                url = redact_url(&url),
                                "=> {} Download remote binary",
                                Label::dry_run()
                            ),
                            | false => {
                                let root = options.root.as_deref().unwrap_or_else(|| {
                                    error!(module = self.name, "=> {} Root directory", Label::not_found());
                                    exit(exitcode::UNAVAILABLE);
                                });
                                let output = policy
                                    .download(&url, root)
                                    .unwrap_or_else(|why| self.remote_failure(&url, why.to_string()));
                                self.verify_downloaded_binary(&url, checksum, output, options);
                            }
                        }
                    }
                }
            }
            | ModuleUri::Url(_) => {
                error!(
                    module = self.name,
                    uri = redact_url(uri),
                    "=> {} Remote binary URI scheme",
                    Label::invalid()
                );
                exit(exitcode::USAGE);
            }
            | ModuleUri::File(path) => match path.try_exists() {
                | Ok(true) if options.dry_run.unwrap_or_default() => info!(
                    module = self.name,
                    path = path.display().to_string(),
                    "=> {} Validate local binary",
                    Label::dry_run()
                ),
                | Ok(true) => {
                    debug!(
                        module = self.name,
                        path = path.display().to_string(),
                        "=> {} Local binary",
                        Label::found()
                    );
                    match make_executable(path.clone()) {
                        | true => info!(
                            module = self.name,
                            path = path.display().to_string(),
                            "=> {} Binary is executable",
                            Label::pass()
                        ),
                        | false => {
                            error!(
                                module = self.name,
                                path = path.display().to_string(),
                                "=> {} Binary is not executable",
                                Label::fail()
                            );
                            exit(exitcode::USAGE);
                        }
                    }
                }
                | Ok(false) | Err(_) => {
                    error!(
                        module = self.name,
                        path = path.display().to_string(),
                        "=> {} Local binary",
                        Label::not_found()
                    );
                    exit(exitcode::UNAVAILABLE);
                }
            },
            | ModuleUri::Unknown(_) => {
                error!(module = self.name, uri, "=> {} Binary module URI", Label::invalid());
                exit(exitcode::USAGE);
            }
        }
    }
    fn verify_downloaded_binary(&self, url: &str, checksum: Option<&str>, output: PathBuf, options: &ConfigOptions) {
        match options.skip_verify_checksum.unwrap_or_default() {
            | true => warn!(
                module = self.name,
                url = redact_url(url),
                "=> {} Skipping checksum verification",
                Label::pass()
            ),
            | false => {
                let calculated = file_checksum(output.clone(), None);
                let expected = checksum.map(Checksum::from);
                match (checksum, expected.as_ref(), calculated.as_ref()) {
                    | (Some(_), Some(expected), Some(calculated))
                        if expected.algorithm == calculated.algorithm && expected.checksum_value == calculated.checksum_value =>
                    {
                        info!(
                            module = self.name,
                            url = redact_url(url),
                            algorithm = %calculated.algorithm,
                            "=> {} Checksum verified",
                            Label::pass()
                        );
                    }
                    | (Some(value), _, Some(calculated)) => {
                        error!(
                            module = self.name,
                            url = redact_url(url),
                            checksum = value,
                            algorithm = %calculated.algorithm,
                            calculated = calculated.checksum_value,
                            "=> {} Remote binary checksum does not match",
                            Label::fail()
                        );
                        exit(exitcode::USAGE);
                    }
                    | (Some(value), _, None) => {
                        error!(
                            module = self.name,
                            url = redact_url(url),
                            checksum = value,
                            "=> {} Could not calculate remote binary checksum",
                            Label::fail()
                        );
                        exit(exitcode::USAGE);
                    }
                    | (None, _, _) => {
                        error!(
                            module = self.name,
                            url = redact_url(url),
                            "=> {} Remote binary checksum is required",
                            Label::fail()
                        );
                        exit(exitcode::UNAVAILABLE);
                    }
                }
            }
        }
        match make_executable(output) {
            | true => info!(
                module = self.name,
                url = redact_url(url),
                "=> {} Downloaded binary is executable",
                Label::pass()
            ),
            | false => {
                error!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Downloaded binary is not executable",
                    Label::fail()
                );
                exit(exitcode::USAGE);
            }
        }
    }
    fn prepare_script(&self, uri: &str, python: bool, options: &ConfigOptions, policy: &RemotePolicy) {
        match ModuleUri::from(uri) {
            | ModuleUri::Url(value)
                if matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS)
                    || value.scheme().as_str().eq_ignore_ascii_case("ssh") =>
            {
                let url = value.to_string();
                policy
                    .ensure_allowed(&url)
                    .unwrap_or_else(|why| self.remote_failure(&url, why.to_string()));
                match self.skip_or_reject_offline(&url, options) {
                    | true => (),
                    | false => {
                        policy.probe_git(&url).unwrap_or_else(|why| self.remote_failure(&url, why.to_string()));
                        debug!(
                            module = self.name,
                            url = redact_url(&url),
                            "=> {} Remote script repository",
                            Label::found()
                        );
                        let root = options.root.as_ref().unwrap_or_else(|| {
                            error!(module = self.name, "=> {} Root directory", Label::not_found());
                            exit(exitcode::UNAVAILABLE);
                        });
                        let current_dir = root.to_absolute_path();
                        let module_root = module_working_directory(uri, Some(root)).unwrap_or_else(|| {
                            error!(module = self.name, uri = redact_url(uri), "=> {} Module URI", Label::invalid());
                            exit(exitcode::USAGE);
                        });
                        if let Some(script) = Script::maybe_git_clone(uri.to_string(), current_dir) {
                            match options.dry_run.unwrap_or_default() {
                                | true => info!(
                                    module = self.name,
                                    script = script.to_string(),
                                    cwd = script.current_dir,
                                    "=> {}",
                                    Label::dry_run()
                                ),
                                | false => {
                                    script.run();
                                }
                            }
                        }
                        if python {
                            self.prepare_python_environment(module_root, options);
                        }
                    }
                }
            }
            | ModuleUri::Url(_) => {
                error!(
                    module = self.name,
                    uri = redact_url(uri),
                    "=> {} Remote script URI scheme",
                    Label::invalid()
                );
                exit(exitcode::USAGE);
            }
            | ModuleUri::File(path) => match path.try_exists() {
                | Ok(true) => {
                    let path = path.to_absolute_path();
                    debug!(module = self.name, path, "=> {} Local script repository", Label::found());
                    if python {
                        self.prepare_python_environment(path, options);
                    }
                }
                | Ok(false) | Err(_) => {
                    error!(
                        module = self.name,
                        path = path.display().to_string(),
                        "=> {} Local script module",
                        Label::not_found()
                    );
                    exit(exitcode::UNAVAILABLE);
                }
            },
            | ModuleUri::Unknown(_) => {
                error!(module = self.name, uri, "=> {} Script module URI", Label::invalid());
                exit(exitcode::USAGE);
            }
        }
    }
    fn prepare_python_environment(&self, module_root: String, options: &ConfigOptions) {
        let environment = self.resolve_virtual_environment(&module_root);
        if let Some(script) = Script::maybe_create_virtual_environment(module_root, environment) {
            match options.dry_run.unwrap_or_default() {
                | true => info!(
                    module = self.name,
                    script = script.to_string(),
                    cwd = script.current_dir,
                    "=> {}",
                    Label::dry_run()
                ),
                | false => {
                    script.run();
                }
            }
        }
    }
    fn skip_or_reject_offline(&self, url: &str, options: &ConfigOptions) -> bool {
        match (options.offline.unwrap_or_default(), options.dry_run.unwrap_or_default()) {
            | (false, _) => false,
            | (true, true) => {
                info!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Remote module skipped in offline dry run",
                    Label::dry_run()
                );
                true
            }
            | (true, false) => {
                error!(
                    module = self.name,
                    url = redact_url(url),
                    "=> {} Remote module is unavailable in offline mode",
                    Label::fail()
                );
                exit(exitcode::UNAVAILABLE);
            }
        }
    }
    fn remote_failure(&self, url: &str, why: String) -> ! {
        error!(
            module = self.name,
            url = redact_url(url),
            error = why,
            "=> {} Remote module preflight",
            Label::fail()
        );
        exit(exitcode::UNAVAILABLE);
    }
}

impl Extension {
    pub fn from_string(value: String) -> Extension {
        match value.as_str() {
            | "json" => Extension::Json,
            | "yaml" => Extension::Yaml,
            | _ => Extension::Unknown,
        }
    }
}
/// Get list of directory children
///
/// Only first level children are returned
///
fn get_children(path: String) -> Vec<String> {
    match fs::read_dir(path) {
        | Ok(value) => value.flatten().map(|x| x.path().to_absolute_path()).collect::<Vec<_>>(),
        | Err(_) => Vec::new(),
    }
}
#[cfg(test)]
mod tests;
