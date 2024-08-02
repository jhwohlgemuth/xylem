#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::wrong_self_convention)]
use bon::builder;
use clap::ValueEnum;
use clap_verbosity_flag::Level;
use color_eyre::eyre::{Report, Result};
use derive_more::Display;
use fancy_regex::Regex;
use is_terminal::IsTerminal;
use owo_colors::{OwoColorize, Style, Styled};
use serde::ser::StdError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::{
    env::{set_var, vars},
    io,
};
use tracing::{debug, error, info, warn};
use uriparse::Scheme;
use uriparse::URI;
use valuable_derive::Valuable;

use std::process;

pub type CommandResult = Result<(), Box<(dyn StdError + 'static)>>;
pub type Void = Result<(), Report>;
pub type Output = Vec<Option<Details>>;

/// Values that from Serde that can be set as environment variables
#[derive(Clone, Debug, Display)]
pub enum EnvironmentValue {
    String(String),
    Number(Number),
    Bool(bool),
    Unknown,
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
#[derive(Clone, Debug, Serialize, Deserialize, Valuable)]
#[serde(rename_all = "lowercase")]
pub enum ModuleLanguage {
    Binary,
    Csharp,
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
    Binary { uri: String },
    /// Nested workflow module that allows for grouping modules
    #[serde(alias = "config")]
    Configuration { uri: String },
    /// Provides legacy support for exceuting scripts (e.g. Python projects like [Module A](https://code.ornl.gov/GSHS/common/pipe/module-a))
    Script {
        #[serde(alias = "programmingLanguage")]
        programming_language: ModuleLanguage,
        uri: String,
    },
}
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum TemplateType {
    /// Example: `"some <%= value %>"`
    EmbeddedJavaScript,
    /// Example: `"some <%= value %>"`
    EmbeddedRuby,
    /// Example: `"some {{ value }}"`
    Handlebars,
    /// Example: `"some {{ value }}"`
    Mustache,
    Unknown,
}
#[derive(Clone, Debug, Display)]
#[display(fmt = "{}", name)]
#[builder(start_fn(name = init))]
pub struct Command {
    /// Prefix string to modify command string
    ///
    /// This is commonly usef when running a command within a virtual environment like Conda
    ///
    /// Example:
    /// ```text
    /// "conda run --name base "
    /// ```
    #[builder(default)]
    pub name: String,
    #[builder]
    pub virtual_environment: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Configuration version in semver format
    version: String,
    #[serde(alias = "doc")]
    description: String,
    pub requirements: Vec<String>,
    /// Keywords or tags used to describe this module
    keywords: Option<Vec<String>>,
    pub modules: Vec<Module>,
    /// Environment variables set at the workflow level and available to all modules
    ///
    /// Note: Modules can override these variables
    pub variables: Option<Map<String, Value>>,
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
#[display(fmt = "{name}")]
pub struct Module {
    pub name: String,
    #[serde(flatten)]
    pub module_type: ModuleType,
    pub description: Option<String>,
    pub variables: Option<Map<String, Value>>,
    pub before_script: Option<Instruction>,
    pub after_script: Option<Instruction>,
    pub template: Option<String>,
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
#[derive(Clone, Debug)]
pub struct Reporter {
    /// Reporter type name
    pub name: ReporterType,
    /// Function to run the reporter
    pub run: fn(Output, Option<Parameters>) -> Void,
    /// Options to pass to the reporter
    pub parameters: Option<Parameters>,
}
#[derive(Clone, Debug)]
pub struct Rule<'a> {
    /// Name of the rule (displayed by the progress bar)
    pub name: &'a str,
    /// Function to verify the rule
    pub run: fn(&Path, Option<Parameters>) -> Output,
    /// Regular expression used to find files/folders
    pub expression: &'a str,
    /// Options to pass to the checker
    pub parameters: Option<Parameters>,
}
/// Semantic version
///
/// see <https://semver.org/>
#[derive(Clone, Debug, Deserialize, Display, Serialize)]
#[display(fmt = "{}.{}.{}", major, minor, patch)]
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
/// let script = Script::init().command(command).arguments(arguments).build();
/// ```
#[derive(Clone, Debug, Display)]
#[display(fmt = "{} {}", "self.get_command()", "self.get_arguments().join(\" \")")]
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
    command: String,
    environment: Option<String>,
    arguments: Instruction,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StringTemplate {
    value: String,
    template_type: TemplateType,
}
impl Config {
    /// Import PIPE configuration JSON file
    ///
    /// See the [PIPE configuration syntax wiki page](https://code.ornl.gov/groups/GSHS/common/pipe/-/wikis/PIPE-Standards/Configuration-Syntax)
    /// for more information configuration files
    ///
    pub fn read_json(path: PathBuf) -> serde_json::Result<Config> {
        let content = match read_file(path.clone()) {
            | Ok(value) => value,
            | Err(_) => {
                error!("Failed to read config file: {:?}", path);
                "".to_owned()
            }
        };
        let json: serde_json::Result<Config> = serde_json::from_str(&content);
        debug!("=> {} Configuration = {:#?}", "READ".green().on_default_color(), json.dimmed());
        json
    }
    /// Set environment variables from PIPE configuration "variables" attribute
    pub fn set_environment_variables(&self) -> Void {
        match &self.variables {
            | Some(variables) => {
                for (k, v) in variables {
                    debug!("Setting PIPE environment variable: {:?} = {:?}", k, v);
                    EnvironmentValue::from_value(v.clone()).set_variable(k.clone());
                }
            }
            | None => (),
        }
        Ok(())
    }
}
impl PartialEq for Details {
    fn eq(&self, other: &Self) -> bool {
        (self.description == other.description) && (self.condition == other.condition)
    }
}
impl EnvironmentValue {
    pub const PREFIX: &'static str = "pipe";

    fn from_string(value: String) -> EnvironmentValue {
        EnvironmentValue::String(value)
    }
    fn from_value(value: Value) -> EnvironmentValue {
        match value {
            | Value::String(val) => EnvironmentValue::String(val),
            | Value::Number(val) => EnvironmentValue::Number(val),
            | Value::Bool(val) => EnvironmentValue::Bool(val),
            | _ => EnvironmentValue::Unknown,
        }
    }
    fn prepend_prefix(value: String) -> String {
        let prefixed = format!("_{}_{}", EnvironmentValue::PREFIX.to_uppercase(), value);
        prefixed
    }
    fn trim_prefix(key: String) -> String {
        let pre = format!("_{}_", EnvironmentValue::PREFIX.to_uppercase());
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
        let pre = format!("_{}_", EnvironmentValue::PREFIX.to_uppercase());
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
            debug!(key, value, "=> [{}]", "ENV".dimmed());
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
            .for_each(|(key, value)| EnvironmentValue::from_string(value).set_variable(key));
    }
}
impl Label {
    pub fn dry_run() -> Styled<&'static &'static str> {
        let style = Style::new().white().on_yellow();
        " DRY_RUN ".style(style)
    }
    pub fn found() -> Styled<&'static &'static str> {
        let style = Style::new().green().on_default_color();
        "FOUND".style(style)
    }
    pub fn not_found() -> Styled<&'static &'static str> {
        let style = Style::new().red().on_default_color();
        "NOT_FOUND".style(style)
    }
    pub fn run() -> Styled<&'static &'static str> {
        let style = Style::new().white().on_yellow();
        " RUN ▶ ".style(style)
    }
    pub fn using() -> Styled<&'static &'static str> {
        let style = Style::new().dimmed();
        "USING".style(style)
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
                let segments = uri.path().segments().iter().map(|s| s.to_string()).collect::<Vec<_>>();
                format!("{}_{}", host, segments[segments.len() - 2..].join("_").to_lowercase())
            }
            | ModuleUri::File(value) => value.clone().into_os_string().into_string().unwrap(),
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
    /// Expand environment variables in the script arguments from environment
    ///
    /// This method does not set environment variables, but only interpolates the values within the script arguments
    ///
    /// Example:
    /// ```
    /// // Environment: ANSWER=42
    /// let command = Command::init().name("echo").build();
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
    /// let envs = HashMap::from([("ANSWER".to_string(), "42".to_string())]);
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
    /// Create a script from a template JSON file in [ScriptTemplate] format.
    ///
    /// You can easily import the script with
    /// ```
    /// let path = PathBuf::from("./tests/fixtures/template.json");
    /// let script = Script::from_template(path);
    /// ```
    ///
    /// and execute `python -m lib --input $INPUT_DIRECTORY --output $OUTPUT_DIRECTORY` with
    /// ```
    /// let output = script.run();
    /// ```
    pub fn from_template(path: PathBuf) -> Script {
        let content = match read_file(path.clone()) {
            | Ok(value) => value,
            | Err(_) => {
                error!("Failed to read config file: {:?}", path);
                "".to_owned()
            }
        };
        let json: serde_json::Result<ScriptTemplate> = serde_json::from_str(&content);
        let template = json.expect("=> [ERROR] Failed to parse script template").clone();
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
        let environment = template.environment;
        let command = Command::init().name(template.command).build();
        Script::init().command(command).arguments(arguments).build().with_environment(environment)
    }
    fn get_command(&self) -> String {
        match &self.command.virtual_environment {
            | Some(_) => "conda".to_string(),
            | _ => self.command.name.clone(),
        }
    }
    fn get_arguments(&self) -> Vec<String> {
        match &self.command.virtual_environment {
            | Some(name) => {
                let mut arguments = to_string(vec!["run", "--name", name, &self.command.name]);
                arguments.extend(self.arguments.clone());
                arguments
            }
            | _ => self.arguments.clone(),
        }
    }
    /// Execute script in child process and wait for it to finish, print to stdout and ignore output
    pub fn run(&self) {
        let command = &self.get_command();
        let arguments = self.get_arguments();
        let current_dir = PathBuf::from(&self.current_dir);
        let envs = self.envs.clone().unwrap_or_default();
        warn!(script = self.to_string(), "=> {}", Label::run());
        let mut output: Vec<String> = vec![];
        let mut cmd = process::Command::new(command)
            .args(arguments.iter())
            .current_dir(current_dir)
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .envs(envs)
            .spawn()
            .unwrap();
        {
            let stdout = match cmd.stdout.as_mut() {
                | Some(value) => value,
                | None => {
                    error!(script = self.to_string(), "=> [{}]", "FAIL".red());
                    return;
                }
            };
            let reader = BufReader::new(stdout);
            let lines = reader.lines().map_while(Result::ok).collect::<Vec<String>>();
            debug!("=> {:?}", lines.join("\n").cyan().dimmed());
            output.extend(lines);
        }
        match cmd.wait_with_output() {
            | Ok(result) => {
                if result.status.success() {
                    debug!("=> [{}] {:?}", "OUTPUT".dimmed(), result);
                    debug!("=> [{}] {:?}", "OUTPUT".dimmed(), output.iter().last().unwrap().to_string());
                } else {
                    error!(script = self.to_string(), "=> [{}]", "FAIL".red());
                    debug!("=> [{}] {:?}", "OUTPUT".dimmed(), result);
                }
            }
            | Err(error) => {
                error!(error = error.to_string(), script = self.to_string(), "=> [{}]", "FAIL".red());
            }
        }
    }
    /// Execute script in child process and wait for it to finish, return the output
    pub fn run_with_output(&self) -> String {
        let command = &self.get_command();
        let arguments = self.get_arguments();
        let current_dir = PathBuf::from(&self.current_dir);
        let envs = self.envs.clone().unwrap_or_default();
        warn!(script = self.to_string(), cwd = current_dir.to_str().unwrap(), "=> {}", Label::run());
        let output = process::Command::new(command)
            .args(arguments.iter())
            .current_dir(current_dir)
            .envs(envs)
            .output();
        match output {
            | Ok(value) if !value.stdout.is_empty() => String::from_utf8_lossy(&value.stdout).trim().to_string(),
            | _ => "".to_string(),
        }
    }
    /// Set current working directory for the script
    ///
    /// Provides builder interface and returns reference to self
    pub fn with_current_dir(mut self, path: String) -> Self {
        self.current_dir = path;
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
    /// let command = Command::init().name("python".to_string()).build();
    /// let arguments = to_string(vec!["-m", "lib"]);
    /// let script = Script::init().command(command).arguments(arguments).build();
    /// ```
    ///
    /// Run with no virtual environment
    /// ```
    /// script.with_environment(None).run()
    /// ```
    ///
    /// Run using "module-a" Conda virtual environment (basically executes script with `conda run -n <name>`)
    /// ```
    /// script.with_environment(Some("module-a".to_string())).run()
    /// ```
    pub fn with_environment(mut self, environment: Option<String>) -> Self {
        self.command.virtual_environment.clone_from(&environment);
        self
    }
    /// Set script environment variables to be set during script.run (or script.run_with_output) call
    pub fn with_envs(mut self, values: HashMap<String, String>) -> Self {
        self.envs = Some(values.clone());
        self
    }
}
impl StringTemplate {
    const RE_HANDLEBARS: &'static str = r"^\{\{(?P<variable>[A-Z_ ]*)\}\}$";
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
    /// use pipe::util::is_handlebars_template;
    ///
    /// // OK
    /// assert!(is_handlebars("{{INPUT_DIRECTORY}}"));
    /// assert!(is_handlebars("{{ OUTPUT_DIRECTORY }}"));
    /// // NG
    /// assert!(!is_handlebars("{{ MustBeUpperSnakeCase }}"));
    /// assert!(!is_handlebars("hello world"));
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
    /// use pipe::util::remove_handlebars_spaces;
    ///
    /// remove_spaces("{{ variable }}".to_owned());   // "{{variable}}"
    /// remove_spaces("{{ variable   }}".to_owned()); // "{{variable}}"
    /// remove_spaces("{{variable}}".to_owned());     // "{{variable}}"
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
pub fn add_forward_slash(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        PathBuf::from(format!("{}/", path.display()))
    } else {
        path.to_path_buf()
    }
}
/// Create Conda environment from an environment.yml file
pub fn create_conda_environment(uri: &str, dry_run: bool, _verbosity: Option<Level>) {
    let working_dir = ModuleUri::from(uri).working_directory(None);
    let manifest_path = PathBuf::from(&working_dir).join("environment.yml");
    match manifest_path.try_exists() {
        | Ok(true) => {
            info!(path = manifest_path.to_str().unwrap(), "=> {} Manifest file", Label::found());
            match get_conda_environment_name(manifest_path.clone()) {
                | Some(name) => {
                    if get_conda_environment_list().contains(&name) {
                        info!(name, "=> {} Virtual environment", Label::found());
                    } else {
                        let mamba = Command::init().name("mamba".to_string()).build();
                        let name = (if is_command(mamba) { "mamba" } else { "conda" }).to_string();
                        let arguments = to_string(vec!["env", "create", "-f", "environment.yml"]);
                        let command = Command::init().name(name).build();
                        let script = Script::init()
                            .command(command)
                            .arguments(arguments)
                            .build()
                            .with_current_dir(working_dir.clone());
                        if dry_run {
                            info!(script = script.to_string(), cwd = working_dir, "=> {}", Label::dry_run());
                        } else {
                            script.run();
                        }
                    }
                }
                | None => (),
            };
        }
        | _ => {
            error!(path = manifest_path.to_str().unwrap(), "=> {} Manifest file", Label::not_found());
        }
    }
}
/// Get list of Conda virtual environments
pub fn get_conda_environment_list() -> Vec<String> {
    let command = Command::init().name("conda".to_string()).build();
    let arguments = to_string(vec!["env", "list"]);
    let script = Script::init().command(command).arguments(arguments).build();
    script
        .run_with_output()
        .replace(['#', '*'], "")
        .split('\n')
        .filter(|x| !x.is_empty())
        .skip(2)
        .map(|x| x.trim().chars().take_while(|x| !x.is_whitespace()).collect())
        .collect::<Vec<String>>()
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
/// get_conda_environment_name(PathBuf::from("/path/to/environment.yml"))
/// // Some("module-a")
/// ```
pub fn get_conda_environment_name(path: PathBuf) -> Option<String> {
    match read_file(path) {
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
    }
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
/// use pipe::util::has_extension;
///
/// assert!(has_extension("txt", Path::new("hello.txt")));
/// assert!(!has_extension("txt", Path::new("hello.md")));
/// assert!(!has_extension("", Path::new("/path/to/folder")));
/// ```
pub fn has_extension(extension: &str, path: &Path) -> bool {
    let file_extension = path.extension().unwrap_or_default().to_str().unwrap_or_default();
    extension == file_extension
}
/// Check if a command exists in current command line context
///
/// Can also be used to check if a command exists within a [conda](https://conda.io/projects/conda/en/latest/user-guide/getting-started.html) virtual environment
///
/// # Parameters
/// - `value` - [Command] value to check
///
/// # Examples
/// ```
/// use pipe::util::{is_command, Command};
///
/// let command = Command {
///     name: "ls".to_owned(),
///     virtual_environment: None
/// };
/// assert!(is_command(command));
///
/// // Using conda virtual environment
/// let command = Command {
///     name: "pip".to_owned(),
///     virtual_environment: Some("base".to_owned())
/// };
/// assert!(is_command(command));
/// ```
pub fn is_command(value: Command) -> bool {
    let script = if cfg!(target_os = "windows") {
        let command = Command::init()
            .name("pwsh".to_owned())
            .maybe_virtual_environment(value.virtual_environment)
            .build();
        let arguments = to_string(vec!["-NoProfile", "-Command", "command", &value.name]);
        Script::init().command(command).arguments(arguments).build()
    } else {
        let command = Command::init()
            .name("which".to_owned())
            .maybe_virtual_environment(value.virtual_environment)
            .build();
        let arguments = vec![value.name];
        Script::init().command(command).arguments(arguments).build()
    };
    let output = script.run_with_output();
    match PathBuf::from(output.clone()).try_exists() {
        | Ok(true) => {
            info!(output, "=> {} Command", "FOUND".green().on_default_color());
            true
        }
        | _ => {
            error!(output, "=> {} Command", "NOTFOUND".red().on_default_color());
            false
        }
    }
}
/// Check if stdin is piped
///
/// # Examples
/// ```
/// // Using pipe command line tool
/// echo "hello world" | pipe
/// ```
pub fn is_stdin_piped() -> StdinIsPipedResult {
    if !io::stdin().is_terminal() {
        StdinIsPipedResult::StdinIsPiped
    } else {
        StdinIsPipedResult::StdinIsNotPiped
    }
}
/// Check if stdout is piped
///
/// # Examples
/// ```
/// // Using pipe command line tool
/// pipe | process
/// ```
pub fn is_stdout_piped() -> StdoutIsPipedResult {
    if !io::stdout().is_terminal() {
        StdoutIsPipedResult::StdoutIsPiped
    } else {
        StdoutIsPipedResult::StdoutIsNotPiped
    }
}
/// Read content of a file to a string
///
/// Fails when the file cannot be found or read
///
/// # Examples
/// ```
/// use std::path::PathBuf;
/// use pipe::util::read_file;
///
/// let path = PathBuf::from("./tests/fixtures/template.json");
/// let content = read_file(path).unwrap();
pub fn read_file(path: PathBuf) -> Result<String, Box<dyn Error>> {
    let mut content = String::new();
    let _ = match File::open(path.clone()) {
        | Ok(mut file) => {
            debug!(path = path.to_str().unwrap(), "=> {}", "READ".green().on_default_color());
            file.read_to_string(&mut content)
        }
        | Err(err) => {
            debug!(path = path.to_str().unwrap(), "=> {}", " FAIL ".white().on_red());
            Err(err)
        }
    };
    Ok(content)
}
pub fn to_string(values: Vec<&str>) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests;
