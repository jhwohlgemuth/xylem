//! Progress events emitted by module preparation and workflow execution.

/// A workflow phase that can report progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressPhase {
    /// Downloading, cloning, and validating modules.
    Preparation,
    /// Running a module for a workflow input.
    Execution,
}
impl ProgressPhase {
    /// Emit a progress update for this phase when an observer is present.
    pub fn emit(self, observer: Option<&dyn ProgressObserver>, state: ProgressState, module: &str, input: Option<&str>) {
        if let Some(observer) = observer {
            observer.update(ProgressUpdate {
                phase: self,
                module: module.to_string(),
                input: input.map(String::from),
                state,
            });
        }
    }
}
/// The state of one progress item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressState {
    /// Work on the item has started.
    Started,
    /// Work on the item completed successfully.
    Finished,
}
/// A progress update for one module operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressUpdate {
    /// Phase producing this update.
    pub phase: ProgressPhase,
    /// Module name.
    pub module: String,
    /// Active workflow input, when execution is input-scoped.
    pub input: Option<String>,
    /// Current state of the item.
    pub state: ProgressState,
}
/// Receives progress updates without coupling the library to a terminal UI.
pub trait ProgressObserver: Send + Sync {
    /// Observe a progress update.
    fn update(&self, update: ProgressUpdate);
}
