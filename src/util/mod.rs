#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::wrong_self_convention)]
use clap::ValueEnum;
use fancy_regex::Regex;
use is_terminal::IsTerminal;
use log::{debug, error, info};
use serde::ser::StdError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{
    env::{set_var, var, vars},
    fmt, io,
};
use uriparse::Scheme;
use uriparse::URI;

use std::process;

pub const PREFIX: &str = "pipe";

pub type CommandResult = Result<(), Box<(dyn StdError + 'static)>>;
pub type Void = Result<(), Box<dyn Error>>;
pub type Output = Vec<Option<Details>>;

/// Values that from Serde that can be set as environment variables
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleLanguage {
    Binary,
    Csharp,
    JavaScript,
    Julia,
    Python,
    Rust,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    File(PathBuf),
    Url(URI<'a>),
    Unknown,
}
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Module {
    name: String,
    #[serde(flatten)]
    pub module_type: ModuleType,
    description: Option<String>,
    variables: Option<Map<String, Value>>,
    before_script: Option<Instruction>,
    after_script: Option<Instruction>,
    template: Option<String>,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SemanticVersion {
    /// Version when you make incompatible API changes
    pub major: u32,
    /// Version when you add functionality in a backward compatible manner
    pub minor: u32,
    /// Version when you make backward compatible bug fixes
    pub patch: u32,
}
#[derive(Clone, Debug)]
pub struct Script {
    pub command: Command,
    pub arguments: Vec<String>,
    /// Directory in which the script should be executed when run method is invoked
    pub current_dir: String,
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
    /// Get evironment variables saved with PIPE prefix
    pub fn get_environment_variables(&self) -> HashMap<String, String> {
        match self.variables {
            | Some(ref variables) => {
                let mut lookup: HashMap<String, String> = HashMap::new();
                variables.iter().for_each(|(k, _)| {
                    let prefixed = format!("_{}_{}", PREFIX.to_uppercase(), k);
                    lookup.insert(k.to_string(), var(prefixed).unwrap());
                });
                debug!("PIPE Environment variables: {:?}", lookup);
                lookup
            }
            | None => HashMap::new(),
        }
    }
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
        debug!("Imported config: {:?}", json);
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
    fn from_value(value: Value) -> EnvironmentValue {
        match value {
            | Value::String(val) => EnvironmentValue::String(val),
            | Value::Number(val) => EnvironmentValue::Number(val),
            | Value::Bool(val) => EnvironmentValue::Bool(val),
            | _ => EnvironmentValue::Unknown,
        }
    }
    pub fn get_all(prefix: &str) -> Vec<(String, String)> {
        let pre = format!("_{}_", prefix.to_uppercase());
        vars()
            .filter(|(key, _)| key.starts_with(&pre))
            .map(|(key, value)| {
                let key = key.trim_start_matches(&pre).to_string();
                (key, value)
            })
            .collect()
    }
    pub fn print_all(prefix: &str) {
        EnvironmentValue::get_all(prefix).into_iter().for_each(|(key, value)| {
            info!("=> [ENV] {}: {}", key, value);
        });
    }
    fn set_variable(self, key: String) {
        let prefixed = format!("_{}_{}", PREFIX.to_uppercase(), key);
        let prefixed_key = OsStr::new(&prefixed);
        match self {
            | EnvironmentValue::String(value) => set_var(prefixed_key, value),
            | EnvironmentValue::Number(value) => set_var(prefixed_key, value.to_string()),
            | EnvironmentValue::Bool(value) => set_var(prefixed_key, value.to_string()),
            | EnvironmentValue::Unknown => (),
        }
    }
}
impl fmt::Display for EnvironmentValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl fmt::Display for ReporterType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "'{} {}' in {}", self.command.name, self.arguments.join(" "), self.current_dir)
    }
}
impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
impl ModuleUri<'_> {
    pub fn from(value: &str) -> ModuleUri<'_> {
        match URI::try_from(value) {
            | Ok(uri) => match uri.scheme() {
                | Scheme::HTTP | Scheme::HTTPS => ModuleUri::Url(uri),
                | Scheme::File => ModuleUri::File(PathBuf::from(uri.path().to_string())),
                | _ => ModuleUri::Unknown,
            },
            | Err(_) => ModuleUri::Unknown,
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
impl<'a> Script {
    /// Create a new script from a command and arguments
    pub fn new(command: String, arguments: Vec<String>) -> Script {
        Self {
            command: Command {
                name: command,
                virtual_environment: None,
            },
            arguments,
            current_dir: ".".to_string(),
        }
    }
    /// Expand environment variables in the script arguments
    pub fn expand_arguments(mut self) -> Script {
        let expanded = self
            .arguments
            .into_iter()
            .map(|argument| StringTemplate::new(&argument))
            .map(|argument| {
                if StringTemplate::is_handlebars(argument.clone()) {
                    let vars = EnvironmentValue::get_all(PREFIX);
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
        let template = json.expect("[ERROR] => Failed to parse script template").clone();
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
        Script::new(template.command, arguments).with_environment(environment)
    }
    /// Execute script in child process and wait for it to finish
    pub fn run(&'a self) -> String {
        const ERROR_MESSAGE: &str = "[ERROR] => Failed to execute process";
        let command = &self.command.name;
        let current_dir = PathBuf::from(&self.current_dir);
        let output = process::Command::new(command)
            .args(self.arguments.clone().iter())
            .current_dir(current_dir)
            .output();
        match output {
            | Ok(value) if !value.stdout.is_empty() => String::from_utf8_lossy(&value.stdout).trim().to_string(),
            | _ => "".to_string(),
        }
    }
    /// Set current working directory for the script
    ///
    /// Provides builder interface and returns reference to self
    pub fn with_current_dir(mut self, path: &str) -> Self {
        self.current_dir = path.to_string();
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
    /// let script = Script::new("python".to_string(), vec!["-m".to_string(), "lib".to_string()]);
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
        self.command.virtual_environment = environment.clone();
        let command = match environment {
            | Some(ref value) => {
                format!("conda run --name {} {}", value, self.command.name)
            }
            | None => self.command.name.clone(),
        };
        self.command.name = command;
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
        vars.iter().for_each(|(key, value)| debug!("{} = {}", key, value));
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
        match value.virtual_environment {
            | Some(name) => {
                let command = Command {
                    name: "conda".to_owned(),
                    virtual_environment: Some(name.clone()),
                };
                Script {
                    command,
                    arguments: to_string(vec!["run", "--name", &name, "pwsh", "-NoProfile", "-Command", "command", &value.name]),
                    current_dir: ".".to_string(),
                }
            }
            | None => Script {
                command: Command {
                    name: "pwsh".to_owned(),
                    virtual_environment: None,
                },
                arguments: to_string(vec!["-NoProfile", "-Command", "command", &value.name]),
                current_dir: ".".to_string(),
            },
        }
    } else {
        match value.virtual_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda".to_owned(),
                    virtual_environment: Some(name.clone()),
                },
                arguments: to_string(vec!["run", "--name", &name, "which", &value.name]),
                current_dir: ".".to_string(),
            },
            | None => Script {
                command: Command {
                    name: "which".to_owned(),
                    virtual_environment: None,
                },
                arguments: vec![value.name],
                current_dir: ".".to_string(),
            },
        }
    };
    let output = script.run();
    match PathBuf::from(output.clone()).try_exists() {
        | Ok(true) => {
            debug!("Command found at {:?}", output);
            true
        }
        | _ => {
            debug!("Command NOT found at {:?}", output);
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
            debug!("Read file: {:?}", path);
            file.read_to_string(&mut content)
        }
        | Err(err) => {
            error!("Failed to read file: {:?}", path);
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
