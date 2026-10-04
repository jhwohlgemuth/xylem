//! Progress events emitted by module preparation and workflow execution.
use crate::journal::ExecutionState;

/// Receives progress updates without coupling the library to a terminal UI.
pub trait ProgressObserver: Send + Sync {
    /// Observe a progress update.
    fn update(&self, update: ProgressUpdate);
}

/// A workflow phase that can report progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressPhase {
    /// Downloading, cloning, and validating modules.
    Preparation,
    /// Running a module for a workflow input.
    Execution,
}

/// The state of one progress item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressState {
    /// Work on the item was cancelled.
    Cancelled,
    /// Work on the item has started.
    Started,
    /// Work on the item completed successfully.
    Finished,
    /// Work on the item failed.
    Failed,
    /// Work on the item was intentionally skipped.
    Skipped,
    /// Work on the item was skipped because declared outputs already exist.
    SkippedExisting,
}

/// A progress update for one module operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressUpdate {
    /// Phase producing this update.
    pub phase: ProgressPhase,
    /// Module name.
    pub module: String,
    /// Stable ancestry from the root workflow to the executable leaf.
    pub module_path: Vec<String>,
    /// Active workflow input, when execution is input-scoped.
    pub input: Option<String>,
    /// Current state of the item.
    pub state: ProgressState,
}

impl ProgressPhase {
    /// Emit a progress update for this phase when an observer is present.
    pub fn emit(self, state: ProgressState, module: &str, module_path: &[String], observer: Option<&dyn ProgressObserver>, input: Option<&str>) {
        if let Some(observer) = observer {
            observer.update(ProgressUpdate {
                phase: self,
                module: module.to_string(),
                module_path: module_path.to_vec(),
                input: input.map(String::from),
                state,
            });
        }
    }
}

impl From<ExecutionState> for ProgressState {
    fn from(state: ExecutionState) -> Self {
        match state {
            | ExecutionState::NotStarted | ExecutionState::Pending | ExecutionState::Running => Self::Started,
            | ExecutionState::Cancelled => Self::Cancelled,
            | ExecutionState::Succeeded => Self::Finished,
            | ExecutionState::Failed => Self::Failed,
            | ExecutionState::Skipped => Self::Skipped,
            | ExecutionState::SkippedExisting => Self::SkippedExisting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_states_convert_to_progress_states() {
        assert_eq!(ProgressState::from(ExecutionState::NotStarted), ProgressState::Started);
        assert_eq!(ProgressState::from(ExecutionState::Pending), ProgressState::Started);
        assert_eq!(ProgressState::from(ExecutionState::Running), ProgressState::Started);
        assert_eq!(ProgressState::from(ExecutionState::Cancelled), ProgressState::Cancelled);
        assert_eq!(ProgressState::from(ExecutionState::Succeeded), ProgressState::Finished);
        assert_eq!(ProgressState::from(ExecutionState::Failed), ProgressState::Failed);
        assert_eq!(ProgressState::from(ExecutionState::Skipped), ProgressState::Skipped);
        assert_eq!(ProgressState::from(ExecutionState::SkippedExisting), ProgressState::SkippedExisting);
    }
}
