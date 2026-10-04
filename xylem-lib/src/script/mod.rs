use crate::completion::Completion;
use crate::error::{ModuleLoadError, ProcessError, TemplateError};
use crate::util::absolute_path;
use crate::util::constants::{RE_HANDLEBARS, RE_TEMPLATE_SPACES};
use acorn_cmd::{args, CommandSpec};
use acorn_core::{Location, Scheme};
use acorn_host::fs::read_text;
use acorn_host::terminal::Label;
use bon::Builder;
use clap::ValueEnum;
use core::convert::TryFrom;
use derive_more::{Display, FromStr};
use fancy_regex::Regex;
use owo_colors::OwoColorize;
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::env::{set_var, vars};
use std::ffi::{OsStr, OsString};
use std::fs::read_to_string;
use std::path::{is_separator, Path, PathBuf};
use strum::EnumIs;
use tracing::{debug, error, info, trace, warn};
use uriparse::URI;
use which::which;

/// Cleanup policy for a container created by Xylem.
#[derive(Clone, Copy, Debug, Default, Deserialize, Display, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ContainerCleanup {
    /// Remove the container after every run outcome.
    #[default]
    #[display("always")]
    Always,
    /// Remove the container only after a successful run.
    #[display("on_success")]
    #[value(name = "on-success")]
    OnSuccess,
    /// Retain the container after the run.
    #[display("never")]
    Never,
}
#[derive(Clone, Copy, Debug, Display, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
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
/// Startup strategy for a container created by Xylem.
#[derive(Clone, Copy, Debug, Deserialize, Display, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStartup {
    /// Run the configured command without a shell.
    #[display("explicit_command")]
    ExplicitCommand,
    /// Preserve the image's configured entrypoint and command.
    #[display("image_default")]
    ImageDefault,
    /// Run the documented idle command.
    #[display("keep_alive")]
    KeepAlive,
}
/// Values that from Serde that can be set as environment variables
#[derive(Clone, Debug, Display)]
pub enum EnvironmentValue {
    String(String),
    Number(Number),
    Bool(bool),
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize, Display, EnumIs)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModuleType {
    /// Compiled "black boxes" that are used via a command line interface
    #[serde(alias = "bin")]
    Binary { checksum: Option<String>, uri: Location },
    /// Nested workflow module that allows for grouping modules
    #[serde(alias = "config")]
    Configuration { uri: Location },
    /// Provides support for executing scripts
    Script {
        #[serde(alias = "language", alias = "programmingLanguage")]
        programming_language: Option<ModuleLanguage>,
        uri: Location,
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
    Url(Box<URI<'a>>),
    Unknown(String),
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawModuleType {
    #[serde(alias = "bin")]
    Binary,
    #[serde(alias = "config")]
    Configuration,
    Script,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TemplateAttribute {
    Path(String),
    Template(Box<ScriptTemplate>),
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
    /// [Mamba package manager](https://mamba.readthedocs.io/)
    #[display("mamba")]
    #[serde(alias = "mamba")]
    Mamba,
    /// Simple Python version management (<https://github.com/pyenv/pyenv>)
    #[display("pyenv")]
    #[serde(alias = "pyenv")]
    Pyenv,
    /// Tool to create isolated Python environments (<https://virtualenv.pypa.io/en/latest/>)
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
#[derive(Builder, Clone, Debug, Display)]
#[display("{}", name)]
#[builder(start_fn(name = init))]
pub struct Command {
    /// Command name (ex. "echo")
    #[builder(default)]
    pub name: String,
    /// Virtual environment struct that contains details about the associated virtual environment
    pub virtual_environment: Option<VirtualEnvironment>,
    /// Container struct that contains details about the associated containerized environment
    pub container: Option<ContainerEnvironment>,
}
/// Struct for using and sharing containerized environments
///
/// Examples:
/// ```ignore
/// use xylem_lib::script::ContainerEnvironment;
///
/// let container = ContainerEnvironment::init().name("myenv").build();
/// let container = ContainerEnvironment::init().name("myenv").runtime(ContainerRuntime::Podman).build();
/// ```
#[derive(Builder, Clone, Debug, Deserialize, Display, Eq, PartialEq, Serialize)]
#[display("{}", "{runtime}:{image}")]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct ContainerEnvironment {
    /// Name of container
    pub name: String,
    /// Runtime (ex. "docker", "podman", etc...)
    #[builder(default=ContainerRuntime::Docker)]
    #[serde(default = "default_container_runtime")]
    pub runtime: ContainerRuntime,
    /// Context name to use (defaults to active context)
    pub context: Option<String>,
    /// Name of image container was built from
    pub image: Option<String>,
    /// Whether Xylem creates and owns the container for this run.
    #[builder(default)]
    #[serde(default)]
    pub create: bool,
    /// Optional startup command for a managed container.
    pub command: Option<Vec<String>>,
    /// Whether to run the documented idle command for a managed container.
    #[builder(default)]
    #[serde(default)]
    pub keep_alive: bool,
    /// Cleanup policy for a managed container.
    pub cleanup: Option<ContainerCleanup>,
    /// Explicit bind mounts available to a managed container.
    #[builder(default)]
    #[serde(default)]
    pub mounts: Vec<ContainerMount>,
    /// Maximum number of seconds to wait for readiness.
    pub startup_timeout_seconds: Option<u64>,
}
/// Explicit bind mount for a managed container.
#[derive(Builder, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct ContainerMount {
    /// Host path, resolved relative to the declaring file.
    pub source: String,
    /// Absolute path exposed inside the container.
    pub target: String,
    /// Whether the guest mount is read-only.
    #[builder(default = true)]
    #[serde(default = "default_true")]
    pub read_only: bool,
}
#[derive(Clone, Debug, Display, Serialize)]
#[display("{name}")]
pub struct Module {
    pub name: String,
    /// Stable ancestry from the root workflow to this executable leaf.
    #[serde(skip)]
    pub module_path: Vec<String>,
    #[serde(flatten)]
    pub module_type: ModuleType,
    pub description: Option<String>,
    pub variables: Option<Map<String, Value>>,
    pub before_script: Option<Instruction>,
    pub after_script: Option<Instruction>,
    pub template: Option<TemplateAttribute>,
    pub completion: Option<Completion>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModule {
    after_script: Option<Instruction>,
    before_script: Option<Instruction>,
    checksum: Option<String>,
    completion: Option<Completion>,
    description: Option<String>,
    name: String,
    #[serde(alias = "language", alias = "programmingLanguage")]
    programming_language: Option<ModuleLanguage>,
    template: Option<TemplateAttribute>,
    #[serde(rename = "type")]
    module_type: RawModuleType,
    uri: Location,
    variables: Option<Map<String, Value>>,
}
/// Script struct
///
/// Uses [bon](https://elastio.github.io/bon/) crate to implement builder pattern.
///
/// bon provides intuitive default values and allows for streamlined API.
///
/// Example:
/// ```
/// use xylem_lib::script::*;
///
/// let command = Command::init().name("echo".to_string()).build();
/// let arguments = vec!["hello".to_string()];
/// let script = Script::init().command(command).arguments(arguments).build();
/// ```
#[derive(Builder, Clone, Debug, Display)]
#[display("{} {}", self.get_command(), self.get_arguments().join(" "))]
#[builder(start_fn(name = init))]
pub struct Script {
    #[builder(default = Command::init().build())]
    pub command: Command,
    #[builder(
        default = Vec::<String>::new(),
        with = |arguments: impl IntoIterator<Item = impl AsRef<OsStr>>| arguments
            .into_iter()
            .map(|argument| argument.as_ref().to_string_lossy().into_owned())
            .collect()
    )]
    pub arguments: Vec<String>,
    /// Directory in which the script should be executed when run method is invoked
    #[builder(default = ".".to_string())]
    pub current_dir: String,
    pub envs: Option<HashMap<String, String>>,
    #[builder(default = true)]
    pub forward_container_env: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptTemplate {
    pub command: String,
    pub container: Option<ContainerEnvironment>,
    #[serde(alias = "environment")]
    pub virtual_environment: Option<VirtualEnvironment>,
    pub arguments: Instruction,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StringTemplate {
    pub value: String,
    pub template_type: TemplateType,
}
#[derive(Builder, Clone, Debug, Deserialize, Display, PartialEq, Serialize)]
#[display("{name}")]
#[builder(start_fn(name = init))]
#[serde(deny_unknown_fields)]
pub struct VirtualEnvironment {
    pub name: String,
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
    /// use xylem_lib::Command;
    ///
    /// let command = Command::init().name("ls".to_string()).build();
    /// assert!(command.test());
    /// ```
    pub fn test(self) -> bool {
        match which(&self.name) {
            | Ok(value) => {
                let path = value.display().to_string();
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
impl ContainerEnvironment {
    pub(crate) fn resolved_startup(&self) -> (ContainerStartup, Vec<OsString>) {
        match (self.keep_alive, &self.command) {
            | (true, _) => (ContainerStartup::KeepAlive, args!["sleep", "infinity"]),
            | (false, Some(command)) => (ContainerStartup::ExplicitCommand, args![..command.clone()]),
            | (false, None) => (ContainerStartup::ImageDefault, args![]),
        }
    }
}
impl EnvironmentValue {
    pub const PREFIX: &'static str = "xylem";
    /// Resolve `{{ VARIABLE }}` references in namespaced environment values.
    ///
    /// Values may reference other values in the same map.
    /// Resolution is repeated until the map stabilizes or every value has had a chance to resolve a dependency chain.
    /// Unknown and cyclic references are left intact.
    pub fn expand(values: HashMap<String, String>) -> HashMap<String, String> {
        let raw = values
            .into_iter()
            .map(|(key, value)| (EnvironmentValue::trim_prefix(key), value))
            .collect::<HashMap<_, _>>();
        let mut expanded = raw.clone();
        for _ in 0..=raw.len() {
            let variables = expanded.clone().into_iter().collect::<Vec<_>>();
            let next = raw
                .iter()
                .map(|(key, value)| (key.clone(), StringTemplate::new(value).expand(variables.clone())))
                .collect::<HashMap<_, _>>();
            if next == expanded {
                break;
            }
            expanded = next;
        }
        expanded
            .into_iter()
            .map(|(key, value)| (EnvironmentValue::prepend_prefix(key), value))
            .collect()
    }
    fn formatted_prefix() -> String {
        format!("_{}_", EnvironmentValue::PREFIX.to_uppercase())
    }
    /// Convert serde Value to EnvironmentValue
    ///
    pub fn from_value(value: Value) -> EnvironmentValue {
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
                | value => value.to_string(),
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
    /// Set values that already use Xylem's `_XYLEM_` namespace.
    pub fn set_namespaced_variables(values: HashMap<String, String>) {
        values.into_iter().for_each(|(key, value)| set_var(key, value));
    }
}
impl Module {
    /// Resolve the module's explicitly configured or auto-detected virtual environment.
    pub fn resolve_virtual_environment(&self, working_dir: &str) -> Result<Option<VirtualEnvironment>, TemplateError> {
        match &self.template {
            | Some(TemplateAttribute::Path(value)) => {
                let template_path = PathBuf::from(working_dir).join(value);
                match template_path.is_file() {
                    | true => ScriptTemplate::from_path(template_path).map(|template| template.virtual_environment),
                    | false => Ok(None),
                }
            }
            | Some(TemplateAttribute::Template(value)) => Ok(value.virtual_environment.clone()),
            | None => ScriptTemplate::resolve_python_virtual_environment(PathBuf::from(working_dir)),
        }
    }
}
impl<'de> Deserialize<'de> for Module {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        RawModule::deserialize(deserializer).and_then(
            |RawModule {
                 after_script,
                 before_script,
                 checksum,
                 completion,
                 description,
                 name,
                 programming_language,
                 template,
                 module_type,
                 uri,
                 variables,
             }| {
                match (module_type, checksum, programming_language) {
                    | (RawModuleType::Binary, checksum, None) => Ok(ModuleType::Binary { checksum, uri }),
                    | (RawModuleType::Binary, _, Some(_)) => Err(D::Error::custom("binary modules do not accept programmingLanguage")),
                    | (RawModuleType::Configuration, None, None) => Ok(ModuleType::Configuration { uri }),
                    | (RawModuleType::Configuration, Some(_), _) => Err(D::Error::custom("configuration modules do not accept checksum")),
                    | (RawModuleType::Configuration, _, Some(_)) => Err(D::Error::custom("configuration modules do not accept programmingLanguage")),
                    | (RawModuleType::Script, None, programming_language) => Ok(ModuleType::Script { programming_language, uri }),
                    | (RawModuleType::Script, Some(_), _) => Err(D::Error::custom("script modules do not accept checksum")),
                }
                .map(|module_type| Self {
                    after_script,
                    before_script,
                    completion,
                    description,
                    module_type,
                    module_path: vec![name.clone()],
                    name,
                    template,
                    variables,
                })
            },
        )
    }
}
impl ModuleLanguage {
    pub const fn is_supported(&self) -> bool {
        self.is_generic() || self.is_python()
    }
}
impl ModuleUri<'_> {
    pub fn absolutize(self, source: &Path) -> Option<Location> {
        match self {
            | Self::File(path) if !path.is_absolute() => source
                .parent()
                .map(|parent| parent.join(path).components().collect::<PathBuf>())
                .map(|path| Location::from(format!("file:{}", path.display()).as_str())),
            | Self::File(_) | Self::Unknown(_) | Self::Url(_) => None,
        }
    }
    pub fn from(value: &Location) -> ModuleUri<'_> {
        let value = <&str>::from(value);
        match URI::try_from(value) {
            | Ok(uri) => match (Scheme::from(uri.scheme().as_str()), uri.scheme().as_str().eq_ignore_ascii_case("ssh")) {
                | (Scheme::HTTP | Scheme::HTTPS | Scheme::SSH, _) | (Scheme::Unsupported, true) => ModuleUri::Url(Box::new(uri)),
                | (Scheme::File, _) => ModuleUri::File(PathBuf::from(uri.path().to_string())),
                | (Scheme::Unsupported, false) => ModuleUri::Unknown(value.to_string()),
            },
            | Err(_) => ModuleUri::Unknown(value.to_string()),
        }
    }
    pub fn resolve_configuration(value: &Location, source: &Path) -> Option<PathBuf> {
        match ModuleUri::from(value) {
            | ModuleUri::File(path) if path.is_absolute() => Some(path),
            | ModuleUri::File(path) => source.parent().map(|parent| parent.join(path)),
            | ModuleUri::Unknown(_) | ModuleUri::Url(_) => None,
        }
    }
}
impl Script {
    pub fn ensure_relative(mut self) -> Script {
        let path = PathBuf::from(self.command.name);
        self.command.name = add_dot_slash(path).to_string_lossy().into_owned();
        self
    }
    /// Expand environment variables in the script arguments from environment
    ///
    /// This method does not set environment variables, but only interpolates the values within the script arguments
    ///
    /// ### Example
    /// ```
    /// use xylem_lib::script::*;
    ///
    /// // Environment: ANSWER=42
    /// std::env::set_var("_XYLEM_ANSWER", "42");
    /// let command = Command::init().name("echo".to_string()).build();
    /// let arguments = vec!["{{ ANSWER }}".to_string()];
    /// let script = Script::init().command(command).arguments(arguments).build().expand_arguments();
    /// assert_eq!(script.to_string(), "echo 42");
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
    /// use xylem_lib::script::*;
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
    pub fn from_module(module: Module, root: Option<PathBuf>) -> Result<Script, ModuleLoadError> {
        const DEFAULT_FILENAME: &str = "template.json";
        let module_uri = match &module.module_type {
            | ModuleType::Binary { uri, .. } | ModuleType::Script { uri, .. } => Ok(uri),
            | ModuleType::Configuration { .. } => Err(ModuleLoadError::InvalidType { module: module.name.clone() }),
        };
        module_uri.and_then(|module_uri| {
            module_working_directory(module_uri.into(), root.as_ref())
                .ok_or_else(|| ModuleLoadError::InvalidUri {
                    module: module.name.clone(),
                    uri: module_uri.uri().unwrap_or_else(|| <&str>::from(module_uri).to_string()),
                })
                .and_then(|working_dir| {
                    let default_template_path = PathBuf::from(&working_dir).join(DEFAULT_FILENAME);
                    match &module.template {
                        | Some(attribute) => match attribute {
                            | TemplateAttribute::Path(value) => match PathBuf::from(&working_dir).join(value).try_exists() {
                                | Ok(true) => {
                                    let path = PathBuf::from(&working_dir).join(value);
                                    debug!(module = module.name, path = %path.display(), "=> {} Template file", Label::using());
                                    Script::from_path(path).map_err(|source| ModuleLoadError::Template {
                                        module: module.name.clone(),
                                        source,
                                    })
                                }
                                | Ok(false) => {
                                    let path = PathBuf::from(&working_dir).join(value);
                                    warn!(module = module.name, "=> {} {}", Label::not_found(), path.display());
                                    warn!(module = module.name, "=> {} Default template file path", Label::using());
                                    Script::from_path(default_template_path).map_err(|source| ModuleLoadError::Template {
                                        module: module.name.clone(),
                                        source,
                                    })
                                }
                                | Err(source) => Err(ModuleLoadError::Inspect {
                                    module: module.name.clone(),
                                    path: PathBuf::from(&working_dir).join(value),
                                    source,
                                }),
                            },
                            | TemplateAttribute::Template(value) => {
                                debug!(module = module.name, "=> {} Template attribute", Label::using());
                                Ok(Script::from_template(value.as_ref().clone()))
                            }
                        },
                        | None => match default_template_path.try_exists() {
                            | Ok(true) => {
                                warn!(module = module.name, "=> {} Default template file path", Label::using());
                                Script::from_path(default_template_path).map_err(|source| ModuleLoadError::Template {
                                    module: module.name.clone(),
                                    source,
                                })
                            }
                            | Ok(false) => {
                                warn!(module = module.name, "=> {} {}", Label::not_found(), default_template_path.display());
                                warn!(module = module.name, "=> {} Default Python script", Label::using());
                                Ok(Script::python_default())
                            }
                            | Err(source) => Err(ModuleLoadError::Inspect {
                                module: module.name.clone(),
                                path: default_template_path,
                                source,
                            }),
                        },
                    }
                })
        })
    }
    /// Create a script from a template JSON file in [ScriptTemplate] format.
    ///
    /// You can easily import the script with
    /// ```
    /// use xylem_lib::script::*;
    /// use std::path::PathBuf;
    ///
    /// let path = PathBuf::from("../tests/fixtures/template.json");
    /// let script = Script::from_path(path).expect("valid template");
    /// assert_eq!(script.command.name, "python");
    /// ```
    pub fn from_path(path: PathBuf) -> Result<Script, TemplateError> {
        ScriptTemplate::from_path(path).map(Script::from_template)
    }
    /// Create a script from a ScriptTemplate struct.
    ///
    /// You can easily import the script with
    /// ```
    /// use xylem_lib::script::*;
    /// let template = ScriptTemplate {
    ///     command: "echo".to_string(),
    ///     container: None,
    ///     virtual_environment: None,
    ///     arguments: Instruction::ExecFormat(vec!["hello".to_string()]),
    /// };
    /// let script = Script::from_template(template);
    /// assert_eq!(script.to_string(), "echo hello");
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
                    | VirtualEnvironmentManager::Conda => args!["run", "--name", name, "--no-capture-output", self.command.name.clone()],
                    | VirtualEnvironmentManager::Mamba => args!["run", "--name", name, self.command.name.clone()],
                    | VirtualEnvironmentManager::Pixi => args!["run", "--environment", name, self.command.name.clone()],
                    | VirtualEnvironmentManager::Poetry => args!["run", self.command.name.clone()],
                    | _ => {
                        error!(name = manager.to_string(), "=> {} Virtual environment manager", Label::not_found());
                        vec![]
                    }
                }
                .into_iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
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
                let arguments = match runtime {
                    | ContainerRuntime::Docker | ContainerRuntime::Podman => {
                        let environment = match self.forward_container_env {
                            | true => {
                                let mut keys = self.envs.clone().unwrap_or_default().into_keys().collect::<Vec<_>>();
                                keys.sort();
                                keys.into_iter().flat_map(|key| args!["--env", key]).collect::<Vec<_>>()
                            }
                            | false => args![],
                        };
                        args!["exec", ..environment, name, wrapped_command, ..self_arguments]
                    }
                    | ContainerRuntime::Apptainer => args![
                        "exec",
                        ..(!self.forward_container_env).then_some("--cleanenv"),
                        name,
                        wrapped_command,
                        ..self_arguments
                    ],
                    | ContainerRuntime::Unknown => {
                        error!(name = runtime.to_string(), "=> {} Container runtime", Label::not_found());
                        args![]
                    }
                };
                arguments.into_iter().map(|argument| argument.to_string_lossy().into_owned()).collect()
            }
            | _ => self_arguments,
        }
    }
    /// Create a virtual environment from an associated Conda or Pixi manifest.
    pub fn maybe_create_virtual_environment(parent: String, environment: Option<VirtualEnvironment>) -> Result<Option<Script>, TemplateError> {
        let current_dir = absolute_path(PathBuf::from(parent));
        environment
            .map_or_else(
                || ScriptTemplate::resolve_python_virtual_environment(PathBuf::from(&current_dir)),
                |environment| Ok(Some(environment)),
            )
            .map(|environment| match environment {
                | Some(value) if value.manager == VirtualEnvironmentManager::Pixi => create_pixi_environment_script(current_dir, value),
                | Some(value) if value.manager == VirtualEnvironmentManager::Conda => create_conda_environment_script(current_dir),
                | Some(value) if value.manager == VirtualEnvironmentManager::Mamba => create_mamba_environment_script(current_dir),
                | Some(value) => {
                    let manager = value.manager;
                    debug!(
                        manager = manager.to_string(),
                        "=> {} Unsupported environment creation manager",
                        Label::not_found()
                    );
                    None
                }
                | None => None,
            })
    }
    pub fn maybe_git_clone(url: String, current_dir: String) -> Option<Script> {
        let location = Location::from(&url);
        let command = Command::init().name("git".to_string()).build();
        let arguments = vec![
            "-c".to_string(),
            "http.followRedirects=false".to_string(),
            "clone".to_string(),
            url,
            location.hash(),
        ];
        let script = Script::init().command(command).arguments(arguments).build().with_current_dir(current_dir);
        Some(script)
    }
    pub fn python_default() -> Script {
        let command = Command::init().name("python".to_string()).build();
        let arguments = vec![
            "-m".to_string(),
            "lib".to_string(),
            "--input_directory".to_string(),
            "{{ INPUT_DIRECTORY }}".to_string(),
            "--output_directory".to_string(),
            "{{ OUTPUT_DIRECTORY }}".to_string(),
        ];
        Script::init().command(command).arguments(arguments).build()
    }
    /// Execute a script and return its combined standard output and standard error.
    ///
    /// Execute a script and return failures to the caller.
    pub fn run(&self) -> Result<Vec<String>, ProcessError> {
        self.run_checked()
    }
    /// Execute a script and preserve command failures for the caller.
    pub fn run_checked(&self) -> Result<Vec<String>, ProcessError> {
        let command = &self.get_command();
        let arguments = self.get_arguments();
        let current_dir = PathBuf::from(&self.current_dir);
        let envs = self.envs.clone().unwrap_or_default();
        warn!(cwd = %current_dir.display(), script = self.to_string(), "=> {}", Label::run());
        let envs = match (&self.command.container, self.forward_container_env) {
            | (Some(_), false) => HashMap::new(),
            | _ => envs,
        };
        let specification = envs.into_iter().fold(
            CommandSpec::new(command).args(arguments).current_dir(&current_dir),
            |specification, (key, value)| specification.env(key, value),
        );
        match specification.output() {
            | Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout).lines().map(String::from).collect::<Vec<_>>();
                let stderr = String::from_utf8_lossy(&result.stderr).lines().map(String::from).collect::<Vec<_>>();
                let mut output = stdout;
                output.extend(stderr.clone());
                match result.status.success() {
                    | true => {
                        trace!("=> {} {:#?}", Label::output(), result.dimmed());
                        trace!("=> {} {:#?}", Label::output(), output.dimmed());
                        Ok(output)
                    }
                    | false => {
                        let details = stderr.join("\n");
                        error!(script = self.to_string(), status = %result.status, details, "=> {}", Label::fail());
                        Err(ProcessError::Unsuccessful {
                            script: self.to_string(),
                            working_directory: current_dir,
                            status: result.status.to_string(),
                            stderr: details,
                        })
                    }
                }
            }
            | Err(error) => {
                error!(
                    error = error.to_string(),
                    script = self.to_string(),
                    "=> {} Failed to run child process",
                    Label::fail()
                );
                Err(ProcessError::Spawn {
                    script: self.to_string(),
                    working_directory: current_dir,
                    source: error,
                })
            }
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
        self.envs = Some(values);
        self
    }
    /// Configure whether resolved Xylem variables are forwarded to a container command.
    pub fn with_container_env(mut self, forward: bool) -> Self {
        self.forward_container_env = forward;
        self
    }
    /// Resolve the effective run-scoped name for a managed container.
    pub fn with_container_run(mut self, run_id: Option<&str>) -> Self {
        if let (Some(container), Some(run_id)) = (&mut self.command.container, run_id) {
            if container.create {
                container.name = managed_container_name(&container.name, run_id);
            }
        }
        self
    }
    /// Set virtual environment for the script
    ///
    /// Provides builder interface and returns reference to self
    ///
    /// ### Examples
    /// For a given script,
    /// ```
    /// use xylem_lib::script::*;
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = vec!["-m".to_string(), "lib".to_string()];
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// ```
    ///
    /// Run with no virtual environment
    /// ```no_run
    /// use xylem_lib::script::*;
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = vec!["-m".to_string(), "lib".to_string()];
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// let output = script.with_virtual_environment(None).run();
    /// ```
    ///
    /// Run using "module-a" Conda virtual environment (basically executes script with `conda run -n <name>`)
    /// ```no_run
    /// use xylem_lib::script::*;
    ///
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = vec!["-m".to_string(), "lib".to_string()];
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
    pub fn from_path(path: PathBuf) -> Result<ScriptTemplate, TemplateError> {
        read_to_string(&path)
            .map_err(|source| TemplateError::Read { path: path.clone(), source })
            .and_then(|content| {
                serde_json::from_str::<ScriptTemplate>(&content)
                    .map_err(|source| TemplateError::Parse { path: path.clone(), source })
                    .map(|mut template| {
                        template.absolutize_container_mounts(&path);
                        template
                    })
            })
    }
    pub(crate) fn absolutize_container_mounts(&mut self, source: &Path) {
        if let (Some(parent), Some(container)) = (source.parent(), &mut self.container) {
            container.mounts.iter_mut().for_each(|mount| {
                let path = PathBuf::from(&mount.source);
                if !path.is_absolute() {
                    mount.source = parent.join(path).components().collect::<PathBuf>().display().to_string();
                }
            });
        }
    }
    pub fn resolve_python_virtual_environment(path: PathBuf) -> Result<Option<VirtualEnvironment>, TemplateError> {
        let default_template_path = path.join("template.json");
        let conda_manifest_path = PathBuf::from(&path).join("environment.yml");
        let configured = match default_template_path.is_file() {
            | true => ScriptTemplate::from_path(default_template_path).map(|template| template.virtual_environment),
            | false => Ok(None),
        };
        configured.map(|configured| match configured {
            | Some(environment) => Some(environment),
            | None => match find_pixi_manifest(path) {
                | Some(manifest_path) => {
                    debug!(path = %manifest_path.display(), "=> {} Pixi manifest file", Label::using());
                    Some(
                        VirtualEnvironment::init()
                            .name("default".to_string())
                            .manager(VirtualEnvironmentManager::Pixi)
                            .build(),
                    )
                }
                | None => get_conda_environment_name(conda_manifest_path.clone()).map(|value| {
                    debug!(path = %conda_manifest_path.display(), "=> {} Conda manifest file", Label::using());
                    VirtualEnvironment::init().name(value).manager(VirtualEnvironmentManager::Conda).build()
                }),
            },
        })
    }
}
impl StringTemplate {
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
        vars.into_iter().fold(self.remove_spaces().value, |acc, (key, value)| {
            acc.replace(&format!("{{{{{key}}}}}"), &value)
        })
    }
    /// Get variable name from a template string value
    pub fn get_variable(self) -> String {
        match Regex::new(RE_HANDLEBARS) {
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
    /// use xylem_lib::script::StringTemplate;
    ///
    /// // OK
    /// let template = StringTemplate::new("{{ INPUT_DIRECTORY }}");
    /// assert!(template.is_handlebars());
    /// // NG
    /// let template = StringTemplate::new("{{ MustBeUpperSnakeCase }}");
    /// assert!(!template.is_handlebars());
    /// ```
    pub fn is_handlebars(self) -> bool {
        match Regex::new(RE_HANDLEBARS) {
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
    /// use xylem_lib::script::StringTemplate;
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
        let updated = match Regex::new(RE_TEMPLATE_SPACES) {
            | Ok(re) => re.replace_all(&self.value, "").to_string(),
            | _ => self.value,
        };
        self.value = updated;
        self
    }
}
/// Add forward slash to end of folder path
fn add_dot_slash(path: PathBuf) -> PathBuf {
    if path.starts_with("./") {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("./{}", path.display()))
    }
}
fn create_conda_environment_script(current_dir: String) -> Option<Script> {
    create_environment_script(current_dir, "conda", get_conda_environment_list())
}
fn create_environment_script(current_dir: String, manager: &str, environments: Vec<String>) -> Option<Script> {
    let manifest_path = PathBuf::from(&current_dir).join("environment.yml");
    match manifest_path.try_exists() {
        | Ok(true) => {
            info!(path = %manifest_path.display(), "=> {} Manifest file", Label::found());
            match get_conda_environment_name(manifest_path) {
                | Some(name) if environments.contains(&name) => {
                    info!(name, "=> {} Virtual environment", Label::found());
                    None
                }
                | Some(_) => {
                    let arguments = vec!["env", "create", "-f", "environment.yml"]
                        .into_iter()
                        .map(String::from)
                        .collect::<Vec<_>>();
                    let command = Command::init().name(manager.to_string()).build();
                    Some(Script::init().command(command).arguments(arguments).build().with_current_dir(current_dir))
                }
                | None => None,
            }
        }
        | Ok(false) => {
            debug!(path = %manifest_path.display(), "=> {} Manifest file", Label::not_found());
            None
        }
        | Err(_) => {
            error!(
                path = %manifest_path.display(),
                "=> {} Error while trying to test existence",
                Label::fail()
            );
            None
        }
    }
}
fn create_mamba_environment_script(current_dir: String) -> Option<Script> {
    create_environment_script(current_dir, "mamba", get_mamba_environment_list())
}
fn create_pixi_environment_script(current_dir: String, environment: VirtualEnvironment) -> Option<Script> {
    match find_pixi_manifest(PathBuf::from(&current_dir)) {
        | Some(manifest_path) => {
            info!(path = %manifest_path.display(), "=> {} Pixi manifest file", Label::found());
            let command = Command::init().name("pixi".to_string()).build();
            let arguments = vec!["install", "--environment", &environment.name]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>();
            Some(Script::init().command(command).arguments(arguments).build().with_current_dir(current_dir))
        }
        | None => {
            debug!(path = current_dir, "=> {} Pixi manifest file", Label::not_found());
            None
        }
    }
}
fn default_container_runtime() -> ContainerRuntime {
    ContainerRuntime::Docker
}
fn default_true() -> bool {
    true
}
fn find_pixi_manifest(path: PathBuf) -> Option<PathBuf> {
    let pixi_toml = path.join("pixi.toml");
    match pixi_toml.is_file() {
        | true => Some(pixi_toml),
        | false => {
            let pyproject = path.join("pyproject.toml");
            read_text(pyproject.clone())
                .ok()
                .and_then(|content| content.parse::<toml::Table>().ok())
                .and_then(|manifest| {
                    manifest
                        .get("tool")
                        .and_then(toml::Value::as_table)
                        .and_then(|tool| tool.get("pixi"))
                        .map(|_| pyproject)
                })
        }
    }
}
/// Get list of Conda virtual environments through Conda.
pub fn get_conda_environment_list() -> Vec<String> {
    get_environment_list("conda")
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
/// use xylem_lib::script::get_conda_environment_name;
///
/// get_conda_environment_name(PathBuf::from("/path/to/environment.yml"));
/// ```
pub fn get_conda_environment_name(path: PathBuf) -> Option<String> {
    match path.try_exists() {
        | Ok(true) => match read_text(path) {
            | Ok(content) => content.lines().next().and_then(|line| line.strip_prefix("name: ")).map(str::to_string),
            | _ => None,
        },
        | _ => None,
    }
}
fn get_environment_list(manager: &str) -> Vec<String> {
    let command = Command::init().name(manager.to_string()).build();
    let arguments = vec!["env".to_string(), "list".to_string()];
    let script = Script::init().command(command.clone()).arguments(arguments).build();
    if command.test() {
        script
            .run()
            .unwrap_or_default()
            .join("\n")
            .replace(['#', '*'], "")
            .split('\n')
            .filter(|x| !x.is_empty())
            .skip(1)
            .filter_map(|line| {
                line.split_whitespace().next().and_then(|value| match value.contains(is_separator) {
                    | true => PathBuf::from(value).file_name().and_then(|name| name.to_str()).map(str::to_string),
                    | false => Some(value.to_string()),
                })
            })
            .collect::<Vec<String>>()
    } else {
        vec![]
    }
}
/// Get list of Conda-format virtual environments through Mamba.
pub fn get_mamba_environment_list() -> Vec<String> {
    get_environment_list("mamba")
}
/// Return the deterministic runtime name for a managed container.
pub fn managed_container_name(logical_name: &str, run_id: &str) -> String {
    format!("xylem-{logical_name}-{run_id}")
}
pub(crate) fn module_working_directory(uri: &str, root: Option<&PathBuf>) -> Option<String> {
    let location = Location::from(uri);
    match ModuleUri::from(&location) {
        | ModuleUri::Url(_) => Some(root.map_or_else(|| location.hash(), |path| absolute_path(path.join(location.hash())))),
        | ModuleUri::File(path) => Some(absolute_path(path)),
        | ModuleUri::Unknown(_) => None,
    }
}
#[cfg(test)]
mod tests;
