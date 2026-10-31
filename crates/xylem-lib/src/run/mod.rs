//! Shared workflow cancellation primitives.
extern crate alloc;

use crate::config::{Config, FailurePolicy};
use crate::execution::{ExecutionLimits, InputBatch};
use crate::script::Module;
use crate::workspace::{InputWorkspace, RunWorkspace, WorkspaceError};
use acorn_cmd::CancellationToken;
use acorn_host::fs::SafePath;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use bon::Builder;
use core::sync::atomic::{AtomicU8, Ordering};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Canonical durable run bundles and history discovery.
pub mod store;
/// Lightweight host and process-tree sampling.
pub mod telemetry;

/// Failure while constructing an immutable run or input context.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// The durable journal identity was assigned more than once.
    #[error("run journal path is already bound")]
    JournalAlreadyBound,
    /// A path required as a template or environment value is not valid Unicode.
    #[error("{field} path must be valid UTF-8: {}", path.display())]
    NonUnicodePath {
        /// Context field containing the rejected path.
        field: &'static str,
        /// Rejected path.
        path: PathBuf,
    },
    /// An input workspace could not be created.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}
/// Shared two-stage cancellation handle.
#[derive(Clone, Debug, Default)]
pub struct RunCancellation {
    requests: Arc<AtomicU8>,
    token: CancellationToken,
}
/// Immutable policy and owned filesystem state for one workflow run.
#[derive(Builder, Debug)]
#[builder(derive(Debug), start_fn(name = init))]
pub struct RunContext {
    #[builder(with = |path: PathBuf| -> Result<_, ContextError> { validate_path("artifact output", path) })]
    artifact_output: Option<PathBuf>,
    batch: Option<InputBatch>,
    #[builder(default)]
    cancellation: RunCancellation,
    #[builder(with = |path: PathBuf| -> Result<_, ContextError> { validate_path("configuration directory", path) })]
    configuration_directory: PathBuf,
    #[builder(default)]
    failure_policy: FailurePolicy,
    #[builder(with = |path: PathBuf| -> Result<_, ContextError> { validate_path("invocation directory", path) })]
    invocation_directory: PathBuf,
    #[builder(skip)]
    journal_path: OnceLock<PathBuf>,
    limits: ExecutionLimits,
    #[builder(default)]
    retain_workspace: bool,
    #[builder(with = |run_id: String| -> Result<_, ContextError> { validate_run_id(run_id) })]
    run_id: SafePath,
    #[builder(with = |workspace: RunWorkspace| -> Result<_, ContextError> { validate_workspace(workspace) })]
    workspace: RunWorkspace,
}
/// Immutable variables and owned paths for one configured input position.
#[derive(Debug)]
pub struct WorkflowContext<'run> {
    input: Option<String>,
    input_index: usize,
    run: &'run RunContext,
    workspace: InputWorkspace,
}

