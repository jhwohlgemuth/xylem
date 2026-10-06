#![cfg_attr(not(test), deny(clippy::exit, clippy::expect_used, clippy::panic, clippy::unwrap_used))]
//! Xylem Library
//! > Digital xylem that transports data through workflows with structure and support
use crate::error::PyProjectError;
use clap::ValueEnum;
pub use config::*;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;
use std::path::PathBuf;

pub mod completion;
// These legacy API modules predate the workspace documentation gate. Keep the
// exception at module boundaries so new top-level APIs remain denied by default.
#[allow(missing_docs)]
pub mod config;
pub mod container;
#[allow(missing_docs)]
pub mod error;
pub mod execution;
pub mod gpu;
#[allow(missing_docs)]
pub mod journal;
pub mod outcome;
pub mod plan;
pub mod progress;
pub mod projection;
#[allow(missing_docs)]
pub mod remote;
pub mod run;
pub mod sandbox;
#[allow(missing_docs)]
pub mod script;
pub mod selection;
#[allow(missing_docs)]
pub mod util;
#[derive(ValueEnum, Copy, Clone, Debug, Display, PartialEq, Eq)]
/// Output format for a completed run or dry-run plan.
pub enum ReporterType {
    /// Human-readable terminal output.
    Console,
    /// Machine-readable JSON output.
    Json,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
/// Relevant fields read from a Python `pyproject.toml` document.
pub struct PyProjectToml {
    #[serde(flatten)]
    inner: pyproject_toml::PyProjectToml,
    tool: Option<PyProjectTool>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
/// Tool-specific section of a Python project manifest.
pub struct PyProjectTool {
    xylem: Option<ToolXylem>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
/// Xylem settings embedded in a Python project manifest.
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
    /// Read and parse a Python project manifest.
    pub fn read(path: PathBuf) -> Result<Self, PyProjectError> {
        read_to_string(&path)
            .map_err(|source| PyProjectError::Read { path: path.clone(), source })
            .and_then(|content| match content.trim().is_empty() {
                | true => Err(PyProjectError::Empty { path }),
                | false => toml::from_str(&content).map_err(|source| PyProjectError::Parse { path, source }),
            })
    }
    /// Return the configured Xylem module name, if present.
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
