//! Owned errors returned by reusable Xylem operations
use core::fmt;

/// Failure while executing a child process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessError {
    /// The child could not be created or observed.
    Spawn { script: String, source: String },
    /// The child completed unsuccessfully.
    Unsuccessful { script: String, status: String },
}
impl core::error::Error for ProcessError {}
impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Spawn { script, source } => write!(formatter, "run {script}: {source}"),
            | Self::Unsuccessful { script, status } => write!(formatter, "run {script}: process exited with status {status}"),
        }
    }
}