impl RunCancellation {
    /// Force managed child termination immediately.
    pub fn force(&self) {
        self.requests.store(2, Ordering::Release);
        self.token.cancel();
    }
    /// Return whether graceful cancellation was requested.
    pub fn is_requested(&self) -> bool {
        self.requests.load(Ordering::Acquire) > 0
    }
    /// Request graceful cancellation first and force termination on repetition.
    pub fn request(&self) {
        if self.requests.fetch_add(1, Ordering::AcqRel) > 0 {
            self.token.cancel();
        }
    }
    /// Borrow the ACORN token used by active managed commands.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}
impl RunContext {
    /// Return the CLI artifact override, when present.
    pub fn artifact_output(&self) -> Option<&Path> {
        self.artifact_output.as_deref()
    }
    /// Return the configured input batch ceiling.
    pub fn batch(&self) -> Option<InputBatch> {
        self.batch
    }
    /// Borrow the shared cancellation handle.
    pub fn cancellation(&self) -> &RunCancellation {
        &self.cancellation
    }
    /// Remove the owned workspace immediately.
    pub fn close(self) -> Result<(), WorkspaceError> {
        self.workspace.close()
    }
    /// Return the directory containing the selected workflow configuration.
    pub fn configuration_directory(&self) -> &Path {
        &self.configuration_directory
    }
    /// Return the effective workflow failure policy.
    pub fn failure_policy(&self) -> FailurePolicy {
        self.failure_policy
    }
    /// Return the directory in which the CLI was invoked.
    pub fn invocation_directory(&self) -> &Path {
        &self.invocation_directory
    }
    /// Return the durable journal path after the run store is opened.
    pub fn journal_path(&self) -> Option<&Path> {
        self.journal_path.get().map(PathBuf::as_path)
    }
    /// Return the resolved worker limit.
    pub fn limits(&self) -> ExecutionLimits {
        self.limits
    }
    /// Return whether the disposable workspace should be retained after execution.
    pub fn retain_workspace(&self) -> bool {
        self.retain_workspace
    }
    /// Keep the owned workspace and return its path.
    pub fn retain(self) -> PathBuf {
        self.workspace.retain()
    }
    /// Return the stable run identifier.
    pub fn run_id(&self) -> &str {
        self.run_id.as_path().to_str().unwrap_or_default()
    }
    /// Bind the durable journal path after opening the run store.
    /// ### Errors
    /// Returns [`ContextError::NonUnicodePath`] when the path cannot be retained losslessly, or [`ContextError::JournalAlreadyBound`] on repeated binding.
    pub fn bind_journal_path(&self, path: PathBuf) -> Result<(), ContextError> {
        require_unicode_path("journal", &path).and_then(|()| self.journal_path.set(path).map_err(|_| ContextError::JournalAlreadyBound))
    }
    /// Borrow the owned run workspace.
    pub fn workspace(&self) -> &RunWorkspace {
        &self.workspace
    }
    /// Construct one input context with its own workspace directories.
    pub fn workflow_context(&self, input_index: usize, input: Option<String>) -> Result<WorkflowContext<'_>, ContextError> {
        self.workspace
            .input(input_index)
            .map_err(ContextError::from)
            .map(|workspace| WorkflowContext {
                input,
                input_index,
                run: self,
                workspace,
            })
    }
}
impl WorkflowContext<'_> {
    /// Build the namespaced environment for one module command.
    pub fn command_environment(&self, config: &Config, module: &Module) -> Result<BTreeMap<String, String>, ContextError> {
        self.variables(config, Some(module))
            .map(|variables| variables.into_iter().map(|(key, value)| (format!("_XYLEM_{key}"), value)).collect())
    }
    /// Return the configured input value, when this is not a zero-input workflow.
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }
    /// Return the stable configured input position.
    pub fn input_index(&self) -> usize {
        self.input_index
    }
    /// Resolve ordered, unprefixed variables for this input and optional module.
    pub fn variables(&self, config: &Config, module: Option<&Module>) -> Result<BTreeMap<String, String>, ContextError> {
        config.resolve_context_variables(self, module)
    }
    /// Replace the command-visible input while preserving the owned input boundary.
    pub fn with_input(mut self, input: Option<String>) -> Self {
        self.input = input;
        self
    }
    /// Borrow the input-owned filesystem paths.
    pub fn workspace(&self) -> &InputWorkspace {
        &self.workspace
    }
    pub(crate) fn run(&self) -> &RunContext {
        self.run
    }
}
fn require_unicode_path(field: &'static str, path: &Path) -> Result<(), ContextError> {
    path.to_str().map(|_| ()).ok_or_else(|| ContextError::NonUnicodePath {
        field,
        path: path.to_path_buf(),
    })
}
fn validate_path(field: &'static str, path: PathBuf) -> Result<PathBuf, ContextError> {
    require_unicode_path(field, &path).map(|()| path)
}
fn validate_run_id(run_id: String) -> Result<SafePath, ContextError> {
    SafePath::new(&run_id)
        .map_err(|_| WorkspaceError::RunId(run_id.clone()))
        .and_then(|path| match path.as_path().components().count() {
            | 1 => Ok(path),
            | _ => Err(WorkspaceError::RunId(run_id)),
        })
        .map_err(ContextError::from)
}
fn validate_workspace(workspace: RunWorkspace) -> Result<RunWorkspace, ContextError> {
    require_unicode_path("workspace", workspace.root()).map(|()| workspace)
}

#[cfg(test)]
mod tests;
