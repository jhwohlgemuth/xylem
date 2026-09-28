//! Structured module and workflow execution outcomes
use crate::journal::ExecutionState;
use bon::Builder;
use serde::{Deserialize, Serialize};

/// Result of executing one configured module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleOutcome {
    /// Deterministic module position.
    pub index: usize,
    /// Configured module name.
    pub name: String,
    /// Stable ancestry from the root workflow to the executable leaf.
    pub module_path: Vec<String>,
    /// Terminal module state.
    pub state: ExecutionState,
    /// Failure detail, when present.
    pub error: Option<String>,
}
/// Result of executing one workflow input.
#[derive(Builder, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[builder(start_fn(name = init))]
pub struct WorkflowOutcome {
    /// User-facing input value.
    pub input: Option<String>,
    /// Terminal input state.
    #[builder(default = ExecutionState::Succeeded)]
    pub state: ExecutionState,
    /// Ordered module outcomes that reached a terminal state.
    #[builder(default)]
    pub modules: Vec<ModuleOutcome>,
    /// Failure detail, when present.
    pub error: Option<String>,
}
impl WorkflowOutcome {
    /// Build a failed workflow outcome before any module reaches a terminal state.
    pub fn failed(input: Option<String>, error: impl Into<String>) -> Self {
        Self::init().maybe_input(input).state(ExecutionState::Failed).error(error.into()).build()
    }
    /// Whether all required work for this input succeeded or was intentionally skipped.
    pub fn is_success(&self) -> bool {
        self.state.is_complete()
    }
}
