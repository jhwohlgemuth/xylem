//! Xylem Library
//! > "Digital xylem" that transports data through workflows with structure and support
use acorn::io::read_file;
use acorn::util::Label;
use clap::ValueEnum;
use color_eyre::eyre::{Report, Result};
use derive_more::Display;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{error, trace};

pub mod config;
pub mod script;

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
impl PartialEq for Details {
    fn eq(&self, other: &Self) -> bool {
        (self.description == other.description) && (self.condition == other.condition)
    }
}
impl core::ops::Deref for PyProjectToml {
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

#[cfg(test)]
mod tests;
