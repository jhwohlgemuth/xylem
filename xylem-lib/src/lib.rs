#![cfg_attr(not(test), deny(clippy::exit, clippy::expect_used, clippy::panic, clippy::unwrap_used))]
//! Xylem Library
//! > Digital xylem that transports data through workflows with structure and support
use clap::ValueEnum;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;
use std::path::PathBuf;

use crate::error::PyProjectError;

pub mod completion;
pub mod config;
pub mod container;
pub mod error;
pub mod execution;
pub mod journal;
pub mod outcome;
pub mod plan;
pub mod progress;
pub mod remote;
pub mod script;
pub mod util;

pub use config::*;

#[derive(ValueEnum, Copy, Clone, Debug, Display, PartialEq, Eq)]
pub enum ReporterType {
    Console,
    Json,
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
impl core::ops::Deref for PyProjectToml {
    type Target = pyproject_toml::PyProjectToml;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl PyProjectToml {
    pub fn read(path: PathBuf) -> Result<Self, PyProjectError> {
        read_to_string(&path)
            .map_err(|source| PyProjectError::Read { path: path.clone(), source })
            .and_then(|content| match content.trim().is_empty() {
                | true => Err(PyProjectError::Empty { path }),
                | false => toml::from_str(&content).map_err(|source| PyProjectError::Parse { path, source }),
            })
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
