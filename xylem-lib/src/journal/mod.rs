//! Durable, append-only execution journals and deterministic recovery
extern crate alloc;

use crate::ReporterType;
use acorn_core::validation::ValidationError;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, remove_file, rename, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use strum::EnumIs;

/// Current on-disk journal schema version.
pub const JOURNAL_SCHEMA_VERSION: u32 = 2;
/// Maximum number of UTF-8 bytes retained for one journal error detail.
pub const MAX_ERROR_BYTES: usize = 4096;
trait Summary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<u64>, &mut u64);

    fn update_state(&mut self, next: ExecutionState, error: Option<String>, timestamp_ms: u64) {
        let (current, current_error, started_at_ms, updated_at_ms) = self.state_fields();
        *current = next;
        *current_error = error;
        if next.is_running() && started_at_ms.is_none() {
            *started_at_ms = Some(timestamp_ms);
        }
        *updated_at_ms = timestamp_ms;
    }
}
/// State of a run input or module at one point in time.
#[derive(Clone, Copy, Debug, Deserialize, EnumIs, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    /// Work is known but has not been admitted for execution.
    NotStarted,
    /// Work has been admitted but has not started doing module work.
    Pending,
    /// Work is active.
    Running,
    /// Work completed successfully.
    Succeeded,
    /// Work completed unsuccessfully.
    Failed,
    /// Work was intentionally not executed, such as during a dry run.
    Skipped,
    /// Work was not executed because declared outputs were already complete.
    SkippedExisting,
}
/// Final state written by a normally completed run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalStatus {
    /// All required work succeeded.
    Succeeded,
    /// At least one required operation failed.
    Failed,
}
/// Journal creation, persistence, validation, or recovery failure.
#[derive(Debug)]
pub enum JournalError {
    /// An operating-system I/O operation failed.
    Io(std::io::Error),
    /// JSON serialization or deserialization failed.
    Json(serde_json::Error),
    /// A journal invariant was violated.
    Invalid(String),
    /// A concurrent writer panicked while owning the journal lock.
    Poisoned,
}
/// One append-only journal event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    /// Declares the run before any work begins.
    RunStarted { metadata: RunMetadata },
    /// Records an input transition.
    InputState {
        input: JournalInput,
        state: ExecutionState,
        error: Option<String>,
    },
    /// Records a module transition for an input.
    ModuleState {
        input_index: usize,
        input_id: String,
        module_index: usize,
        module: String,
        state: ExecutionState,
        error: Option<String>,
    },
    /// Marks a normal run finalization.
    RunFinished { status: FinalStatus, error: Option<String> },
}
/// Overall state reconstructed from a journal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// The journal has a successful finalization record.
    Succeeded,
    /// The journal has a failed finalization record.
    Failed,
    /// The journal has no finalization record.
    Interrupted,
}
/// Stable configuration identity recorded at run start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigurationIdentity {
    /// Configuration path used by the command.
    pub path: String,
    /// Optional workflow identifier declared by the configuration.
    pub id: Option<String>,
    /// Optional workflow version declared by the configuration.
    pub version: Option<String>,
}
/// Latest reconstructed input state and its ordered modules.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InputSummary {
    /// Deterministic zero-based input position.
    pub index: usize,
    /// Stable input identity within the run.
    pub input_id: String,
    /// User-facing input value.
    pub value: Option<String>,
    /// Latest durable state.
    pub state: ExecutionState,
    /// Bounded failure detail, when present.
    pub error: Option<String>,
    /// First running timestamp, when execution began.
    pub started_at_ms: Option<u64>,
    /// Timestamp of the latest transition.
    pub updated_at_ms: u64,
    /// Ordered module states.
    pub modules: Vec<ModuleSummary>,
}
/// Stable identity and display value for one workflow input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalInput {
    /// Deterministic zero-based input position.
    pub index: usize,
    /// Stable identity within this run.
    pub input_id: String,
    /// User-facing input value. `None` represents a zero-input workflow.
    pub value: Option<String>,
}
/// One versioned line in a journal file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalRecord {
    /// Schema version for this record.
    pub schema_version: u32,
    /// Monotonically increasing sequence number, beginning at zero.
    pub sequence: u64,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Run identifier repeated on every record for independent validation.
    pub run_id: String,
    /// Recorded transition.
    #[serde(flatten)]
    pub event: JournalEvent,
}
#[derive(Debug)]
struct JournalSink {
    writer: BufWriter<File>,
    next_sequence: u64,
    input_states: Vec<ExecutionState>,
    module_states: Vec<Vec<ExecutionState>>,
    finalized: bool,
}
/// Latest reconstructed module state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleSummary {
    /// Deterministic zero-based module position.
    pub index: usize,
    /// Configured module name.
    pub name: String,
    /// Latest durable state.
    pub state: ExecutionState,
    /// Bounded failure detail, when present.
    pub error: Option<String>,
    /// First running timestamp, when execution began.
    pub started_at_ms: Option<u64>,
    /// Timestamp of the latest transition.
    pub updated_at_ms: u64,
}
/// A serialized, flush-on-transition journal writer safe for concurrent callers.
#[derive(Debug)]
pub struct RunJournal {
    path: PathBuf,
    run_id: String,
    inputs: Vec<JournalInput>,
    modules: Vec<String>,
    sink: Mutex<JournalSink>,
}
/// Run metadata written before any input begins.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunMetadata {
    /// Unique run identifier.
    pub run_id: String,
    /// Configuration used for this run.
    pub configuration: ConfigurationIdentity,
    /// Whether this is a side-effect-free dry run.
    pub dry_run: bool,
    /// Unix timestamp in milliseconds.
    pub started_at_ms: u64,
}
/// Deterministic summary reconstructed from durable journal records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunSummary {
    /// Summary schema version.
    pub schema_version: u32,
    /// Unique run identifier.
    pub run_id: String,
    /// Configuration used for this run.
    pub configuration: ConfigurationIdentity,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Run start timestamp.
    pub started_at_ms: u64,
    /// Finalization timestamp, absent for interrupted runs.
    pub finished_at_ms: Option<u64>,
    /// Elapsed run duration, absent for interrupted runs.
    pub duration_ms: Option<u64>,
    /// Overall recovered status.
    pub status: RunStatus,
    /// Final bounded run error, when present.
    pub error: Option<String>,
    /// Ordered input summaries.
    pub inputs: Vec<InputSummary>,
}
impl ExecutionState {
    /// Whether this state represents completed work.
    pub fn is_complete(self) -> bool {
        self.is_succeeded() || self.is_skipped() || self.is_skipped_existing()
    }
}
impl Summary for InputSummary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<u64>, &mut u64) {
        (&mut self.state, &mut self.error, &mut self.started_at_ms, &mut self.updated_at_ms)
    }
}
impl core::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Io(error) => Some(error),
            | Self::Json(error) => Some(error),
            | Self::Invalid(_) | Self::Poisoned => None,
        }
    }
}
impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Io(error) => write!(formatter, "journal I/O failed: {error}"),
            | Self::Json(error) => write!(formatter, "journal JSON failed: {error}"),
            | Self::Invalid(message) => write!(formatter, "invalid journal: {message}"),
            | Self::Poisoned => formatter.write_str("journal writer lock was poisoned"),
        }
    }
}
impl From<serde_json::Error> for JournalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl From<std::io::Error> for JournalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<ValidationReport> for JournalError {
    fn from(error: ValidationReport) -> Self {
        Self::Invalid(error.to_string())
    }
}
impl JournalInput {
    /// Create a deterministic input identity from its ordered position.
    pub fn new(index: usize, value: Option<String>) -> Self {
        Self {
            index,
            input_id: format!("input-{index:06}"),
            value,
        }
    }
}
impl JournalSink {
    /// Append one event with the next sequence number and durable flush.
    fn append_locked(&mut self, run_id: &str, event: JournalEvent) -> Result<(), JournalError> {
        match self.finalized {
            | true => Err(JournalError::Invalid("cannot append after run finalization".to_string())),
            | false => {
                let record = JournalRecord {
                    schema_version: JOURNAL_SCHEMA_VERSION,
                    sequence: self.next_sequence,
                    timestamp_ms: timestamp_ms(),
                    run_id: run_id.to_string(),
                    event,
                };
                serde_json::to_writer(&mut self.writer, &record)
                    .map_err(JournalError::from)
                    .and_then(|()| self.writer.write_all(b"\n").map_err(JournalError::from))
                    .and_then(|()| self.writer.flush().map_err(JournalError::from))
                    .and_then(|()| self.writer.get_ref().sync_data().map_err(JournalError::from))
                    .map(|()| self.next_sequence += 1)
            }
        }
    }
}
impl Summary for ModuleSummary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<u64>, &mut u64) {
        (&mut self.state, &mut self.error, &mut self.started_at_ms, &mut self.updated_at_ms)
    }
}
impl RunJournal {
    /// Create a new journal and durably initialize every input and module.
    ///
    /// Existing destinations are never overwritten. No execution should begin until this constructor has successfully returned.
    pub fn create(path: PathBuf, metadata: RunMetadata, inputs: Vec<JournalInput>, modules: Vec<String>) -> Result<Self, JournalError> {
        metadata
            .validate()
            .map_err(JournalError::from)
            .and_then(|()| path.parent().map_or(Ok(()), create_dir_all).map_err(JournalError::from))
            .and_then(|()| OpenOptions::new().write(true).create_new(true).open(&path).map_err(JournalError::from))
            .and_then(|file| {
                let module_states = inputs.iter().map(|_| vec![ExecutionState::NotStarted; modules.len()]).collect::<Vec<_>>();
                let journal = Self {
                    path,
                    run_id: metadata.run_id.clone(),
                    inputs: inputs.clone(),
                    modules: modules.clone(),
                    sink: Mutex::new(JournalSink {
                        writer: BufWriter::new(file),
                        next_sequence: 0,
                        input_states: vec![ExecutionState::NotStarted; inputs.len()],
                        module_states,
                        finalized: false,
                    }),
                };
                match journal.validate().map_err(JournalError::from) {
                    | Ok(()) => journal
                        .append(JournalEvent::RunStarted { metadata })
                        .and_then(|()| {
                            inputs.into_iter().try_for_each(|input| {
                                journal
                                    .append(JournalEvent::InputState {
                                        input: input.clone(),
                                        state: ExecutionState::NotStarted,
                                        error: None,
                                    })
                                    .and_then(|()| {
                                        modules.iter().enumerate().try_for_each(|(module_index, module)| {
                                            journal.append(JournalEvent::ModuleState {
                                                input_index: input.index,
                                                input_id: input.input_id.clone(),
                                                module_index,
                                                module: module.clone(),
                                                state: ExecutionState::NotStarted,
                                                error: None,
                                            })
                                        })
                                    })
                            })
                        })
                        .map(|()| journal),
                    | Err(why) => {
                        let invalid_path = journal.path.clone();
                        drop(journal);
                        let _ = remove_file(invalid_path);
                        Err(why)
                    }
                }
            })
    }
    /// Return the journal's filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Durably transition one input.
    pub fn transition_input(&self, input_index: usize, state: ExecutionState, error: Option<&str>) -> Result<(), JournalError> {
        self.inputs
            .get(input_index)
            .cloned()
            .ok_or_else(|| JournalError::Invalid(format!("unknown input index {input_index}")))
            .and_then(|input| {
                self.sink.lock().map_err(|_| JournalError::Poisoned).and_then(|mut sink| {
                    let current = sink.input_states[input_index];
                    validate_transition(current, state, "input")
                        .and_then(
                            |()| match state.is_succeeded() && sink.module_states[input_index].iter().any(|state| !state.is_complete()) {
                                | true => Err(JournalError::Invalid(format!(
                                    "input {input_index} cannot succeed while a module is unfinished or failed"
                                ))),
                                | false => Ok(()),
                            },
                        )
                        .and_then(|()| {
                            sink.append_locked(
                                &self.run_id,
                                JournalEvent::InputState {
                                    input,
                                    state,
                                    error: error.map(bounded_error),
                                },
                            )
                        })
                        .map(|()| sink.input_states[input_index] = state)
                })
            })
    }
    /// Durably transition one module for one input.
    pub fn transition_module(&self, input_index: usize, module_index: usize, state: ExecutionState, error: Option<&str>) -> Result<(), JournalError> {
        self.inputs
            .get(input_index)
            .ok_or_else(|| JournalError::Invalid(format!("unknown input index {input_index}")))
            .and_then(|input| {
                self.modules
                    .get(module_index)
                    .ok_or_else(|| JournalError::Invalid(format!("unknown module index {module_index}")))
                    .map(|module| (input, module))
            })
            .and_then(|(input, module)| {
                self.sink.lock().map_err(|_| JournalError::Poisoned).and_then(|mut sink| {
                    let current = sink.module_states[input_index][module_index];
                    validate_transition(current, state, "module")
                        .and_then(|()| {
                            sink.append_locked(
                                &self.run_id,
                                JournalEvent::ModuleState {
                                    input_index,
                                    input_id: input.input_id.clone(),
                                    module_index,
                                    module: module.clone(),
                                    state,
                                    error: error.map(bounded_error),
                                },
                            )
                        })
                        .map(|()| sink.module_states[input_index][module_index] = state)
                })
            })
    }
    /// Durably finalize the run.
    pub fn finish(&self, status: FinalStatus, error: Option<&str>) -> Result<(), JournalError> {
        self.sink.lock().map_err(|_| JournalError::Poisoned).and_then(|mut sink| {
            match (
                sink.finalized,
                status == FinalStatus::Succeeded && sink.input_states.iter().any(|state| !state.is_complete()),
            ) {
                | (true, _) => Err(JournalError::Invalid("run is already finalized".to_string())),
                | (false, true) => Err(JournalError::Invalid(
                    "a successful run cannot contain unfinished or failed inputs".to_string(),
                )),
                | (false, false) => sink
                    .append_locked(
                        &self.run_id,
                        JournalEvent::RunFinished {
                            status,
                            error: error.map(bounded_error),
                        },
                    )
                    .map(|()| sink.finalized = true),
            }
        })
    }
    /// Recover a deterministic summary from a journal file.
    ///
    /// A malformed final fragment without a trailing newline is treated as a crash-truncated record. Malformed complete records are rejected.
    pub fn read(path: impl AsRef<Path>) -> Result<RunSummary, JournalError> {
        RunSummary::try_from(path.as_ref())
    }
    fn append(&self, event: JournalEvent) -> Result<(), JournalError> {
        self.sink
            .lock()
            .map_err(|_| JournalError::Poisoned)
            .and_then(|mut sink| sink.append_locked(&self.run_id, event))
    }
}
impl Validate for RunJournal {
    fn validate(&self) -> Result<(), ValidationReport> {
        let report = match self.run_id.is_empty() {
            | true => ValidationReport::from_error("run_id", ValidationError::new("nonempty").with_message("Run ID cannot be empty")),
            | false => ValidationReport::new(),
        };
        self.inputs
            .iter()
            .enumerate()
            .fold((report, BTreeSet::new()), |(mut report, mut input_ids), (expected_index, input)| {
                if input.index != expected_index {
                    report.add(
                        format!("inputs[{expected_index}].index"),
                        ValidationError::new("sequence").with_message(format!("Expected input index {expected_index}, found {}", input.index)),
                    );
                }
                match (input.input_id.is_empty(), input_ids.insert(input.input_id.as_str())) {
                    | (true, _) => report.add(
                        format!("inputs[{expected_index}].input_id"),
                        ValidationError::new("nonempty").with_message("Input ID cannot be empty"),
                    ),
                    | (false, false) => report.add(
                        format!("inputs[{expected_index}].input_id"),
                        ValidationError::new("unique").with_message(format!("Duplicate input ID {}", input.input_id)),
                    ),
                    | (false, true) => (),
                }
                (report, input_ids)
            })
            .0
            .finish()
    }
}
impl RunMetadata {
    /// Build metadata with the current system time.
    pub fn now(run_id: String, configuration: ConfigurationIdentity, dry_run: bool) -> Self {
        Self {
            run_id,
            configuration,
            dry_run,
            started_at_ms: timestamp_ms(),
        }
    }
}
impl Validate for RunMetadata {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self.run_id.is_empty() {
            | true => Err(ValidationReport::from_error(
                "run_id",
                ValidationError::new("nonempty").with_message("Run ID cannot be empty"),
            )),
            | false => Ok(()),
        }
    }
}
impl fmt::Display for RunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::Succeeded => "succeeded",
            | Self::Failed => "failed",
            | Self::Interrupted => "interrupted",
        })
    }
}
impl RunSummary {
    /// Persist this summary as formatted JSON without overwriting an existing report.
    pub fn write_json_atomic(&self, directory: &Path) -> Result<PathBuf, JournalError> {
        create_dir_all(directory).map_err(JournalError::from).and_then(|()| {
            let destination = directory.join(format!("{}.json", self.run_id));
            match destination.exists() {
                | true => Err(JournalError::Invalid(format!(
                    "refusing to overwrite reporter output {}",
                    destination.display()
                ))),
                | false => {
                    let temporary = directory.join(format!(".{}.json.tmp", self.run_id));
                    let result = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temporary)
                        .map_err(JournalError::from)
                        .and_then(|mut file| {
                            serde_json::to_writer_pretty(&mut file, self)
                                .map_err(JournalError::from)
                                .and_then(|()| file.write_all(b"\n").map_err(JournalError::from))
                                .and_then(|()| file.flush().map_err(JournalError::from))
                                .and_then(|()| file.sync_all().map_err(JournalError::from))
                        })
                        .and_then(|()| rename(&temporary, &destination).map_err(JournalError::from));
                    match result {
                        | Ok(()) => Ok(destination),
                        | Err(why) => {
                            let _ = remove_file(&temporary);
                            Err(why)
                        }
                    }
                }
            }
        })
    }
    /// Render selected reporters in the order requested by the user.
    pub fn render(&self, reporters: &[ReporterType], output_directory: Option<&Path>, journal_path: &Path) -> Result<Vec<PathBuf>, JournalError> {
        let mut files = Vec::new();
        reporters
            .iter()
            .try_for_each(|reporter| match reporter {
                | ReporterType::Console => self.render_console(journal_path),
                | ReporterType::Json => match output_directory {
                    | Some(directory) => self.write_json_atomic(directory).map(|path| files.push(path)),
                    | None => self.render_json_stdout(),
                },
            })
            .map(|()| files)
    }
    /// Render the human-readable console summary to standard output.
    fn render_console(&self, journal_path: &Path) -> Result<(), JournalError> {
        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut skipped_existing = 0usize;
        let mut unfinished = 0usize;
        for input in &self.inputs {
            match input.state {
                | ExecutionState::Succeeded => succeeded += 1,
                | ExecutionState::Failed => failed += 1,
                | ExecutionState::Skipped => skipped += 1,
                | ExecutionState::SkippedExisting => skipped_existing += 1,
                | ExecutionState::NotStarted | ExecutionState::Pending | ExecutionState::Running => unfinished += 1,
            }
        }
        let stdout = io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "Run {}: {}", self.run_id, self.status)
            .and_then(|()| {
                writeln!(
                    output,
                    "Inputs: {succeeded} succeeded, {failed} failed, {skipped} skipped, {skipped_existing} skipped existing, {unfinished} unfinished"
                )
            })
            .and_then(|()| {
                self.inputs.iter().filter(|input| input.state.is_failed()).try_for_each(|input| {
                    let name = input.value.as_deref().unwrap_or("<no input>");
                    writeln!(output, "Failed: {name}: {}", input.error.as_deref().unwrap_or("module execution failed"))
                })
            })
            .and_then(|()| writeln!(output, "Journal: {}", journal_path.display()))
            .and_then(|()| output.flush())
            .map_err(JournalError::from)
    }
    /// Render this summary as formatted JSON to standard output.
    fn render_json_stdout(&self) -> Result<(), JournalError> {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        serde_json::to_writer_pretty(&mut output, self)
            .map_err(JournalError::from)
            .and_then(|()| output.write_all(b"\n").map_err(JournalError::from))
            .and_then(|()| output.flush().map_err(JournalError::from))
    }
}
impl TryFrom<&Path> for RunSummary {
    type Error = JournalError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        File::open(path).map_err(JournalError::from).and_then(|file| {
            let mut reader = BufReader::new(file);
            let mut records = Vec::new();
            let mut line_number = 0usize;
            let mut reading = true;
            let mut result = Ok(());
            while reading && result.is_ok() {
                let mut bytes = Vec::new();
                match reader.read_until(b'\n', &mut bytes) {
                    | Ok(0) => reading = false,
                    | Ok(_) => {
                        line_number += 1;
                        let complete = bytes.last() == Some(&b'\n');
                        if complete {
                            bytes.pop();
                            if bytes.last() == Some(&b'\r') {
                                bytes.pop();
                            }
                        }
                        match serde_json::from_slice::<JournalRecord>(&bytes) {
                            | Ok(record) => records.push(record),
                            | Err(_) if !complete => reading = false,
                            | Err(error) => {
                                result = Err(JournalError::Invalid(format!("record {line_number} is not valid JSON: {error}")));
                            }
                        }
                    }
                    | Err(why) => result = Err(JournalError::from(why)),
                }
            }
            result.and_then(|()| <Self as TryFrom<Vec<JournalRecord>>>::try_from(records))
        })
    }
}
impl TryFrom<PathBuf> for RunSummary {
    type Error = JournalError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}
