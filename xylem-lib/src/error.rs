//! Owned errors returned by reusable Xylem operations
use core::fmt;
use std::io;
use std::path::PathBuf;

/// Failure while preparing modules for execution.
#[derive(Debug)]
pub enum AcquisitionError {
    /// A module could not be acquired or prepared.
    Module { module: String, source: io::Error },
    /// The configured remote-host policy is invalid.
    Policy { source: io::Error },
}
/// Failure while resolving a configured module into an executable script.
#[derive(Debug)]
pub enum ModuleLoadError {
    /// A module type cannot be executed.
    InvalidType { module: String },
    /// A module URI cannot be resolved to a working directory.
    InvalidUri { module: String, uri: String },
    /// A template path could not be inspected.
    Inspect { module: String, path: PathBuf, source: io::Error },
    /// A template could not be loaded.
    Template { module: String, source: TemplateError },
}
/// Failure while executing a child process.
#[derive(Debug)]
pub enum ProcessError {
    /// The child could not be created or observed.
    Spawn {
        script: String,
        working_directory: PathBuf,
        source: io::Error,
    },
    /// The child completed unsuccessfully.
    Unsuccessful {
        script: String,
        working_directory: PathBuf,
        status: String,
        stderr: String,
    },
}
/// Failure while loading a Python project manifest.
#[derive(Debug)]
pub enum PyProjectError {
    /// The manifest is empty.
    Empty { path: PathBuf },
    /// The manifest could not be parsed.
    Parse { path: PathBuf, source: toml::de::Error },
    /// The manifest could not be read.
    Read { path: PathBuf, source: io::Error },
}
/// Failure while reading or parsing a module command template.
#[derive(Debug)]
pub enum TemplateError {
    /// The template is not valid JSON.
    Parse { path: PathBuf, source: serde_json::Error },
    /// The template could not be read.
    Read { path: PathBuf, source: io::Error },
}
/// Failure while checking a declared host requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementError {
    /// Executable name that was not found.
    pub command: String,
}
impl core::error::Error for AcquisitionError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Module { source, .. } | Self::Policy { source } => Some(source),
        }
    }
}
impl fmt::Display for AcquisitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Module { module, source } => write!(formatter, "prepare module {module}: {source}"),
            | Self::Policy { source } => write!(formatter, "configure remote module policy: {source}"),
        }
    }
}
impl core::error::Error for ModuleLoadError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Inspect { source, .. } => Some(source),
            | Self::Template { source, .. } => Some(source),
            | Self::InvalidType { .. } | Self::InvalidUri { .. } => None,
        }
    }
}
impl fmt::Display for ModuleLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Inspect { module, path, source } => {
                write!(formatter, "module {module} cannot inspect template {}: {source}", path.display())
            }
            | Self::InvalidType { module } => write!(formatter, "module {module} uses reserved configuration module type"),
            | Self::InvalidUri { module, uri } => write!(formatter, "module {module} has invalid URI: {uri}"),
            | Self::Template { module, source } => write!(formatter, "module {module}: {source}"),
        }
    }
}
impl core::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Spawn { source, .. } => Some(source),
            | Self::Unsuccessful { .. } => None,
        }
    }
}
impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Spawn {
                script,
                working_directory,
                source,
            } => write!(formatter, "run {script} in {}: {source}", working_directory.display()),
            | Self::Unsuccessful {
                script,
                working_directory,
                status,
                stderr,
            } => match stderr.is_empty() {
                | true => write!(
                    formatter,
                    "run {script} in {}: process exited with status {status}",
                    working_directory.display()
                ),
                | false => write!(
                    formatter,
                    "run {script} in {}: process exited with status {status}: {stderr}",
                    working_directory.display()
                ),
            },
        }
    }
}
impl core::error::Error for PyProjectError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Empty { .. } => None,
            | Self::Parse { source, .. } => Some(source),
            | Self::Read { source, .. } => Some(source),
        }
    }
}
impl fmt::Display for PyProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Empty { path } => write!(formatter, "Python project manifest {} is empty", path.display()),
            | Self::Parse { path, source } => write!(formatter, "parse Python project manifest {}: {source}", path.display()),
            | Self::Read { path, source } => write!(formatter, "read Python project manifest {}: {source}", path.display()),
        }
    }
}
impl core::error::Error for RequirementError {}
impl fmt::Display for RequirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "required command was not found: {}", self.command)
    }
}
impl core::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Parse { source, .. } => Some(source),
            | Self::Read { source, .. } => Some(source),
        }
    }
}
impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Parse { path, source } => write!(formatter, "parse module template {}: {source}", path.display()),
            | Self::Read { path, source } => write!(formatter, "read module template {}: {source}", path.display()),
        }
    }
}
