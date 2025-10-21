#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::wrong_self_convention)]
use acorn_lib::util::{read_file, Label};
use bon::Builder;
use clap::ValueEnum;
use color_eyre::eyre::{Report, Result};
use derive_more::Display;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{error, trace};

pub mod config;
pub mod script;
pub mod util;

use config::*;

pub type Void = Result<(), Report>;
pub type Output = Vec<Option<Details>>;

#[derive(ValueEnum, Copy, Clone, Debug, Display, PartialEq, Eq)]
pub enum ReporterType {
    Console,
    Json,
    Stdout,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PyProjectToml {
    #[serde(flatten)]
    inner: pyproject_toml::PyProjectToml,
    tool: Option<PyProjectTool>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectTool {
    xylem: Option<ToolXylem>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ToolXylem {
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
/// Semantic version
///
/// see <https://semver.org/>
#[derive(Builder, Clone, Debug, Deserialize, Display, Serialize)]
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
impl PartialEq for Details {
    fn eq(&self, other: &Self) -> bool {
        (self.description == other.description) && (self.condition == other.condition)
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
            | Some(PyProjectTool { xylem, .. }) => match xylem {
                | Some(ToolXylem { module_name, .. }) => Some(module_name),
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

#[cfg(test)]
mod tests;