impl TryFrom<Vec<JournalRecord>> for RunSummary {
    type Error = JournalError;

    fn try_from(records: Vec<JournalRecord>) -> Result<Self, Self::Error> {
        let mut metadata = None;
        let mut inputs = BTreeMap::<usize, InputSummary>::new();
        let mut finalization = None;
        let mut run_id = None::<String>;
        records
            .into_iter()
            .enumerate()
            .try_for_each(|(expected_sequence, record)| {
                let JournalRecord {
                    schema_version,
                    sequence,
                    timestamp_ms,
                    run_id: record_run_id,
                    event,
                } = record;
                let header = match (schema_version == JOURNAL_SCHEMA_VERSION, sequence == expected_sequence as u64) {
                    | (false, _) => Err(JournalError::Invalid(format!("unsupported schema version {schema_version}"))),
                    | (true, false) => Err(JournalError::Invalid(format!("expected sequence {expected_sequence}, found {sequence}"))),
                    | (true, true) => Ok(()),
                };
                header
                    .and_then(|()| match run_id.as_deref() {
                        | Some(expected) if expected != record_run_id.as_str() => {
                            Err(JournalError::Invalid("record run IDs do not match".to_string()))
                        }
                        | None => {
                            run_id = Some(record_run_id.clone());
                            Ok(())
                        }
                        | Some(_) => Ok(()),
                    })
                    .and_then(|()| match finalization.is_some() {
                        | true => Err(JournalError::Invalid("records follow run finalization".to_string())),
                        | false => Ok(()),
                    })
                    .and_then(|()| match event {
                        | JournalEvent::RunStarted { metadata: value } => {
                            match metadata.is_some() || expected_sequence != 0 || value.run_id != record_run_id {
                                | true => Err(JournalError::Invalid("run_started must be the first and only start record".to_string())),
                                | false => {
                                    metadata = Some(value);
                                    Ok(())
                                }
                            }
                        }
                        | JournalEvent::InputState { input, state, error } => {
                            let initialized_inputs = inputs.len();
                            match inputs.get_mut(&input.index) {
                                | Some(summary) => match summary.input_id != input.input_id || summary.value != input.value {
                                    | true => Err(JournalError::Invalid(format!("input identity changed at index {}", input.index))),
                                    | false => {
                                        validate_transition(summary.state, state, "input").map(|()| summary.update_state(state, error, timestamp_ms))
                                    }
                                },
                                | None if state.is_not_started() && input.index == initialized_inputs => {
                                    inputs.insert(
                                        input.index,
                                        InputSummary {
                                            index: input.index,
                                            input_id: input.input_id,
                                            value: input.value,
                                            state,
                                            error,
                                            started_at_ms: None,
                                            updated_at_ms: timestamp_ms,
                                            modules: Vec::new(),
                                        },
                                    );
                                    Ok(())
                                }
                                | None => Err(JournalError::Invalid(format!("input {} was not initialized in order", input.index))),
                            }
                        }
                        | JournalEvent::ModuleState {
                            input_index,
                            input_id,
                            module_index,
                            module,
                            state,
                            error,
                        } => inputs
                            .get_mut(&input_index)
                            .ok_or_else(|| JournalError::Invalid(format!("module references unknown input {input_index}")))
                            .and_then(|input| match input.input_id != input_id {
                                | true => Err(JournalError::Invalid(format!("module input identity changed at index {input_index}"))),
                                | false => {
                                    let initialized_modules = input.modules.len();
                                    match input.modules.get_mut(module_index) {
                                        | Some(summary) => match summary.index != module_index || summary.name != module {
                                            | true => Err(JournalError::Invalid(format!(
                                                "module identity changed at input {input_index}, index {module_index}"
                                            ))),
                                            | false => validate_transition(summary.state, state, "module")
                                                .map(|()| summary.update_state(state, error, timestamp_ms)),
                                        },
                                        | None if state.is_not_started() && module_index == initialized_modules => {
                                            input.modules.push(ModuleSummary {
                                                index: module_index,
                                                name: module,
                                                state,
                                                error,
                                                started_at_ms: None,
                                                updated_at_ms: timestamp_ms,
                                            });
                                            Ok(())
                                        }
                                        | None => Err(JournalError::Invalid(format!(
                                            "module {module_index} for input {input_index} was not initialized in order"
                                        ))),
                                    }
                                }
                            }),
                        | JournalEvent::RunFinished { status, error } => {
                            finalization = Some((status, error, timestamp_ms));
                            Ok(())
                        }
                    })
            })
            .and_then(|()| metadata.ok_or_else(|| JournalError::Invalid("run_started record is missing".to_string())))
            .and_then(|metadata| {
                let invalid_success = matches!(finalization, Some((FinalStatus::Succeeded, _, _)))
                    && inputs.values().any(|input| {
                        !input.state.is_complete() || (input.state.is_succeeded() && input.modules.iter().any(|module| !module.state.is_complete()))
                    });
                match invalid_success {
                    | true => Err(JournalError::Invalid(
                        "a successful finalization contains unfinished or failed work".to_string(),
                    )),
                    | false => {
                        let (status, error, finished_at_ms) = match finalization {
                            | Some((FinalStatus::Succeeded, error, finished_at_ms)) => (RunStatus::Succeeded, error, Some(finished_at_ms)),
                            | Some((FinalStatus::Failed, error, finished_at_ms)) => (RunStatus::Failed, error, Some(finished_at_ms)),
                            | None => (RunStatus::Interrupted, None, None),
                        };
                        let duration_ms = finished_at_ms.map(|finished| finished.saturating_sub(metadata.started_at_ms));
                        Ok(RunSummary {
                            schema_version: JOURNAL_SCHEMA_VERSION,
                            run_id: metadata.run_id,
                            configuration: metadata.configuration,
                            dry_run: metadata.dry_run,
                            started_at_ms: metadata.started_at_ms,
                            finished_at_ms,
                            duration_ms,
                            status,
                            error,
                            inputs: inputs.into_values().collect(),
                        })
                    }
                }
            })
    }
}
fn bounded_error(value: &str) -> String {
    if value.len() <= MAX_ERROR_BYTES {
        value.to_string()
    } else {
        const SUFFIX: &str = "...";
        let mut end = MAX_ERROR_BYTES - SUFFIX.len();
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}{}", &value[..end], SUFFIX)
    }
}
fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn validate_transition(current: ExecutionState, next: ExecutionState, kind: &str) -> Result<(), JournalError> {
    let valid = match current {
        | ExecutionState::NotStarted => next.is_pending() || next.is_running() || next.is_failed() || next.is_skipped() || next.is_skipped_existing(),
        | ExecutionState::Pending => next.is_running() || next.is_failed() || next.is_skipped() || next.is_skipped_existing(),
        | ExecutionState::Running => next.is_succeeded() || next.is_failed() || next.is_skipped() || next.is_skipped_existing(),
        | ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Skipped | ExecutionState::SkippedExisting => false,
    };
    match valid {
        | true => Ok(()),
        | false => Err(JournalError::Invalid(format!("invalid {kind} transition from {current:?} to {next:?}"))),
    }
}

#[cfg(test)]
mod tests;
