//! Bounded workflow input discovery and scripted selection.
use acorn_schema::hardware::Memory;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

mod cel;

trait ScriptingEngine {
    /// Interpreter-owned representation of a checked script.
    type CompiledScript;
    /// Immutable candidate data exposed to compilation and evaluation.
    type Context;

    /// Parse, check, and deterministically bound a script against its available bindings.
    fn compile(
        &self,
        script: &str,
        context: &Self::Context,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Self::CompiledScript, SelectionError>;
    /// Evaluate a compiled script without granting external side-effect capabilities.
    fn evaluate(
        &self,
        script: &Self::CompiledScript,
        context: &Self::Context,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Vec<Option<String>>, SelectionError>;
    /// Report the language and interpreter identity recorded in plan provenance.
    fn metadata(&self) -> ScriptingEngineMetadata;
}
/// Provenance for the values selected for one workflow invocation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputSource {
    /// Explicit command-line values.
    Cli,
    /// Immediate children of a configured directory.
    Directory,
    /// A configured ordered list.
    List,
    /// A zero-input workflow.
    None,
    /// A bounded scripted selection.
    Selection {
        /// Number of qualified candidate records.
        candidate_count: usize,
        /// Selected engine.
        engine: SelectionEngine,
        /// Stable digest of the qualified inventory.
        inventory_digest: String,
        /// Selection language.
        language: String,
        /// Supported language-subset version.
        language_version: String,
        /// Effective fixed resource limits.
        limits: Box<SelectionLimits>,
        /// Interpreter identity.
        interpreter: String,
        /// Pinned interpreter version.
        interpreter_version: String,
        /// Canonical discovery options.
        options: ResolvedSelectionOptions,
        /// Digest of the exact configured script bytes.
        script_digest: String,
        /// Entries excluded because they were symlinks or special files.
        skipped_unsupported_entries: usize,
    },
}
/// CEL is the initial closed selection-engine set.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionEngine {
    /// [Common Expression Language](https://cel.dev/).
    Cel,
}
/// Failure while validating, discovering, compiling, or evaluating a selection.
#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    /// A binding could not be represented by the interpreter.
    #[error("input.select bindings: {0}")]
    Binding(String),
    /// The expression uses an unsupported declaration or construct.
    #[error("input.select.script check: {0}")]
    Check(String),
    /// A filesystem operation failed during discovery.
    #[error("input.select discovery at {}: {source}", path.display())]
    Discovery {
        /// Path being inspected.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// CEL evaluation failed.
    #[error("input.select.script evaluation: {0}")]
    Evaluation(String),
    /// A deterministic resource limit was exceeded.
    #[error("input.select limit {resource} exceeded: {actual} > {limit}")]
    Limit {
        /// Bounded resource.
        resource: &'static str,
        /// Configured fixed ceiling.
        limit: u64,
        /// Observed or estimated amount.
        actual: u64,
    },
    /// The expression could not be parsed.
    #[error("input.select.script parse: {0}")]
    Parse(String),
    /// The request shape is semantically invalid.
    #[error("input.select: {0}")]
    Request(String),
    /// The expression returned an invalid value.
    #[error("input.select result: {0}")]
    Result(String),
    /// A returned path was not part of the inventory.
    #[error("input.select result contains unknown candidate: {0}")]
    UnknownCandidate(String),
}
/// Object wrapper used by the untagged workflow input syntax.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputSelection {
    /// Structured selection request.
    pub select: SelectionRequest,
}
/// Ordered normalized inputs and their planning provenance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedInputs {
    /// Input source and selection evidence.
    pub source: InputSource,
    /// Ordered input values, with `None` representing a zero-input workflow.
    pub values: Vec<Option<String>>,
}
/// Normalized discovery options recorded in plans and exposed to CEL.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedSelectionOptions {
    /// Effective recursion policy.
    pub recursive: bool,
    /// Canonical UTF-8 roots in configured order.
    pub roots: Vec<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScriptingEngineMetadata {
    interpreter: &'static str,
    interpreter_version: &'static str,
    language: &'static str,
    language_version: &'static str,
}
/// A deterministic filesystem record exposed to CEL.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectionItem {
    /// Depth beneath the configured root.
    pub depth: usize,
    /// Final extension including `.`, or an empty string.
    pub extension: String,
    /// Zero-based configured root index.
    pub index: usize,
    /// `file` or `directory`.
    pub kind: String,
    /// Final path component.
    pub name: String,
    /// Canonical-root-based UTF-8 input path.
    pub path: String,
    /// `/`-separated path beneath the root.
    pub relative_path: String,
    /// File size in bytes, or zero for a directory.
    pub size: u64,
}
/// Fixed resource limits for discovery and CEL evaluation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SelectionLimits {
    /// Maximum parsed expression depth.
    pub ast_depth: usize,
    /// Maximum parsed expression nodes.
    pub ast_nodes: usize,
    /// Maximum candidate records.
    pub candidates: usize,
    /// Maximum estimated CEL evaluation cost.
    pub evaluation_cost: u64,
    /// Maximum intermediate or result memory.
    pub intermediate: Memory,
    /// Maximum intermediate or result records.
    pub intermediate_records: usize,
    /// Maximum metadata operations.
    pub metadata_operations: usize,
    /// Maximum aggregate candidate path memory.
    pub path: Memory,
    /// Maximum configured roots.
    pub roots: usize,
    /// Maximum script memory.
    pub script: Memory,
    /// Maximum traversal depth beneath a root.
    pub traversal_depth: usize,
    /// Maximum scalar memory in one configured variable or expanded root string.
    pub variable_scalar: Memory,
    /// Maximum configured variable depth or root variable-reference depth.
    pub variable_depth: usize,
    /// Maximum configured variable nodes or substitutions across roots.
    pub variable_nodes: usize,
    /// Maximum visited directory entries.
    pub visited_entries: usize,
}
/// Discovery options provided to a selection request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionOptions {
    /// Whether discovery descends beneath immediate root children.
    #[serde(default)]
    pub recursive: bool,
    /// Ordered local search roots, supporting workflow variable placeholders.
    pub roots: Vec<String>,
}
/// One configured selection request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRequest {
    /// Selection language engine.
    pub engine: SelectionEngine,
    /// Bounded discovery options.
    pub options: SelectionOptions,
    /// Inline selection expression.
    pub script: String,
}
impl InputSelection {
    /// Expand workflow variables in roots, then resolve inputs relative to the declaring configuration.
    ///
    /// Root expansion does not modify the typed variable bindings supplied to CEL.
    /// Unknown references, cycles, and expansion limit violations fail before discovery.
    pub fn resolve(&self, variables: Option<&Map<String, Value>>, configuration_path: &Path) -> Result<ResolvedInputs, SelectionError> {
        match self.select.engine {
            | SelectionEngine::Cel => cel::resolve(&self.select, variables, configuration_path),
        }
    }
    pub(crate) fn resolved_options(&self, variables: Option<&Map<String, Value>>) -> Result<SelectionOptions, SelectionError> {
        SelectionOptions::resolve(&self.select, variables, &SelectionLimits::default())
    }
}
impl ResolvedInputs {
    /// Construct resolved inputs for a non-selection source.
    pub fn configured(source: InputSource, values: Vec<Option<String>>) -> Self {
        Self { source, values }
    }
}
impl Default for SelectionLimits {
    fn default() -> Self {
        Self {
            ast_depth: 64,
            ast_nodes: 1_024,
            candidates: 50_000,
            evaluation_cost: 10_000_000,
            intermediate: Memory::mb(16.0),
            intermediate_records: 50_000,
            metadata_operations: 250_000,
            path: Memory::mb(16.0),
            roots: 16,
            script: Memory::kb(16.0),
            traversal_depth: 32,
            variable_depth: 32,
            variable_nodes: 10_000,
            variable_scalar: Memory::mb(1.0),
            visited_entries: 100_000,
        }
    }
}
