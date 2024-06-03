#![allow(dead_code)]
use clap::ValueEnum;
use is_terminal::IsTerminal;
use log::{error, info};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

pub type Void = Result<(), Box<dyn Error>>;
pub type Output = Vec<Option<Details>>;

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
#[derive(Clone, Debug, Deserialize, Eq, Serialize, PartialOrd, Ord)]
pub struct Details {
    // Results of applying check to given path
    pub condition: bool,
    // Description of what was checked (typically a file/folder path)
    pub description: Option<String>,
    // Explanation of why the check failed (null when condition is true)
    pub error: Option<String>,
}
impl PartialEq for Details {
    fn eq(&self, other: &Self) -> bool {
        (self.description == other.description) && (self.condition == other.condition)
    }
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReporterType {
    Console,
    Json,
    Stdout,
}
impl fmt::Display for ReporterType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
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
pub enum StdinIsPipedResult {
    StdinIsPiped,
    StdinIsNotPiped,
}
pub enum StdoutIsPipedResult {
    StdoutIsPiped,
    StdoutIsNotPiped,
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
/// Check if a file has a given file extension
pub fn has_extension(extension: &str, path: &Path) -> bool {
    let file_extension = path.extension().unwrap_or_default().to_str().unwrap_or_default();
    extension == file_extension
}
/// Check if a string is a handlebars template
pub fn is_handlebars_template(value: &str) -> bool {
    match Regex::new(r"^\{\{(?P<variable>[A-Z_ ]*)\}\}$") {
        | Ok(re) => re.is_match(value),
        | Err(_) => false,
    }
}
/// Get variable name from a handlebars template
pub fn parse_handlebars_template(value: &str) -> String {
    match Regex::new(r"^\{\{(?P<variable>[A-Z_ ]*)\}\}$") {
        | Ok(re) => match re.captures(value) {
            | Some(captures) => captures["variable"].trim().to_string(),
            | None => "".to_string(),
        },
        | _ => "".to_string(),
    }
}
/// Check if stdin is piped
///
/// Example: echo "hello world" | pipe
pub fn is_stdin_piped() -> StdinIsPipedResult {
    if !io::stdin().is_terminal() {
        StdinIsPipedResult::StdinIsPiped
    } else {
        StdinIsPipedResult::StdinIsNotPiped
    }
}
/// Check if stdout is piped
///
/// Example: pipe | process
pub fn is_stdout_piped() -> StdoutIsPipedResult {
    if !io::stdout().is_terminal() {
        StdoutIsPipedResult::StdoutIsPiped
    } else {
        StdoutIsPipedResult::StdoutIsNotPiped
    }
}
pub fn to_string(values: Vec<&str>) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests;
