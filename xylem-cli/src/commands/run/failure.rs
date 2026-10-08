use core::fmt;
use xylem_lib::gpu::GpuMonitorError;
use xylem_lib::journal::FinalStatus;

/// Failure returned to the executable boundary with a stable exit code.
#[derive(Debug)]
pub struct RunFailure {
    exit_code: i32,
    message: String,
}
impl From<&RunFailure> for FinalStatus {
    fn from(_: &RunFailure) -> Self {
        Self::Failed
    }
}
impl From<RunFailure> for FinalStatus {
    fn from(_: RunFailure) -> Self {
        Self::Failed
    }
}
impl core::error::Error for RunFailure {}
impl fmt::Display for RunFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}
impl From<GpuMonitorError> for RunFailure {
    fn from(error: GpuMonitorError) -> Self {
        match error {
            | GpuMonitorError::Journal(error) => Self::new(exitcode::IOERR, error.to_string()),
            | GpuMonitorError::Runtime(message) => Self::new(exitcode::UNAVAILABLE, message),
            | GpuMonitorError::Thread(message) => Self::new(exitcode::OSERR, message),
        }
    }
}
impl RunFailure {
    pub(super) fn new(exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
    pub(super) fn with_secondary(mut self, message: impl fmt::Display) -> Self {
        self.message = format!("{}; additionally, {message}", self.message);
        self
    }
    /// Return the process exit code owned by the CLI boundary.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}
