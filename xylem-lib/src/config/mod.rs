use acorn_lib::util::{download_binary, extension, make_executable, read_file, snake_case, Label, ToAbsoluteString};
use bon::Builder;
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use uriparse::Scheme;

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
    #[builder]
    pub root: Option<PathBuf>,
    #[builder]
    #[serde(alias = "dry-run")]
    pub dry_run: Option<bool>,
    #[builder]
    pub offline: Option<bool>,
    #[builder]
    #[serde(alias = "skip-checksum", alias = "skip-verify-checksum")]
    pub skip_verify_checksum: Option<bool>,
    #[builder]
    #[serde(alias = "skip-requirements", alias = "skip-verify-requirements")]
    pub skip_verify_requirements: Option<bool>,
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
    #[builder]
    pub uri: Option<String>,
    /// Options to pass to the checker
    #[builder]
    pub parameters: Option<Parameters>,
}
impl Config {
    pub fn get_environment_variables(self, module: Option<Module>, input: Option<String>) -> HashMap<String, String> {
        match input {
            | Some(value) => {
                let config_vars = self.variables.clone().unwrap_or_default();
                let module_vars = module.unwrap().variables.clone().unwrap_or_default();
                let mut envs = EnvironmentValue::get_from_serde(&config_vars);
                let parent = PathBuf::from(value.clone()).file_name().unwrap().to_str().unwrap().to_string();
                envs.extend(EnvironmentValue::get_from_serde(&module_vars));
                envs.insert("INPUT".to_string(), value);
                envs.insert("PARENT_DIRECTORY".to_string(), parent);
                debug!("=> {} Environment variables: {:#?}", Label::using(), envs.dimmed());
                envs
            }
            | None => HashMap::new(),
        }
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
                                let children = get_children(path.to_absolute_string());
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
        let names = self.modules.iter().map(|module| snake_case(module.name.clone())).collect::<Vec<String>>();
        names.join(",").to_string()
    }
    /// Prepare workflow modules for execution
    ///
    /// Module preparation is done in parallel and includes the following steps:
    /// - Download binary files and verify file integrity
    /// - Git clone script projects and setup virtual environments
    /// - Validate local and downloaded remote modules are ready for execution
    ///
    pub fn prepare_modules(self) -> Config {
        let options = match self.clone().options {
            | Some(value) => value,
            | None => {
                warn!("=> {} Configuration options", Label::not_found());
                ConfigOptions::init().build()
            }
        };
        self.modules.par_iter().for_each(|module| match &module.module_type {
            | ModuleType::Binary { uri, checksum } => match ModuleUri::from(uri) {
                | ModuleUri::Url(value) if !options.offline.unwrap_or_default() => match value.scheme() {
                    | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                        let url = value.to_string();
                        if ModuleUri::is_allowed(value) {
                            debug!(module = module.name, url, "=> {} Remote binary", Label::found());
                            info!(module = module.name, url, "=> {} Remote binary URL is on whitelist", Label::pass());
                        } else {
                            error!(module = module.name, url, "=> {} Not on whitelist", Label::rejected());
                            std::process::exit(exitcode::NOPERM);
                        }
                        // TODO: Check that URL exists
                        if !options.dry_run.unwrap_or_default() {
                            let output = match options.root.clone() {
                                | Some(value) => download_binary(&url, value),
                                | None => Err("Root directory not found".to_string()),
                            };
                            if !options.skip_verify_checksum.unwrap_or_default() {
                                let calculated_checksum = match output.clone() {
                                    | Ok(path) => match acorn_lib::util::checksum(path) {
                                        | Some(value) => value,
                                        | None => "".to_string(),
                                    },
                                    | _ => "".to_string(),
                                };
                                match checksum {
                                    | Some(value) if value == &calculated_checksum => {
                                        info!(module = module.name, url, "=> {} Checksum verified", Label::pass());
                                        if let Ok(path) = output {
                                            if make_executable(&path) {
                                                info!(module = module.name, url, "=> {} Downloaded binary is executable", Label::pass());
                                            } else {
                                                error!(
                                                    module = module.name,
                                                    url,
                                                    checksum,
                                                    "=> {} Downloaded binary is not executable",
                                                    Label::fail()
                                                );
                                                std::process::exit(exitcode::USAGE);
                                            }
                                        }
                                    }
                                    | Some(_) => {
                                        error!(
                                            module = module.name,
                                            url,
                                            checksum,
                                            calculated = calculated_checksum,
                                            "=> {} Remote binary checksum does not match",
                                            Label::fail()
                                        );
                                        std::process::exit(exitcode::USAGE);
                                    }
                                    | None => {
                                        error!(
                                            module = module.name,
                                            url,
                                            checksum,
                                            "=> {} Remote binary checksum does not match",
                                            Label::fail()
                                        );
                                        std::process::exit(exitcode::UNAVAILABLE);
                                    }
                                }
                            } else {
                                warn!(module = module.name, url, "=> {} Skipping checksum verification", Label::pass());
                            }
                        } else {
                            info!(module = module.name, url, "=> {} Download remote binary", Label::dry_run());
                        }
                    }
                    | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote binary modules"),
                },
                | ModuleUri::File(value) => match value.try_exists() {
                    | Ok(true) => {
                        let path = value.into_os_string().into_string().unwrap();
                        debug!(module = module.name, path, "=> {} Local binary", Label::found());
                        if make_executable(&PathBuf::from(path.clone())) {
                            info!(module = module.name, path, "=> {} Binary is executable", Label::pass());
                        } else {
                            error!(module = module.name, path, "=> {} Binary is not executable", Label::fail());
                            std::process::exit(exitcode::USAGE);
                        }
                    }
                    | _ => {
                        let path = value.into_os_string().into_string().unwrap();
                        error!(module = module.name, path, "=> {} Local binary", Label::not_found());
                        std::process::exit(exitcode::UNAVAILABLE);
                    }
                },
                | _ => unimplemented!("Only URLs and files are currently supported for binary modules"),
            },
            | ModuleType::Script { programming_language, uri } => match programming_language {
                | Some(ModuleLanguage::Generic) | None => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) if !options.offline.unwrap_or_default() => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            let url = value.to_string();
                            if ModuleUri::is_allowed(value) {
                                debug!(module = module.name, url, "=> {} Remote script repository", Label::found());
                                info!(module = module.name, url, "=> {} URL is on whitelist", Label::pass());
                            } else {
                                error!(module = module.name, url, "=> {} Not on whitelist", Label::rejected());
                                std::process::exit(exitcode::NOPERM);
                            }
                            // TODO: Check that URL exists
                            // TODO: Git clone module
                            // TODO: Check script repository with is_valid_xylem_module_python
                            unimplemented!("Remote Generic script modules are not supported yet");
                        }
                        | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote generic script modules"),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            debug!(module = module.name, path, "=> {} Local script repository", Label::found());
                            // TODO: Check script repository with is_valid_xylem_module_python
                        }
                        | _ => {
                            let path = value.into_os_string().into_string().unwrap();
                            error!(module = module.name, path, "=> {} Local script module", Label::not_found());
                            std::process::exit(exitcode::UNAVAILABLE);
                        }
                    },
                    | _ => unimplemented!("Only URLs and files are currently supported for Generic script modules"),
                },
                | Some(ModuleLanguage::Python) => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) if !options.offline.unwrap_or_default() => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            let url = value.to_string();
                            if ModuleUri::is_allowed(value) {
                                debug!(module = module.name, url, "=> {} Remote Python script repository", Label::found());
                                info!(module = module.name, url, "=> {} URL is on whitelist", Label::pass());
                            } else {
                                error!(module = module.name, url, "=> {} Not on whitelist", Label::rejected());
                                std::process::exit(exitcode::NOPERM);
                            }
                            // TODO: Check that URL exists
                            if ModuleUri::from(uri).test() {
                                info!(module = module.name, uri, "=> {}", Label::found());
                            } else {
                                error!(module = module.name, uri, "=> {}", Label::rejected());
                                std::process::exit(exitcode::USAGE);
                            };
                            let current_dir = match options.root.clone() {
                                | Some(value) => value.to_absolute_string(),
                                | None => {
                                    error!(module = module.name, "=> {} Root directory", Label::not_found());
                                    std::process::exit(exitcode::UNAVAILABLE);
                                }
                            };
                            let module_root = match options.root.clone() {
                                | Some(value) => value.join(ModuleUri::from(uri).hash()).to_absolute_string(),
                                | None => {
                                    error!(module = module.name, "=> {} Root directory", Label::not_found());
                                    std::process::exit(exitcode::UNAVAILABLE);
                                }
                            };
                            if let Some(script) = Script::maybe_git_clone(uri.clone(), current_dir) {
                                if options.dry_run.unwrap_or_default() {
                                    let cwd = script.clone().current_dir;
                                    warn!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                } else {
                                    script.run();
                                }
                            }
                            // TODO: Check script repository with is_valid_xylem_module_python
                            // TODO: Add support for designating virtual environment manager
                            // let manager = match &module.template {
                            //     | Some(TemplateAttribute::Template(ScriptTemplate { manager, .. })) => manager,
                            //     | Some(TemplateAttribute::Path(_)) => &None,
                            //     | _ => &None,
                            // };
                            if let Some(script) = Script::maybe_create_virtual_environment(module_root, None) {
                                if options.dry_run.unwrap_or_default() {
                                    let cwd = script.clone().current_dir;
                                    info!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                } else {
                                    script.run();
                                }
                            };
                        }
                        | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for Python script modules"),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            debug!(module = module.name, path, "=> {} Local Python script repository", Label::found());
                            // TODO: Check script repository with is_valid_xylem_module_python
                            // TODO: Add support for designating virtual environment manager
                            // let manager = match &module.template {
                            //     | Some(TemplateAttribute::Template(ScriptTemplate { manager, .. })) => manager,
                            //     | Some(TemplateAttribute::Path(_)) => &None,
                            //     | _ => &None,
                            // };
                            if let Some(script) = Script::maybe_create_virtual_environment(uri.to_string(), None) {
                                if options.dry_run.unwrap_or_default() {
                                    let cwd = script.clone().current_dir;
                                    info!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                } else {
                                    script.run();
                                }
                            };
                        }
                        | _ => {
                            let path = value.into_os_string().into_string().unwrap();
                            error!(module = module.name, path, "=> {} Local Python script module", Label::not_found(),);
                            std::process::exit(exitcode::UNAVAILABLE);
                        }
                    },
                    | _ => unimplemented!("Only URLs and files are currently supported for Python script modules"),
                },
                | _ => unimplemented!("Only Generic and Python script modules are currently supported"),
            },
            | _ => unimplemented!("Only binary and script modules are currently supported"),
        });
        self
    }
    pub fn read(path: PathBuf) -> Option<Config> {
        let content = match Extension::from_string(extension(&path)) {
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
            std::process::exit(exitcode::UNAVAILABLE);
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
        EnvironmentValue::print_all();
        self.modules.iter().for_each(|module| match &module.module_type {
            | ModuleType::Binary { .. } => {
                let envs = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                let working_dir = match options.root.clone() {
                    | Some(value) => value.to_absolute_string(),
                    | None => {
                        error!(module = module.name, "=> {} Root directory", Label::not_found());
                        std::process::exit(exitcode::UNAVAILABLE);
                    }
                };
                let loaded = Script::from_module(module.clone(), Some(PathBuf::from(working_dir.clone())));
                let script = loaded.ensure_relative().expand_arguments_from(envs).with_current_dir(working_dir);
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
                            std::process::exit(exitcode::UNAVAILABLE);
                        }
                    };
                    let working_dir = ModuleUri::from(uri).working_directory(root.clone());
                    let loaded = Script::from_module(module.clone(), root);
                    let script = loaded.expand_arguments_from(envs).with_current_dir(working_dir);
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
                            std::process::exit(exitcode::UNAVAILABLE);
                        }
                    };
                    let working_dir = ModuleUri::from(uri).working_directory(root.clone());
                    let virtual_environment = match &module.template {
                        | Some(TemplateAttribute::Path(value)) => {
                            ScriptTemplate::from_path(PathBuf::from(&working_dir).join(value)).virtual_environment
                        }
                        | Some(TemplateAttribute::Template(value)) => value.virtual_environment.clone(),
                        | None => ScriptTemplate::resolve_python_virtual_environment(PathBuf::from(working_dir.clone())),
                    };
                    let script = Script::from_module(module.clone(), root)
                        .expand_arguments_from(envs)
                        .with_current_dir(working_dir)
                        .with_virtual_environment(virtual_environment);
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
        });
    }
    /// Set environment variables from Xylem configuration "variables" attribute
    pub fn set_environment_variables(self) -> Config {
        match &self.variables {
            | Some(variables) => {
                for (k, v) in variables {
                    debug!("Setting Xylem environment variable: {:?} = {:?}", k, v);
                    EnvironmentValue::from_value(v.clone()).set_variable(k.clone());
                }
            }
            | None => (),
        }
        let module_list = EnvironmentValue::String(self.clone().get_module_list());
        module_list.set_variable("MODULE_LIST".to_string());
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
                        std::process::exit(exitcode::UNAVAILABLE);
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
        | Ok(value) => value.flatten().map(|x| x.path().to_absolute_string()).collect::<Vec<_>>(),
        | Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
