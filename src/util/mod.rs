#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]
use clap::ValueEnum;
use fancy_regex::Regex;
use is_terminal::IsTerminal;
use log::{error, info};
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
    env::{set_var, var},
    fmt, io,
};
use uriparse::Scheme;
use uriparse::URI;

use std::process;

const PREFIX: &str = "PIPE";

pub type CommandResult = Result<(), Box<(dyn StdError + 'static)>>;
pub type Void = Result<(), Box<dyn Error>>;
pub type Output = Vec<Option<Details>>;

/// Values that from Serde that can be set as environment variables
#[derive(Clone, Debug)]
enum EnvironmentValue {
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
    /// Module exported in .wasm WASI-compliant format and accessed via a command line interface
    #[serde(alias = "wasm")]
    WebAssembly { run: String },
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
#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub conda_environment: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Configuration version in semver format
    version: String,
    #[serde(alias = "doc")]
    description: String,
    requirements: Vec<String>,
    /// Keywords or tags used to describe this module
    keywords: Option<Vec<String>>,
    modules: Vec<Module>,
    variables: Option<Map<String, Value>>,
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
    module_type: ModuleType,
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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptTemplate {
    command: String,
    environment: Option<String>,
    arguments: Instruction,
}
impl Config {
    /// Get evironment variables saved with PIPE prefix
    pub fn get_environment_variables(&self) -> HashMap<String, String> {
        match self.variables {
            | Some(ref variables) => {
                let mut lookup: HashMap<String, String> = HashMap::new();
                variables.iter().for_each(|(k, _)| {
                    let prefixed = format!("{}_{}", PREFIX, k);
                    lookup.insert(k.to_string(), var(prefixed).unwrap());
                });
                info!("PIPE Environment variables: {:?}", lookup);
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
        info!("Imported config: {:?}", json);
        json
    }
    /// Set environment variables from PIPE configuration "variables" attribute
    pub fn set_environment_variables(&self) -> Void {
        match &self.variables {
            | Some(variables) => {
                for (k, v) in variables {
                    info!("Setting PIPE environment variable: {:?} = {:?}", k, v);
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
    fn set_variable(self, key: String) {
        let prefixed = format!("{}_{}", PREFIX, key);
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
        fmt::Debug::fmt(self, f)
    }
}
impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
impl ModuleUri<'_> {
    fn from(value: &str) -> ModuleUri<'_> {
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
    pub fn new(command: String, arguments: Vec<String>) -> Script {
        Script {
            command: Command {
                name: command,
                conda_environment: None,
            },
            arguments,
        }
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
    fn from_template(path: PathBuf) -> Script {
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
            | Instruction::ShellFormat(value) => remove_handlebars_spaces(value)
                .split_whitespace()
                .map(|s| s.to_owned())
                .collect::<Vec<_>>(),
            | Instruction::ExecFormat(values) => values.into_iter().map(remove_handlebars_spaces).collect(),
        };
        Script::new(template.command, arguments)
    }
    fn run(&'a self) -> process::Output {
        const ERROR_MESSAGE: &str = "[ERROR] => Failed to execute process";
        return process::Command::new(self.command.name.clone())
            .args(self.arguments.clone().iter())
            .output()
            .expect(ERROR_MESSAGE);
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
/// Get parent of a file/folder path
pub fn get_parent(path: PathBuf) -> PathBuf {
    PathBuf::from(path.parent().unwrap())
}
/// Get variable name from a handlebars template
pub fn get_variable_from_handlebars_template(value: &str) -> String {
    match Regex::new(r"^\{\{(?P<variable>[A-Z_ ]*)\}\}$") {
        | Ok(re) => match re.captures(value) {
            | Ok(Some(captures)) => captures["variable"].trim().to_string(),
            | _ => "".to_string(),
        },
        | _ => "".to_string(),
    }
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
/// Can also be used to check if a command exists within a [conda](https://conda.io/projects/conda/en/latest/user-guide/getting-started.html) environment
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
///     conda_environment: None
/// };
/// assert!(is_command(command));
///
/// // Using conda environment
/// let command = Command {
///     name: "pip".to_owned(),
///     conda_environment: Some("base".to_owned())
/// };
/// assert!(is_command(command));
/// ```
pub fn is_command(value: Command) -> bool {
    let script = if cfg!(target_os = "windows") {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda".to_owned(),
                    conda_environment: Some(name.clone()),
                },
                arguments: to_string(vec!["run", "--name", &name, "pwsh", "-NoProfile", "-Command", "command", &value.name]),
            },
            | None => Script {
                command: Command {
                    name: "pwsh".to_owned(),
                    conda_environment: None,
                },
                arguments: to_string(vec!["-NoProfile", "-Command", "command", &value.name]),
            },
        }
    } else {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda".to_owned(),
                    conda_environment: Some(name.clone()),
                },
                arguments: to_string(vec!["run", "--name", &name, "which", &value.name]),
            },
            | None => Script {
                command: Command {
                    name: "which".to_owned(),
                    conda_environment: None,
                },
                arguments: vec![value.name],
            },
        }
    };
    let output = script.run();
    match output {
        | process::Output { stdout, .. } if !stdout.is_empty() => {
            let path = String::from_utf8_lossy(&stdout);
            info!("Command found at {:?}", path.trim());
            return PathBuf::from(path.trim()).exists();
        }
        | _ => false,
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
/// # Examples
/// ```
/// use pipe::util::is_handlebars_template;
///
/// // OK
/// assert!(is_handlebars_template("{{INPUT_DIRECTORY}}"));
/// assert!(is_handlebars_template("{{ OUTPUT_DIRECTORY }}"));
/// // NG
/// assert!(!is_handlebars_template("{{ MustBeUpperSnakeCase }}"));
/// assert!(!is_handlebars_template("hello world"));
/// ```
pub fn is_handlebars_template(value: &str) -> bool {
    match Regex::new(r"^\{\{(?P<variable>[A-Z_ ]*)\}\}$") {
        | Ok(re) => match re.is_match(value) {
            | Ok(value) => value,
            | _ => false,
        },
        | Err(_) => false,
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
            info!("Read file: {:?}", path);
            file.read_to_string(&mut content)
        }
        | Err(err) => {
            error!("Failed to read file: {:?}", path);
            Err(err)
        }
    };
    Ok(content)
}
/// Remove spaces around variable name within a handlebars template
///
/// # Parameters
/// - `value` - string that contains 0 or more handlebars templates
///
/// # Examples
/// ```
/// use pipe::util::remove_handlebars_spaces;
///
/// remove_handlebars_spaces("{{ variable }}".to_owned());   // "{{variable}}"
/// remove_handlebars_spaces("{{ variable   }}".to_owned()); // "{{variable}}"
/// remove_handlebars_spaces("{{variable}}".to_owned());     // "{{variable}}"
/// ```
pub fn remove_handlebars_spaces(value: String) -> String {
    match Regex::new(r"((?<=\{\{) *(?=[A-Z])|(?<=[A-Z]) *(?=\}\}))") {
        | Ok(re) => re.replace_all(&value, "").to_string(),
        | Err(err) => err.to_string(),
    }
}
pub fn to_string(values: Vec<&str>) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests;
