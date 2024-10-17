#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::wrong_self_convention)]
use bon::builder;
use clap::ValueEnum;
use color_eyre::eyre::{Report, Result};
use derive_more::{Display, FromStr};
use fancy_regex::Regex;
use is_executable::IsExecutable;
use is_terminal::IsTerminal;
use owo_colors::{OwoColorize, Style, Styled};
use rayon::prelude::*;
use serde::ser::StdError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::copy;
use std::io::Cursor;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::{
    env::{set_var, vars},
    fs, io,
};
use tokio::time::{sleep, Duration, Sleep};
use tracing::{debug, error, info, trace, warn};
use uriparse::Scheme;
use uriparse::URI;
use valuable_derive::Valuable;
use which::which;

use std::process;

pub type CommandResult = Result<(), Box<(dyn StdError + 'static)>>;
pub type Void = Result<(), Report>;
pub type Output = Vec<Option<Details>>;

#[derive(Clone, Debug, Display, Serialize, Deserialize, Valuable)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntime {
    #[display("docker")]
    #[serde(alias = "Docker")]
    Docker,
    #[display("podman")]
    #[serde(alias = "Podman")]
    Podman,
    #[display("apptainer")]
    #[serde(alias = "Apptainer", alias = "Singularity", alias = "singularity")]
    Apptainer,
    #[display("unknown")]
    Unknown,
}
/// Values that from Serde that can be set as environment variables
#[derive(Clone, Debug, Display)]
pub enum EnvironmentValue {
    String(String),
    Number(Number),
    Bool(bool),
    Unknown,
}
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
#[derive(Clone, Debug, Serialize, Deserialize, Valuable)]
#[serde(untagged)]
pub enum Instruction {
    /// Shell arguments as a string
    /// ```text
    /// "-m lib --input {{ INPUT_DIRECTORY }} --output {{ OUTPUT_DIRECTORY }}"
    /// ```
    ShellFormat(String),
    /// Shell arguments as a list of strings
    /// ```json
    /// [
    ///   "-m",
    ///   "lib",
    ///   "--input",
    ///   "{{ INPUT_DIRECTORY }}",
    ///   "--output",
    ///   "{{ OUTPUT_DIRECTORY }}"
    /// ]
    /// ```
    ExecFormat(Vec<String>),
}
#[derive(Clone, Debug, Serialize, Deserialize, Display, Valuable)]
#[serde(rename_all = "lowercase")]
pub enum ModuleLanguage {
    Binary,
    Csharp,
    Generic,
    JavaScript,
    Julia,
    Python,
    Rust,
}
/// Designates module type which specifies necessary attributes
#[derive(Clone, Debug, Serialize, Deserialize, Valuable)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModuleType {
    /// Compiled "black boxes" that are used via a command line interface
    #[serde(alias = "bin")]
    Binary { checksum: Option<String>, uri: String },
    /// Nested workflow module that allows for grouping modules
    #[serde(alias = "config")]
    Configuration { uri: String },
    /// Provides legacy support for exceuting scripts (e.g. Python projects like [Module A](https://code.ornl.gov/GSHS/common/pipe/module-a))
    Script {
        #[serde(alias = "programmingLanguage")]
        programming_language: Option<ModuleLanguage>,
        uri: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleUri<'a> {
    /// File URIs can be absolute or relative.
    ///
    /// Examples:
    ///
    /// Absolute file path
    /// ```json
    /// "modules": {
    ///   "uri": "file:///root/dev/module-a"
    /// }
    /// ```
    ///
    /// Relative file path
    /// ```json
    /// "modules": {
    ///   "uri": "file:./module-a"
    /// }
    /// ```
    File(PathBuf),
    /// URLs can use HTTP, HTTPS, or SSH schemes.
    ///
    /// Examples:
    ///
    /// URL (HTTPS)
    /// ```json
    /// "modules": {
    ///   "uri": "https://code.ornl.gov/GSHS/GDS/Common/PIPE/module-a"
    /// }
    /// ```
    ///
    /// URL (SSH)
    /// ```json
    /// "modules": {
    ///   "uri": "ssh://git@code.ornl.gov/GSHS/GDS/Common/PIPE/module-a"
    /// }
    /// ```
    //TODO: Implement Serialize/Deserialize for URI - https://serde.rs/impl-serialize.html
    Url(URI<'a>),
    Unknown(String),
}
#[derive(ValueEnum, Copy, Clone, Debug, Display, PartialEq, Eq)]
pub enum ReporterType {
    Console,
    Json,
    Stdout,
}
pub enum StdinIsPipedResult {
    StdinIsPiped,
    StdinIsNotPiped,
}
pub enum StdoutIsPipedResult {
    StdoutIsPiped,
    StdoutIsNotPiped,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TemplateAttribute {
    Path(String),
    Template(ScriptTemplate),
}
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum TemplateType {
    /// Example: `"some <%= value %>"`
    #[serde(alias = "ejs")]
    EmbeddedJavaScript,
    /// Example: `"some <%= value %>"`
    #[serde(alias = "erb")]
    EmbeddedRuby,
    /// Example: `"some {{ value }}"`
    #[serde(alias = "handlebars", alias = "hbs")]
    Handlebars,
    /// Example: `"some {{ value }}"`
    #[serde(alias = "mustache")]
    Mustache,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Display, FromStr, PartialEq, Serialize)]
pub enum VirtualEnvironmentManager {
    /// [Conda package manager](https://github.com/conda/conda)
    #[display("conda")]
    #[serde(alias = "conda")]
    Conda,
    /// Simple Python version management (https://github.com/pyenv/pyenv)
    #[display("pyenv")]
    #[serde(alias = "pyenv")]
    Pyenv,
    /// Tool to create isolated Python environments (https://virtualenv.pypa.io/en/latest/)
    #[display("venv")]
    #[serde(alias = "venv")]
    Virtualenv,
    /// [Pixi package manager](https://github.com/prefix-dev/pixi/) (that has functional parity with Conda)
    #[display("pixi")]
    #[serde(alias = "pixi")]
    Pixi,
    /// [Poetry](https://python-poetry.org/) - Python packaging and dependency management made easy
    #[display("poetry")]
    #[serde(alias = "poetry")]
    Poetry,
    #[display("unknown")]
    Unknown,
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
#[derive(Clone, Debug, Display)]
#[display("{}", name)]
#[builder(start_fn(name = init))]
pub struct Command {
    /// Command name (ex. "echo")
    #[builder(default)]
    pub name: String,
    /// Virtual environment struct that contains details about the associated virtual environment
    #[builder]
    pub virtual_environment: Option<VirtualEnvironment>,
    /// Container struct that contains details about the associated containerized environment
    #[builder]
    pub container: Option<ContainerEnvironment>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
/// Struct for using and sharing containerized environments
///
/// Examples:
/// ```ignore
/// use pipe_lib::ContainerEnvironment;
///
/// let container = ContainerEnvironment::init().name("myenv").build();
/// let container = ContainerEnvironment::init().name("myenv").runtime(ContainerRuntime::Podman).build();
/// ```
#[derive(Clone, Debug, Deserialize, Display, Serialize)]
#[display("{}", "{runtime}:{image}")]
#[builder(start_fn(name = init))]
pub struct ContainerEnvironment {
    /// Name of container
    pub name: String,
    /// Runtime (ex. "docker", "podman", etc...)
    #[builder(default=ContainerRuntime::Docker)]
    pub runtime: ContainerRuntime,
    /// Context name to use (defaults to active context)
    #[builder]
    pub context: Option<String>,
    /// Name of image container was built from
    #[builder]
    pub image: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialOrd, Ord)]
pub struct Details {
    // Results of applying check to given path
    pub condition: bool,
    // Description of what was checked (typically a file/folder path)
    pub description: Option<String>,
    // Explanation of why the check failed (null when condition is true)
    pub error: Option<String>,
}
/// Struct for using and sharing colorized logging labels
pub struct Label {}
#[derive(Clone, Debug, Deserialize, Display, Serialize)]
#[display("{name}")]
pub struct Module {
    pub name: String,
    #[serde(flatten)]
    pub module_type: ModuleType,
    pub description: Option<String>,
    pub variables: Option<Map<String, Value>>,
    pub before_script: Option<Instruction>,
    pub after_script: Option<Instruction>,
    pub template: Option<TemplateAttribute>,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PyProjectToml {
    #[serde(flatten)]
    inner: pyproject_toml::PyProjectToml,
    tool: Option<PyProjectTool>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectTool {
    pipe: Option<ToolPipe>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ToolPipe {
    module_name: String,
}
#[derive(Clone, Debug)]
pub struct Reporter {
    /// Reporter type name
    pub name: ReporterType,
    /// Function to run the reporter
    pub run: fn(Output, Option<Parameters>) -> Void,
    /// Options to pass to the reporter
    pub parameters: Option<Parameters>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
/// Semantic version
///
/// see <https://semver.org/>
#[derive(Clone, Debug, Deserialize, Display, Serialize)]
#[display("{}.{}.{}", major, minor, patch)]
#[builder(start_fn(name = init))]
pub struct SemanticVersion {
    /// Version when you make incompatible API changes
    #[builder(default = 0)]
    pub major: u32,
    /// Version when you add functionality in a backward compatible manner
    #[builder(default = 0)]
    pub minor: u32,
    /// Version when you make backward compatible bug fixes
    #[builder(default = 0)]
    pub patch: u32,
}
/// Script struct
///
/// Uses [bon](https://elastio.github.io/bon/) crate to implement builder pattern.
///
/// bon provides intuitive default values and allows for streamlined API.
///
/// Example:
/// ```
/// use pipe_lib::{Command, Script};
///
/// let command = Command::init().name("echo".to_string()).build();
/// let arguments = vec!["hello".to_string()];
/// let script = Script::init().command(command).arguments(arguments).build();
/// ```
#[derive(Clone, Debug, Display)]
#[display("{} {}", self.get_command(), self.get_arguments().join(" "))]
#[builder(start_fn(name = init))]
pub struct Script {
    #[builder(default = Command::init().build())]
    pub command: Command,
    #[builder(default = bon::vec![])]
    pub arguments: Vec<String>,
    /// Directory in which the script should be executed when run method is invoked
    #[builder(default = ".".to_string())]
    pub current_dir: String,
    #[builder]
    pub envs: Option<HashMap<String, String>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptTemplate {
    pub command: String,
    pub container: Option<ContainerEnvironment>,
    #[serde(alias = "environment")]
    pub virtual_environment: Option<VirtualEnvironment>,
    pub arguments: Instruction,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StringTemplate {
    pub value: String,
    pub template_type: TemplateType,
}
#[derive(Clone, Debug, Deserialize, Display, PartialEq, Serialize)]
#[display("{name}")]
#[builder(start_fn(name = init))]
pub struct VirtualEnvironment {
    pub name: String,
    #[builder]
    pub path: Option<String>,
    #[builder(default=VirtualEnvironmentManager::Conda)]
    pub manager: VirtualEnvironmentManager,
}
impl Command {
    /// Check if a command exists in current command line context
    ///
    /// Can also be used to check if a command exists within a [conda](https://conda.io/projects/conda/en/latest/user-guide/getting-started.html) virtual environment
    ///
    /// # Examples
    /// ```ignore
    /// use pipe_lib::Command;
    ///
    /// let command = Command::init().name("ls".to_string()).build();
    /// assert!(command.test());
    /// ```
    pub fn test(self) -> bool {
        match which(&self.name) {
            | Ok(value) => {
                let path = value.to_str().unwrap();
                match value.clone().try_exists() {
                    | Ok(true) => {
                        debug!(path, "=> {} Command test", Label::found());
                        true
                    }
                    | _ => {
                        debug!(path, "=> {} Command test", Label::not_found());
                        false
                    }
                }
            }
            | Err(_) => {
                error!(name = self.name, "=> {} Command test", Label::fail());
                false
            }
        }
    }
}
impl Config {
    pub fn get_environment_variables(self, module: Option<Module>, input: Option<String>) -> HashMap<String, String> {
        let config_vars = self.variables.clone().unwrap_or_default();
        let module_vars = module.unwrap().variables.clone().unwrap_or_default();
        let mut envs = EnvironmentValue::get_from_serde(&config_vars);
        envs.extend(EnvironmentValue::get_from_serde(&module_vars));
        envs.insert("INPUT".to_string(), input.unwrap_or_default());
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
                                let children = get_children(path_to_string(path));
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
    fn get_module_list(self) -> String {
        let names = self
            .modules
            .iter()
            .map(|module| to_snake_case(module.name.clone()))
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
    pub fn prepare_modules(self, options: &ConfigOptions) -> Config {
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
                                | Some(value) => download_binary(url.clone(), value),
                                | None => Err("Root directory not found".to_string()),
                            };
                            if !options.skip_verify_checksum.unwrap_or_default() {
                                let calculated_checksum = match output.clone() {
                                    | Ok(path) => get_checksum(path),
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
                            // TODO: Check script repository with is_valid_pipe_module_python
                            unimplemented!("Remote Generic script modules are not supported yet");
                        }
                        | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote generic script modules"),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            debug!(module = module.name, path, "=> {} Local script repository", Label::found());
                            // TODO: Check script repository with is_valid_pipe_module_python
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
                                | Some(value) => path_to_string(value),
                                | None => {
                                    error!(module = module.name, "=> {} Root directory", Label::not_found());
                                    std::process::exit(exitcode::UNAVAILABLE);
                                }
                            };
                            let module_root = match options.root.clone() {
                                | Some(value) => path_to_string(value.join(ModuleUri::from(uri).hash())),
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
                            // TODO: Check script repository with is_valid_pipe_module_python
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
                            // TODO: Check script repository with is_valid_pipe_module_python
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
        let content = match Extension::from_string(get_extension(&path)) {
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
    /// Import PIPE configuration JSON file
    ///
    /// See the [PIPE configuration syntax wiki page](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Configuration-Syntax)
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
    /// Import PIPE configuration YAML file
    ///
    /// See the [PIPE configuration syntax wiki page](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Configuration-Syntax)
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
                    | Some(value) => path_to_string(value),
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
    /// Set environment variables from PIPE configuration "variables" attribute
    pub fn set_environment_variables(self) -> Config {
        match &self.variables {
            | Some(variables) => {
                for (k, v) in variables {
                    debug!("Setting PIPE environment variable: {:?} = {:?}", k, v);
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
    pub fn verify_requirements(self, options: &ConfigOptions) -> Config {
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
        self
    }
    /// Verify hardware resources (ex. GPU) are available for the workflow on host system
    ///
    /// Under construction
    ///
    pub fn verify_resources(self, _options: &ConfigOptions) -> Config {
        self
    }
    // pub fn with_options(mut self, options: ConfigOptions) -> Config {
    //     self.options = options;
    //     return self
    // }
}
impl PartialEq for Details {
    fn eq(&self, other: &Self) -> bool {
        (self.description == other.description) && (self.condition == other.condition)
    }
}
impl EnvironmentValue {
    pub const PREFIX: &'static str = "pipe";
    fn formatted_prefix() -> String {
        format!("_{}_", EnvironmentValue::PREFIX.to_uppercase())
    }
    /// Convert serde Value to EnvironmentValue
    ///
    fn from_value(value: Value) -> EnvironmentValue {
        match value {
            | Value::String(val) => EnvironmentValue::String(val),
            | Value::Number(val) => EnvironmentValue::Number(val),
            | Value::Bool(val) => EnvironmentValue::Bool(val),
            | _ => EnvironmentValue::Unknown,
        }
    }
    fn prepend_prefix(value: String) -> String {
        let prefixed = format!("{}{}", EnvironmentValue::formatted_prefix(), value);
        prefixed
    }
    fn trim_prefix(key: String) -> String {
        let pre = EnvironmentValue::formatted_prefix();
        key.trim_start_matches(&pre).to_string()
    }
    pub fn get_from_serde(values: &Map<String, Value>) -> HashMap<String, String> {
        let mut lookup: HashMap<String, String> = HashMap::new();
        values.iter().for_each(|(key, value)| {
            let prefixed = EnvironmentValue::prepend_prefix(key.to_string());
            let coerced = match value {
                | Value::String(val) => val.to_string(),
                | Value::Number(val) => val.to_string(),
                | Value::Bool(val) => val.to_string(),
                | _ => String::new(),
            };
            lookup.insert(prefixed, coerced);
        });
        lookup
    }
    pub fn get_all() -> Vec<(String, String)> {
        let pre = EnvironmentValue::formatted_prefix();
        vars()
            .filter(|(key, _)| key.starts_with(&pre))
            .map(|(key, value)| {
                let key = key.trim_start_matches(&pre).to_string();
                (key, value)
            })
            .collect()
    }
    pub fn print_all() {
        EnvironmentValue::get_all().into_iter().for_each(|(key, value)| {
            trace!(
                "=> {} {}{} = {}",
                Label::using(),
                EnvironmentValue::formatted_prefix().dimmed(),
                key,
                value
            );
        });
    }
    pub fn set_variable(self, key: String) {
        let prefixed = EnvironmentValue::prepend_prefix(key);
        let prefixed_key = OsStr::new(&prefixed);
        match self {
            | EnvironmentValue::String(value) => set_var(prefixed_key, value),
            | EnvironmentValue::Number(value) => set_var(prefixed_key, value.to_string()),
            | EnvironmentValue::Bool(value) => set_var(prefixed_key, value.to_string()),
            | EnvironmentValue::Unknown => (),
        }
    }
    pub fn set_variables(values: HashMap<String, String>) {
        values
            .into_iter()
            .for_each(|(key, value)| EnvironmentValue::String(value).set_variable(key));
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
impl Label {
    pub fn dry_run() -> Styled<&'static &'static str> {
        let style = Style::new().black().on_yellow();
        " DRY_RUN ■ ".style(style)
    }
    pub fn invalid() -> String {
        Label::fmt_invalid("INVALID")
    }
    pub fn fmt_invalid(value: &str) -> String {
        let style = Style::new().red().on_default_color();
        value.style(style).to_string()
    }
    pub fn fail() -> String {
        Label::fmt_fail("FAIL")
    }
    pub fn fmt_fail(value: &str) -> String {
        let style = Style::new().white().on_red();
        format!(" ✗ {} ", value).style(style).to_string()
    }
    pub fn found() -> Styled<&'static &'static str> {
        let style = Style::new().green().on_default_color();
        "FOUND".style(style)
    }
    pub fn not_found() -> String {
        Label::fmt_not_found("NOT_FOUND")
    }
    pub fn fmt_not_found(value: &str) -> String {
        let style = Style::new().red().on_default_color();
        value.style(style).to_string()
    }
    pub fn output() -> String {
        Label::fmt_output("OUTPUT")
    }
    pub fn fmt_output(value: &str) -> String {
        let style = Style::new().cyan().dimmed().on_default_color();
        value.style(style).to_string()
    }
    pub fn pass() -> Styled<&'static &'static str> {
        let style = Style::new().green().on_default_color();
        "☑  PASS".style(style)
    }
    pub fn read() -> Styled<&'static &'static str> {
        let style = Style::new().green().on_default_color();
        "READ".style(style)
    }
    pub fn rejected() -> Styled<&'static &'static str> {
        let style = Style::new().red().on_default_color();
        "🛑 REJECTED ".style(style)
    }
    pub fn run() -> Styled<&'static &'static str> {
        let style = Style::new().black().on_yellow();
        " RUN ▶ ".style(style)
    }
    pub fn using() -> String {
        Label::fmt_using("USING")
    }
    pub fn fmt_using(value: &str) -> String {
        let style = Style::new().cyan();
        value.style(style).to_string()
    }
}
impl ModuleUri<'_> {
    pub fn from(value: &str) -> ModuleUri<'_> {
        match URI::try_from(value) {
            | Ok(uri) => match uri.scheme() {
                | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => ModuleUri::Url(uri),
                | Scheme::File => ModuleUri::File(PathBuf::from(uri.path().to_string())),
                | _ => ModuleUri::Unknown(value.to_string()),
            },
            | Err(_) => ModuleUri::Unknown(value.to_string()),
        }
    }
    pub fn hash(&self) -> String {
        match self {
            | ModuleUri::Url(uri) => {
                let host = match uri.host() {
                    | Some(value) => value.clone().to_string().replace('.', "_"),
                    | None => "".to_string(),
                };
                let segments = uri
                    .path()
                    .segments()
                    .iter()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if segments.is_empty() {
                    host.to_string()
                } else {
                    format!("{}_{}", host, segments[segments.len() - 2..].join("_").to_lowercase())
                }
            }
            | ModuleUri::File(value) => value.clone().into_os_string().into_string().unwrap(),
            | ModuleUri::Unknown(_) => unimplemented!(),
        }
    }
    pub fn get_whitelist() -> Vec<String> {
        to_string(vec!["code.ornl.gov", "code-int.ornl.gov"])
    }
    pub fn is_allowed(uri: URI) -> bool {
        ModuleUri::get_whitelist().contains(&uri.host().unwrap().to_string())
    }
    pub fn test(&self) -> bool {
        async fn check(_url: String) -> Sleep {
            sleep(Duration::from_secs(2))
        }
        match self {
            | ModuleUri::Url(uri) => match uri.scheme() {
                | Scheme::SSH if ModuleUri::is_allowed(uri.clone()) => true,
                | _ if ModuleUri::is_allowed(uri.clone()) => {
                    // let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                    // let _ = runtime.block_on(check(uri.to_string()));
                    true
                }
                | _ => false,
            },
            | ModuleUri::File(value) => value.clone().exists(),
            | ModuleUri::Unknown(_) => unimplemented!(),
        }
    }
    pub fn working_directory(&self, root: Option<PathBuf>) -> String {
        match self {
            | ModuleUri::Url(_) => match root {
                | Some(value) => value.join(self.hash()).into_os_string().into_string().unwrap(),
                | None => self.hash(),
            },
            | ModuleUri::File(value) => value.clone().into_os_string().into_string().unwrap(),
            | ModuleUri::Unknown(value) => value.to_string(),
        }
    }
}
impl std::ops::Deref for PyProjectToml {
    type Target = pyproject_toml::PyProjectToml;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl PyProjectToml {
    pub fn read(path: PathBuf) -> Result<Self, toml::de::Error> {
        let content = match read_file(path.clone()) {
            | Ok(value) if !value.is_empty() => value,
            | Ok(_) | Err(_) => {
                error!(path = path.to_str().unwrap(), "=> {} PyProject.toml content is not valid", Label::fail());
                "{}".to_owned()
            }
        };
        let data: Result<PyProjectToml, toml::de::Error> = toml::from_str(&content);
        let label = match data {
            | Ok(_) => Label::output(),
            | Err(_) => Label::invalid(),
        };
        trace!("=> {} PyProject.toml = {:#?}", label, data.dimmed());
        data
    }
    pub fn get_module_name(self) -> Option<String> {
        match self.tool {
            | Some(PyProjectTool { pipe, .. }) => match pipe {
                | Some(ToolPipe { module_name, .. }) => Some(module_name),
                | None => None,
            },
            | None => None,
        }
    }
}
impl SemanticVersion {
    pub fn from_string(version: &str) -> SemanticVersion {
        let mut parts = version.split('.');
        let major = parts.next().unwrap().parse::<u32>().unwrap();
        let minor = parts.next().unwrap().parse::<u32>().unwrap();
        let patch = parts.next().unwrap().parse::<u32>().unwrap();
        SemanticVersion { major, minor, patch }
    }
}
impl Script {
    pub fn ensure_relative(mut self) -> Script {
        let path = PathBuf::from(self.command.name);
        self.command.name = add_dot_slash(path).to_str().unwrap().to_string();
        self
    }
    /// Expand environment variables in the script arguments from environment
    ///
    /// This method does not set environment variables, but only interpolates the values within the script arguments
    ///
    /// Example:
    /// ```
    /// use pipe_lib::{to_string, Command, Script};
    ///
    /// // Environment: ANSWER=42
    /// let command = Command::init().name("echo".to_string()).build();
    /// let arguments = to_string(vec!["{{ ANSWER }}"]);
    /// let script = Script::init().command(command).arguments(arguments).build().expand_arguments();
    /// // "echo 42"
    /// ```
    pub fn expand_arguments(mut self) -> Script {
        let expanded = self
            .arguments
            .into_iter()
            .map(|argument| StringTemplate::new(&argument))
            .map(|argument| {
                if StringTemplate::is_handlebars(argument.clone()) {
                    let vars = EnvironmentValue::get_all();
                    argument.expand(vars)
                } else {
                    argument.value.clone()
                }
            })
            .collect();
        self.arguments = expanded;
        self
    }
    /// Expand environment variables in the script arguments from a HashMap parameter
    ///
    /// This method does not set environment variables, but only interpolates the values within the script arguments
    ///
    /// Example:
    /// ```
    /// use pipe_lib::{Command, Script};
    /// use std::collections::HashMap;
    ///
    /// let envs = HashMap::from([("ANSWER".to_string(), "42".to_string())]);
    /// let command = Command::init().name("echo".to_string()).build();
    /// let arguments = vec!["hello".to_string()];
    /// let script: Script = Script::init()
    ///     .command(command)
    ///     .arguments(arguments)
    ///     .build()
    ///     .expand_arguments_from(envs);
    /// ```
    pub fn expand_arguments_from(mut self, envs: HashMap<String, String>) -> Script {
        let expanded = self
            .arguments
            .into_iter()
            .map(|argument| StringTemplate::new(&argument))
            .map(|argument| {
                if StringTemplate::is_handlebars(argument.clone()) {
                    let vars = envs
                        .clone()
                        .into_iter()
                        .map(|(key, value)| (EnvironmentValue::trim_prefix(key), value))
                        .collect::<Vec<_>>();
                    argument.expand(vars)
                } else {
                    argument.value.clone()
                }
            })
            .collect();
        self.arguments = expanded;
        self
    }
    /// TODO: Factor out root argument?
    /// Create a script from a Module.
    ///
    /// Tries to create script from template interface. Resolves template interface from following:
    ///
    /// - IF `template` attribute exists in workflow configuration file
    ///   - path -> Script::from_path
    ///   - object -> Script::from_template
    ///
    /// - ELSE `template` attribute does NOT exist in workflow configuration file
    ///   - use "template.json", if it exists
    ///   - use default script if "template.json" does not exist
    ///
    /// ```
    pub fn from_module(module: Module, root: Option<PathBuf>) -> Script {
        const DEFAULT_FILENAME: &str = "template.json";
        let module_uri = match module.module_type {
            | ModuleType::Binary { ref uri, .. } => ModuleUri::from(uri),
            | ModuleType::Script { ref uri, .. } => ModuleUri::from(uri),
            | ModuleType::Configuration { .. } => unimplemented!("Configuration module type"),
        };
        let working_dir = module_uri.working_directory(Some(root.unwrap_or_default().clone()));
        let default_template_path = PathBuf::from(&working_dir).join(DEFAULT_FILENAME);
        match &module.template {
            | Some(attribute) => match attribute {
                | TemplateAttribute::Path(value) => match PathBuf::from(&working_dir).join(value).try_exists() {
                    | Ok(true) => {
                        let path = PathBuf::from(&working_dir).join(value);
                        debug!(module = module.name, path = path.to_str().unwrap(), "=> {} Template file", Label::using());
                        Script::from_path(path)
                    }
                    | _ => {
                        let path = PathBuf::from(&working_dir).join(value).into_os_string().into_string().unwrap();
                        warn!(module = module.name, "=> {} {}", Label::not_found(), path);
                        warn!(module = module.name, "=> {} Default template file path", Label::using());
                        Script::from_path(default_template_path)
                    }
                },
                | TemplateAttribute::Template(value) => {
                    debug!(module = module.name, "=> {} Template attribute", Label::using());
                    Script::from_template(value.clone())
                }
            },
            | None => match default_template_path.try_exists() {
                | Ok(true) => {
                    warn!(module = module.name, "=> {} Default template file path", Label::using());
                    Script::from_path(default_template_path)
                }
                | _ => {
                    warn!(
                        module = module.name,
                        "=> {} {}",
                        Label::not_found(),
                        default_template_path.into_os_string().into_string().unwrap()
                    );
                    warn!(module = module.name, "=> {} Default Python script", Label::using());
                    Script::python_default()
                }
            },
        }
    }
    /// Create a script from a template JSON file in [ScriptTemplate] format.
    ///
    /// You can easily import the script with
    /// ```
    /// use pipe_lib::Script;
    /// use std::path::PathBuf;
    ///
    /// let path = PathBuf::from("../tests/fixtures/template.json");
    /// let script = Script::from_path(path);
    /// let output = script.run();
    /// ```
    pub fn from_path(path: PathBuf) -> Script {
        let template = ScriptTemplate::from_path(path);
        Script::from_template(template)
    }
    /// Create a script from a ScriptTemplate struct.
    ///
    /// You can easily import the script with
    /// ```
    /// use pipe_lib::Script;
    /// use std::path::PathBuf;
    ///
    /// let path = PathBuf::from("../tests/fixtures/template.json");
    /// let script = Script::from_path(path);
    /// let _ = script.run();
    /// ```
    pub fn from_template(template: ScriptTemplate) -> Script {
        let arguments = match template.arguments {
            | Instruction::ShellFormat(value) => StringTemplate::new(&value)
                .remove_spaces()
                .value
                .split_whitespace()
                .map(|s| s.to_owned())
                .collect::<Vec<_>>(),
            | Instruction::ExecFormat(values) => values
                .into_iter()
                .map(|value| StringTemplate::new(&value).remove_spaces().value)
                .collect(),
        };
        let command = Command::init()
            .name(template.command)
            .maybe_container(template.container)
            .maybe_virtual_environment(template.virtual_environment)
            .build();
        Script::init().command(command).arguments(arguments).build()
    }
    fn get_command(&self) -> String {
        let name = match &self.command.virtual_environment {
            | Some(VirtualEnvironment { manager, .. }) => manager.to_string(),
            | _ => self.command.name.clone(),
        };
        match &self.command.container {
            | Some(ContainerEnvironment { runtime, .. }) => runtime.to_string(),
            | _ => name,
        }
    }
    fn get_arguments(&self) -> Vec<String> {
        let self_arguments = match &self.command.virtual_environment {
            | Some(VirtualEnvironment { name, manager, .. }) => {
                let mut arguments = match manager {
                    | VirtualEnvironmentManager::Conda => to_string(vec!["run", "--name", name, "--no-capture-output", &self.command.name]),
                    | VirtualEnvironmentManager::Pixi => to_string(vec!["run", "--environment", name, &self.command.name]),
                    | VirtualEnvironmentManager::Poetry => to_string(vec!["run", &self.command.name]),
                    | _ => {
                        error!(name = manager.to_string(), "=> {} Virtual environment manager", Label::not_found());
                        vec![]
                    }
                };
                arguments.extend(self.arguments.clone());
                arguments
            }
            | _ => self.arguments.clone(),
        };
        match &self.command.container {
            | Some(ContainerEnvironment { name, runtime, .. }) => {
                let wrapped_command = match &self.command.virtual_environment {
                    | Some(VirtualEnvironment { manager, .. }) => manager.to_string(),
                    | _ => self.command.name.clone(),
                };
                let mut arguments = match runtime {
                    //TODO: Add support for using "docker run" when container does not exist and image is known
                    | ContainerRuntime::Docker | ContainerRuntime::Podman => to_string(vec!["exec", name, &wrapped_command]),
                    | ContainerRuntime::Apptainer => to_string(vec!["exec", name, &wrapped_command]),
                    | ContainerRuntime::Unknown => {
                        error!(name = runtime.to_string(), "=> {} Container runtime", Label::not_found());
                        vec![]
                    }
                };
                arguments.extend(self_arguments);
                arguments
            }
            | _ => self_arguments,
        }
    }
    /// Create virtual environment from associated manifest file (ex. environment.yml or pyproject.toml)
    // TODO: Add support for Pixi and other virtual environment managers
    pub fn maybe_create_virtual_environment(parent: String, _manager: Option<VirtualEnvironmentManager>) -> Option<Script> {
        let current_dir = ModuleUri::from(&parent).working_directory(None);
        let manifest_path = PathBuf::from(&current_dir).join("environment.yml");
        match manifest_path.try_exists() {
            | Ok(true) => {
                info!(path = manifest_path.to_str().unwrap(), "=> {} Manifest file", Label::found());
                match get_conda_environment_name(manifest_path.clone()) {
                    | Some(name) => {
                        if get_conda_environment_list().contains(&name) {
                            info!(name, "=> {} Virtual environment", Label::found());
                            None
                        } else {
                            let mamba = Command::init().name("mamba".to_string()).build();
                            let name = (if mamba.test() { "mamba" } else { "conda" }).to_string();
                            let arguments = to_string(vec!["env", "create", "-f", "environment.yml"]);
                            let command = Command::init().name(name).build();
                            let script = Script::init()
                                .command(command)
                                .arguments(arguments)
                                .build()
                                .with_current_dir(current_dir.clone());
                            Some(script)
                        }
                    }
                    | None => None,
                }
            }
            | Ok(false) => {
                debug!(path = manifest_path.to_str().unwrap(), "=> {} Manifest file", Label::not_found());
                None
            }
            | Err(_) => {
                error!(
                    path = manifest_path.to_str().unwrap(),
                    "=> {} Error while trying to test existence",
                    Label::fail()
                );
                None
            }
        }
    }
    pub fn maybe_git_clone(url: String, current_dir: String) -> Option<Script> {
        let module_uri = ModuleUri::from(&url);
        let command = Command::init().name("git".to_string()).build();
        let arguments = to_string(vec!["clone", &url, &module_uri.hash()]);
        let script = Script::init().command(command).arguments(arguments).build().with_current_dir(current_dir);
        Some(script)
    }
    pub fn python_default() -> Script {
        let command = Command::init().name("python".to_string()).build();
        let arguments = to_string(vec![
            "-m",
            "lib",
            "--input_directory",
            "{{ INPUT_DIRECTORY }}",
            "--output_directory",
            "{{ OUTPUT_DIRECTORY }}",
        ]);
        Script::init().command(command).arguments(arguments).build()
    }
    /// Execute script in child process and wait for it to finish, print to stdout and ignore output
    /// TODO: This function should return `Option<Vec<String>>`
    pub fn run(&self) -> Vec<String> {
        let command = &self.get_command();
        let arguments = self.get_arguments();
        let current_dir = PathBuf::from(&self.current_dir);
        let envs = self.envs.clone().unwrap_or_default();
        warn!(cwd = current_dir.to_str().unwrap(), script = self.to_string(), "=> {}", Label::run());
        let mut output: Vec<String> = vec![];
        if let Ok(mut cmd) = process::Command::new(command)
            .args(arguments.iter())
            .current_dir(current_dir)
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .envs(envs)
            .spawn()
        {
            {
                // stdout
                let stdout = match cmd.stdout.as_mut() {
                    | Some(value) => value,
                    | None => {
                        error!(script = self.to_string(), "=> {}", Label::fail());
                        return vec![];
                    }
                };
                let reader = BufReader::new(stdout);
                let lines = reader.lines().map_while(Result::ok).collect::<Vec<String>>();
                if !lines.is_empty() {
                    output.extend(lines);
                }
                // stderr
                let stderr = match cmd.stderr.as_mut() {
                    | Some(value) => value,
                    | None => {
                        error!(script = self.to_string(), "=> {}", Label::fail());
                        return vec![];
                    }
                };
                let reader: BufReader<&mut process::ChildStderr> = BufReader::new(stderr);
                let lines = reader.lines().map_while(Result::ok).collect::<Vec<String>>();
                if !lines.is_empty() {
                    error!("=> {} {:#?}", Label::fail(), lines.join("\n").red());
                    output.extend(lines);
                }
            }
            match cmd.wait_with_output() {
                | Ok(result) => {
                    if result.status.success() {
                        let output = output;
                        trace!("=> {} {:#?}", Label::output(), result.dimmed());
                        trace!("=> {} {:#?}", Label::output(), output.dimmed());
                        output
                    } else {
                        error!(script = self.to_string(), "=> {}", Label::fail());
                        trace!("=> {} {:#?}", Label::output(), result);
                        vec![]
                    }
                }
                | Err(error) => {
                    error!(error = error.to_string(), script = self.to_string(), "=> {}", Label::fail());
                    vec![]
                }
            }
        } else {
            error!(script = self.to_string(), "=> {} Failed to spawn child process", Label::fail());
            vec![]
        }
    }
    pub fn with_container(mut self, container: ContainerEnvironment) -> Self {
        self.command.container.clone_from(&Some(container));
        self
    }
    /// Set current working directory for the script
    ///
    /// Provides builder interface and returns reference to self
    pub fn with_current_dir(mut self, path: String) -> Self {
        self.current_dir = path;
        self
    }
    /// Set script environment variables to be set during script.run call
    pub fn with_envs(mut self, values: HashMap<String, String>) -> Self {
        self.envs = Some(values.clone());
        self
    }
    /// Set virtual environment for the script
    ///
    /// Provides builder interface and returns reference to self
    ///
    /// # Examples
    ///
    /// For a given script,
    /// ```
    /// use pipe_lib::{to_string, Command, Script};
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = to_string(vec!["-m", "lib"]);
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// ```
    ///
    /// Run with no virtual environment
    /// ```
    /// use pipe_lib::{to_string, Command, Script};
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = to_string(vec!["-m", "lib"]);
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// let output = script.with_virtual_environment(None).run();
    /// ```
    ///
    /// Run using "module-a" Conda virtual environment (basically executes script with `conda run -n <name>`)
    /// ```
    /// use pipe_lib::{to_string, Command, Script, VirtualEnvironment, VirtualEnvironmentManager};
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = to_string(vec!["-m", "lib"]);
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// let virtual_environment = VirtualEnvironment::init()
    ///     .name("module-a".to_string())
    ///     .manager(VirtualEnvironmentManager::Conda)
    ///     .build();
    /// let output = script.with_virtual_environment(Some(virtual_environment)).run();
    /// ```
    pub fn with_virtual_environment(mut self, value: Option<VirtualEnvironment>) -> Self {
        self.command.virtual_environment.clone_from(&value);
        self
    }
}
impl ScriptTemplate {
    pub fn from_path(path: PathBuf) -> ScriptTemplate {
        let content = match read_file(path.clone()) {
            | Ok(value) => value,
            | Err(_) => {
                error!("Failed to read file: {:?}", path);
                "".to_owned()
            }
        };
        let json: serde_json::Result<ScriptTemplate> = serde_json::from_str(&content);

        json.expect("=> [ERROR] Failed to parse script template").clone()
    }
    pub fn resolve_python_virtual_environment(path: PathBuf) -> Option<VirtualEnvironment> {
        let default_template_path = path.join("template.json");
        let conda_manifest_path = PathBuf::from(&path).join("environment.yml");
        match conda_manifest_path.try_exists() {
            | Ok(true) => match get_conda_environment_name(conda_manifest_path.clone()) {
                | Some(value) => {
                    debug!(path = conda_manifest_path.to_str().unwrap(), "=> {} Manifest file", Label::using());
                    Some(VirtualEnvironment::init().name(value).manager(VirtualEnvironmentManager::Conda).build())
                }
                | None => match default_template_path.try_exists() {
                    | Ok(true) => ScriptTemplate::from_path(default_template_path).virtual_environment,
                    | _ => None,
                },
            },
            | _ => {
                // TODO: Check if other package manager is being used
                match default_template_path.try_exists() {
                    | Ok(true) => ScriptTemplate::from_path(default_template_path).virtual_environment,
                    | _ => None,
                }
            }
        }
    }
}
impl StringTemplate {
    const RE_HANDLEBARS: &'static str = r"\{\{(?P<variable>[A-Z_ ]*)\}\}";
    const RE_SPACES: &'static str = r"((?<=\{\{) *(?=[A-Z])|(?<=[A-Z]) *(?=\}\}))";

    pub fn new(value: &str) -> StringTemplate {
        Self {
            value: value.to_string(),
            template_type: TemplateType::Unknown,
        }
    }
    /// Replace string template with its associated value
    ///
    /// Non-terminal method that provides builder interface and returns reference to self
    pub fn expand(self, vars: Vec<(String, String)>) -> String {
        vars.into_iter()
            .fold(self.remove_spaces().value, |acc, (key, value)| acc.replace(&key, &value))
            .replace("{{", "")
            .replace("}}", "")
    }
    /// Get variable name from a template string value
    pub fn get_variable(self) -> String {
        match Regex::new(StringTemplate::RE_HANDLEBARS) {
            | Ok(re) => match re.captures(&self.value) {
                | Ok(Some(captures)) => captures["variable"].trim().to_string(),
                | _ => "".to_string(),
            },
            | _ => "".to_string(),
        }
    }
    /// Check if a string is a handlebars template
    ///
    /// Template looks like `"{{ VARIABLE_NAME }}"`
    ///
    /// - Template must start with "{{" and end with "}}"
    /// - Template variable name must be in upper snake case format
    /// - Template variable name must not contain any spaces
    /// - Template can have any number of spaces at before and after the variable name
    ///
    ///
    /// # Examples
    /// ```
    /// use pipe_lib::StringTemplate;
    ///
    /// // OK
    /// let template = StringTemplate::new("{{ INPUT_DIRECTORY }}");
    /// assert!(template.is_handlebars());
    /// // NG
    /// let template = StringTemplate::new("{{ MustBeUpperSnakeCase }}");
    /// assert!(!template.is_handlebars());
    /// ```
    pub fn is_handlebars(self) -> bool {
        match Regex::new(StringTemplate::RE_HANDLEBARS) {
            | Ok(re) => match re.is_match(&self.value) {
                | Ok(value) => value,
                | _ => false,
            },
            | _ => false,
        }
    }
    /// Remove spaces around variable name within a handlebars template
    ///
    /// Non-terminal method that provides builder interface and returns reference to self
    ///
    /// # Parameters
    /// - `value` - string that contains 0 or more handlebars templates
    ///
    /// # Examples
    /// ```
    /// use pipe_lib::StringTemplate;
    ///
    /// let expected = "{{VARIABLE}}";
    ///
    /// let template = StringTemplate::new("{{ VARIABLE }}");
    /// assert_eq!(template.remove_spaces().value, expected);
    ///
    /// let template = StringTemplate::new("{{ VARIABLE   }}");
    /// assert_eq!(template.remove_spaces().value, expected);
    ///
    /// let template = StringTemplate::new("{{VARIABLE}}");
    /// assert_eq!(template.remove_spaces().value, expected);
    ///
    /// // Must be UPPER_SNAKE_CASE
    /// let template = StringTemplate::new("{{ variable }}");
    /// assert_eq!(template.remove_spaces().value, "{{ variable }}");
    /// ```
    pub fn remove_spaces(mut self) -> Self {
        let updated = match Regex::new(StringTemplate::RE_SPACES) {
            | Ok(re) => re.replace_all(&self.value, "").to_string(),
            | _ => self.value,
        };
        self.value = updated;
        self
    }
}
/// Add forward slash to end of folder path
pub fn add_dot_slash(path: PathBuf) -> PathBuf {
    if path.starts_with("./") {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("./{}", path.display()))
    }
}
/// Add forward slash to end of folder path
pub fn add_forward_slash(path: PathBuf) -> PathBuf {
    if path.is_dir() && !path.ends_with("/") {
        PathBuf::from(format!("{}/", path.display()))
    } else {
        path.to_path_buf()
    }
}
pub fn download_binary(url: String, destination: PathBuf) -> Result<PathBuf, String> {
    async fn download(url: String, destination: PathBuf) -> Result<(), String> {
        let client = reqwest::Client::new();
        let response = client.get(url.clone()).send();
        let filename = PathBuf::from(url.clone()).file_name().unwrap().to_str().unwrap().to_string();
        match response.await {
            | Ok(data) => match data.bytes().await {
                | Ok(content) => {
                    let mut output = File::create(destination.join(filename.clone())).unwrap();
                    let _ = copy(&mut Cursor::new(content.clone()), &mut output);
                    debug!(filename = filename, "=> {} Downloaded", Label::output());
                    Ok(())
                }
                | Err(_) => Err(format!("No content downloaded from {url}")),
            },
            | Err(_) => Err(format!("Failed to download {url}")),
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _ = runtime.block_on(download(url.clone(), destination.clone()));
    let filename = PathBuf::from(url.clone()).file_name().unwrap().to_str().unwrap().to_string();
    Ok(destination.join(filename))
}
/// Get SHA256 hash of a file
pub fn get_checksum(path: PathBuf) -> String {
    if let Ok(content) = read_file(path) {
        let hash = Sha256::digest(content);
        format!("{:x}", hash)
    } else {
        unreachable!()
    }
}
/// Get list of directory children
///
/// Only first level children are returned
///
pub fn get_children(path: String) -> Vec<String> {
    match fs::read_dir(path) {
        | Ok(value) => value.flatten().map(|x| path_to_string(x.path())).collect::<Vec<_>>(),
        | Err(_) => Vec::new(),
    }
}
/// Get list of Conda virtual environments
pub fn get_conda_environment_list() -> Vec<String> {
    let command = Command::init().name("conda".to_string()).build();
    let arguments = to_string(vec!["env", "list"]);
    let script = Script::init().command(command.clone()).arguments(arguments).build();
    if command.test() {
        script
            .run()
            .join("\n")
            .replace(['#', '*'], "")
            .split('\n')
            .filter(|x| !x.is_empty())
            .skip(2)
            .map(|x| x.trim().chars().take_while(|x| !x.is_whitespace()).collect())
            .collect::<Vec<String>>()
    } else {
        vec![]
    }
}
/// Get name of Conda virtual environment from manifest file
///
/// Not super sophisticated. "name" needs to be first line in environment.yml
///
/// Example:
///
/// `environment.yml`
/// ```yaml
/// name: module-a
/// channels:
///   - conda-forge
///   - defaults
/// dependencies:
///   - pip
///   - numpy
/// ```
///
/// Get the environment name
/// ```
/// use std::path::PathBuf;
/// use pipe_lib::get_conda_environment_name;
///
/// get_conda_environment_name(PathBuf::from("/path/to/environment.yml"));
/// ```
pub fn get_conda_environment_name(path: PathBuf) -> Option<String> {
    match path.try_exists() {
        | Ok(true) => match read_file(path) {
            | Ok(content) => {
                let name = content
                    .split('\n')
                    .collect::<Vec<_>>()
                    .clone()
                    .into_iter()
                    .next()?
                    .split("name: ")
                    .collect::<Vec<_>>()
                    .clone()
                    .into_iter()
                    .nth(1)?
                    .to_string();
                Some(name)
            }
            | _ => None,
        },
        | _ => None,
    }
}
/// Get file extension
///
/// # Examples
/// ```
/// use std::path::Path;
/// use pipe_lib::get_extension;
///
/// assert_eq!("txt", get_extension(Path::new("hello.txt")));
/// assert_eq!("md", get_extension(Path::new("README.md")));
/// assert_eq!("", get_extension(Path::new(".dotfile")));
/// assert_eq!("", get_extension(Path::new("/path/to/folder")));
/// ```
pub fn get_extension(path: &Path) -> String {
    path.extension().unwrap_or_default().to_str().unwrap_or_default().to_string()
}
/// Get parent of a file/folder path
pub fn get_parent(path: PathBuf) -> PathBuf {
    PathBuf::from(path.parent().unwrap())
}
/// Check if a file has a given file extension
///
/// # Examples
/// ```
/// use std::path::Path;
/// use pipe_lib::has_extension;
///
/// assert!(has_extension("txt", Path::new("hello.txt")));
/// assert!(!has_extension("txt", Path::new("hello.md")));
/// ```
pub fn has_extension(extension: &str, path: &Path) -> bool {
    extension == get_extension(path)
}
/// Check if stdin is piped (e.g., `echo "hello world" | pipe`)
pub fn is_stdin_piped() -> StdinIsPipedResult {
    if !io::stdin().is_terminal() {
        StdinIsPipedResult::StdinIsPiped
    } else {
        StdinIsPipedResult::StdinIsNotPiped
    }
}
/// Check if stdout is piped (e.g., `pipe | process`)
pub fn is_stdout_piped() -> StdoutIsPipedResult {
    if !io::stdout().is_terminal() {
        StdoutIsPipedResult::StdoutIsPiped
    } else {
        StdoutIsPipedResult::StdoutIsNotPiped
    }
}
#[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
fn make_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.is_executable()
}
#[cfg(windows)]
fn make_executable(path: &PathBuf) -> bool {
    // TODO: Add windows support...pass through?
    path.is_executable()
}
fn path_to_string(path: PathBuf) -> String {
    // NOTE: fs::canonicalize might cause problems on Windows
    let result = match fs::canonicalize(path.as_path()) {
        | Ok(value) => value,
        | Err(_) => path,
    };
    result.to_str().unwrap().to_string()
}
/// Read content of a file to a string
///
/// Fails when the file cannot be found or read
///
/// # Examples
/// ```
/// use std::path::PathBuf;
/// use pipe_lib::read_file;
///
/// let path = PathBuf::from("./tests/fixtures/template.json");
/// let content = read_file(path).unwrap();
pub fn read_file(path: PathBuf) -> Result<String, Box<dyn Error>> {
    let mut content = String::new();
    let _ = match File::open(path.clone()) {
        | Ok(mut file) => {
            debug!(path = path.to_str().unwrap(), "=> {}", Label::read());
            file.read_to_string(&mut content)
        }
        | Err(err) => {
            error!(path = path.to_str().unwrap(), "=> {} Cannot read file", Label::fail());
            Err(err)
        }
    };
    Ok(content)
}
pub fn to_string(values: Vec<&str>) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
pub fn to_snake_case(value: String) -> String {
    value.replace(" ", "_").to_lowercase()
}

#[cfg(test)]
mod tests;
