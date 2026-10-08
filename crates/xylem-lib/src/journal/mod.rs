//! Durable, append-only execution journals and deterministic recovery
extern crate alloc;

use crate::memory::{MemoryEstimator, PlannedMemoryFallback};
use crate::plan::{MemoryPlanMode, PlannedMemory};
use crate::sandbox::{
    EffectiveSandboxPolicy, IsolationClass, OciRuntime, PreparationNetwork, ProcessGuard, SandboxCapabilities, SandboxProviderKind, SandboxResources,
};
use crate::ReporterType;
use acorn_core::time::Milliseconds;
use acorn_core::validation::ValidationError;
use acorn_macros::With;
use acorn_schema::hardware::Memory;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{fmt, iter::once};
use owo_colors::Style;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::env;
use std::fs::{create_dir_all, remove_file, rename, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use strum::EnumIs;

/// Current on-disk journal schema version.
pub const JOURNAL_SCHEMA_VERSION: u32 = 9;
/// Maximum number of UTF-8 bytes retained for one journal error detail.
pub const MAX_ERROR_BYTES: usize = 4096;
trait Summary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<Milliseconds>, &mut Milliseconds);

    fn update_state(&mut self, next: ExecutionState, error: Option<String>, timestamp: Milliseconds) {
        let (current, current_error, started_at, updated_at) = self.state_fields();
        *current = next;
        *current_error = error;
        if next.is_running() && started_at.is_none() {
            *started_at = Some(timestamp);
        }
        *updated_at = timestamp;
    }
}
/// Durable state of a container used by the run.
#[derive(Clone, Copy, Debug, Deserialize, EnumIs, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    /// The declaration is known but lifecycle work has not started.
    NotStarted,
    /// The container is running and ready for module commands.
    Ready,
    /// The managed container was removed.
    Removed,
    /// The managed container was intentionally retained.
    Retained,
    /// An external container was verified and left untouched.
    External,
    /// Lifecycle work failed.
    Failed,
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
    /// Work was cancelled before successful completion.
    Cancelled,
    /// Work was intentionally not executed, such as during a dry run.
    Skipped,
    /// Work was not executed because declared outputs were already complete.
    SkippedExisting,
}
/// Final state written by a normally completed run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalStatus {
    /// The run was cancelled by its caller.
    Cancelled,
    /// All required work succeeded.
    Succeeded,
    /// At least one required operation failed.
    Failed,
}
#[derive(Clone, Debug)]
enum GpuJournalState {
    Disabled,
    Finished,
    Started { device_ids: Option<BTreeSet<String>> },
}
/// Recovered disposition of requested GPU monitoring.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuMonitoringStatus {
    /// Monitoring completed for the run.
    Complete,
    /// Monitoring started but stopped after a sampling failure.
    Degraded,
    /// The run ended before monitoring finalized.
    Interrupted,
    /// Monitoring could not collect its initial inventory and sample.
    Unavailable,
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
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    /// Declares the run before any work begins.
    RunStarted { metadata: Box<RunMetadata> },
    /// Records a container lifecycle transition.
    ContainerState {
        container: JournalContainer,
        state: ContainerState,
        error: Option<String>,
    },
    /// Records the immutable GPU inventory observed at monitoring start.
    GpuInventory { devices: Vec<GpuDevice> },
    /// Marks the end of GPU monitoring.
    GpuMonitoringFinished { error: Option<String> },
    /// Marks the requested GPU sampling cadence.
    GpuMonitoringStarted { interval: Milliseconds },
    /// Records one run-wide reading for every inventoried GPU.
    GpuSample { readings: Vec<GpuReading> },
    /// Records an input transition.
    InputState {
        input: JournalInput,
        state: ExecutionState,
        error: Option<String>,
    },
    /// Records an optional platform observation of one input's peak memory.
    InputMemoryObserved {
        input_index: usize,
        input_id: String,
        observed_peak: Memory,
    },
    /// Records a module transition for an input.
    ModuleState {
        input_index: usize,
        input_id: String,
        module_index: usize,
        module: String,
        module_path: Vec<String>,
        state: ExecutionState,
        error: Option<String>,
    },
    /// Marks a normal run finalization.
    RunFinished { status: FinalStatus, error: Option<String> },
}
/// Memory scheduling behavior recorded for a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEvidenceMode {
    /// Fixed input batching was retained because at least one estimate was unavailable.
    FixedFallback,
    /// Inputs were admitted using their memory reservations.
    MemoryAware,
}
/// Durable reason that an input could not use memory-aware admission.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryFallbackEvidence {
    /// The input size was outside the estimator's calibrated range.
    #[error("input size is outside the calibrated range")]
    InputOutOfRange {
        /// Largest input size accepted by the estimator.
        maximum_input: Memory,
        /// Smallest input size accepted by the estimator.
        minimum_input: Memory,
    },
    /// The input could not be measured safely.
    #[error("{detail}")]
    InputUnavailable {
        /// Bounded diagnostic explaining why sizing failed.
        detail: String,
    },
    /// The workflow had no concrete input to size.
    #[error("the workflow has no concrete input to size")]
    NoInput,
}
/// Overall state reconstructed from a journal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// The journal has a cancelled finalization record.
    Cancelled,
    /// The journal has a successful finalization record.
    Succeeded,
    /// The journal has a failed finalization record.
    Failed,
    /// The journal has no finalization record.
    Interrupted,
}
/// Stable configuration identity recorded at run start.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigurationIdentity {
    /// Configuration path used by the command.
    pub path: String,
    /// Optional workflow identifier declared by the configuration.
    pub id: Option<String>,
    /// Optional workflow version declared by the configuration.
    pub version: Option<String>,
}
/// Latest reconstructed lifecycle state for one container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContainerSummary {
    /// Deterministic zero-based position.
    pub index: usize,
    /// Effective runtime name.
    pub name: String,
    /// Whether Xylem owns the container lifecycle.
    pub owned: bool,
    /// Runtime executable name.
    pub runtime: String,
    /// Latest durable state.
    pub state: ContainerState,
    /// Bounded lifecycle failure, when present.
    pub error: Option<String>,
    /// Timestamp of the latest transition.
    pub updated_at: Milliseconds,
}
/// Stable identity and capacity for one monitored GPU.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuDevice {
    /// Driver enumeration index for display only.
    pub index: u32,
    /// Product name reported by the driver.
    pub name: String,
    /// Total frame-buffer memory in MiB, when supported.
    pub memory_total_mib: Option<u64>,
    /// Stable NVIDIA device UUID.
    pub uuid: String,
}
#[derive(Clone, Debug)]
struct GpuDeviceAggregate {
    device: GpuDevice,
    memory_used_mib: MetricAggregate,
    samples: u64,
    utilization_percent: MetricAggregate,
}
/// Aggregated telemetry for one monitored GPU.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuDeviceSummary {
    /// Driver enumeration index for display only.
    pub index: u32,
    /// Product name reported by the driver.
    pub name: String,
    /// Total frame-buffer memory in MiB, when supported.
    pub memory_total_mib: Option<u64>,
    /// Aggregate used frame-buffer memory in MiB.
    pub memory_used_mib: Option<GpuMetricSummary>,
    /// Number of readings containing this GPU.
    pub samples: u64,
    /// Aggregate GPU utilization percentage.
    pub utilization_percent: Option<GpuMetricSummary>,
    /// Stable NVIDIA device UUID.
    pub uuid: String,
}
/// Integer aggregate for one sampled GPU metric.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuMetricSummary {
    /// Arithmetic mean rounded to the nearest integer.
    pub average: u64,
    /// Maximum observed value.
    pub maximum: u64,
    /// Number of supported readings included in the aggregate.
    pub samples: u64,
}
/// Recovered run-wide GPU monitoring summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuMonitoringSummary {
    /// Bounded failure detail for unavailable or degraded monitoring.
    pub error: Option<String>,
    /// Fixed sampling interval in milliseconds.
    pub interval: Milliseconds,
    /// Monitoring disposition.
    pub status: GpuMonitoringStatus,
    /// Ordered monitored devices and their aggregates.
    pub devices: Vec<GpuDeviceSummary>,
}
/// One timestamped GPU reading stored in a journal sample event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuReading {
    /// Used frame-buffer memory in MiB, when supported.
    pub memory_used_mib: Option<u64>,
    /// GPU utilization percentage, when supported.
    pub utilization_percent: Option<u8>,
    /// Stable NVIDIA device UUID.
    pub uuid: String,
}
#[derive(Clone, Debug)]
struct GpuRecovery {
    devices: BTreeMap<String, GpuDeviceAggregate>,
    error: Option<String>,
    finished: bool,
    interval: Milliseconds,
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
    pub started_at: Option<Milliseconds>,
    /// Timestamp of the latest transition.
    pub updated_at: Milliseconds,
    /// Ordered module states.
    pub modules: Vec<ModuleSummary>,
}
/// Stable identity for a container in a run journal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalContainer {
    /// Deterministic zero-based position.
    pub index: usize,
    /// Effective runtime name used for execution.
    pub name: String,
    /// Whether Xylem owns the container lifecycle.
    pub owned: bool,
    /// Runtime executable name.
    pub runtime: String,
}
/// Stable identity and display value for one workflow input.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalInput {
    /// Deterministic zero-based input position.
    pub index: usize,
    /// Stable identity within this run.
    pub input_id: String,
    /// User-facing input value. `None` represents a zero-input workflow.
    pub value: Option<String>,
}
/// Stable identity for one executable module in a run journal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalModule {
    /// Deterministic zero-based module position.
    pub index: usize,
    /// Configured leaf display name.
    pub name: String,
    /// Stable ancestry from the root workflow to the executable leaf.
    pub module_path: Vec<String>,
}
/// One versioned line in a journal file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JournalRecord {
    /// Schema version for this record.
    pub schema_version: u32,
    /// Monotonically increasing sequence number, beginning at zero.
    pub sequence: u64,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
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
    input_observations: Vec<bool>,
    input_started: Vec<bool>,
    input_states: Vec<ExecutionState>,
    module_states: Vec<Vec<ExecutionState>>,
    container_states: Vec<ContainerState>,
    finalized: bool,
    gpu_monitoring: GpuJournalState,
}
/// Portable estimate and optional observation for one input.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemoryInputEvidence {
    /// Fixed-point estimate before applying the safety multiplier.
    pub estimated: Option<Memory>,
    /// Reason no reservation was produced.
    pub fallback: Option<MemoryFallbackEvidence>,
    /// Aggregate logical input bytes when sizing succeeded.
    pub input: Option<Memory>,
    /// Stable position in resolved input order.
    pub input_index: usize,
    /// Optional platform observation recorded separately from admission.
    pub observed_peak: Option<Memory>,
    /// Bytes enforced by the memory-aware admission queue.
    pub reservation: Option<Memory>,
    /// Difference between the reservation and base estimate.
    pub safety_reserve: Option<Memory>,
}
#[derive(Clone, Debug, Default)]
struct MetricAggregate {
    maximum: u64,
    samples: u64,
    sum: u128,
}
/// Latest reconstructed module state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleSummary {
    /// Deterministic zero-based module position.
    pub index: usize,
    /// Configured module name.
    pub name: String,
    /// Stable ancestry from the root workflow to the executable leaf.
    pub module_path: Vec<String>,
    /// Latest durable state.
    pub state: ExecutionState,
    /// Bounded failure detail, when present.
    pub error: Option<String>,
    /// First running timestamp, when execution began.
    pub started_at: Option<Milliseconds>,
    /// Timestamp of the latest transition.
    pub updated_at: Milliseconds,
}
/// A serialized, flush-on-transition journal writer safe for concurrent callers.
#[derive(Debug)]
pub struct RunJournal {
    path: PathBuf,
    run_id: String,
    inputs: Vec<JournalInput>,
    memory: Option<RunMemoryEvidence>,
    modules: Vec<JournalModule>,
    containers: Vec<JournalContainer>,
    sink: Mutex<JournalSink>,
}
/// Durable run-wide memory admission evidence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunMemoryEvidence {
    /// Explicit aggregate reservation budget.
    pub budget: Memory,
    /// Stable configured estimator identity.
    pub estimator_identity: String,
    /// Ordered per-input estimates and fallback reasons.
    pub inputs: Vec<MemoryInputEvidence>,
    /// Scheduling behavior selected by the execution plan.
    pub mode: MemoryEvidenceMode,
    /// Safety multiplier scaled by one million.
    pub safety_millionths: u64,
}
/// Run metadata written before any input begins.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, With)]
pub struct RunMetadata {
    /// Unique run identifier.
    pub run_id: String,
    /// Configuration used for this run.
    pub configuration: ConfigurationIdentity,
    /// Whether this is a side-effect-free dry run.
    pub dry_run: bool,
    /// Portable memory admission evidence, when configured.
    #[with(some)]
    pub memory: Option<RunMemoryEvidence>,
    /// Effective sandbox mode and non-secret policy identity.
    pub sandbox: SandboxEvidence,
    /// Unix timestamp in milliseconds.
    pub started_at: Milliseconds,
}
/// Deterministic summary reconstructed from durable journal records.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunSummary {
    /// Summary schema version.
    pub schema_version: u32,
    /// Unique run identifier.
    pub run_id: String,
    /// Configuration used for this run.
    pub configuration: ConfigurationIdentity,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Effective sandbox mode and non-secret policy identity.
    pub sandbox: SandboxEvidence,
    /// Run start timestamp.
    pub started_at: Milliseconds,
    /// Finalization timestamp, absent for interrupted runs.
    pub finished_at: Option<Milliseconds>,
    /// Elapsed run duration, absent for interrupted runs.
    pub duration: Option<Milliseconds>,
    /// Overall recovered status.
    pub status: RunStatus,
    /// Final bounded run error, when present.
    pub error: Option<String>,
    /// Run-wide NVIDIA GPU telemetry, when requested.
    pub gpu_monitoring: Option<GpuMonitoringSummary>,
    /// Ordered container lifecycle summaries.
    pub containers: Vec<ContainerSummary>,
    /// Ordered input summaries.
    pub inputs: Vec<InputSummary>,
    /// Portable memory admission evidence and optional observed peaks.
    pub memory: Option<RunMemoryEvidence>,
}
/// Non-secret sandbox evidence recorded for every run.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SandboxEvidence {
    /// Minimum required isolation class, absent for host mode.
    pub minimum_isolation: Option<IsolationClass>,
    /// `off` for compatibility host execution or `required` for sandbox execution.
    pub mode: String,
    /// Canonical effective-policy digest, absent for host mode.
    pub policy_digest: Option<String>,
    /// Network available only during dependency preparation.
    pub preparation_network: Option<PreparationNetwork>,
    /// Required process guards.
    pub process_guards: Option<Vec<ProcessGuard>>,
    /// Selected provider, absent for host mode.
    pub provider: Option<SandboxProviderKind>,
    /// Capabilities realized during provider discovery.
    pub realized_capabilities: Option<SandboxCapabilities>,
    /// Per-input provider resource ceilings.
    pub resources: Option<SandboxResources>,
    /// Selected OCI runtime, when applicable.
    pub runtime: Option<OciRuntime>,
    /// Provider lifecycle model, absent for host mode.
    pub session_model: Option<String>,
    /// Immutable provider workload, absent for host mode.
    pub workload: Option<String>,
}
impl ExecutionState {
    /// Whether this state represents completed work.
    pub fn is_complete(self) -> bool {
        self.is_succeeded() || self.is_skipped() || self.is_skipped_existing()
    }
    /// Whether this state is terminal regardless of success.
    pub fn is_terminal(self) -> bool {
        self.is_complete() || self.is_failed() || self.is_cancelled()
    }
}
impl fmt::Display for ExecutionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::NotStarted => "not_started",
            | Self::Pending => "pending",
            | Self::Running => "running",
            | Self::Succeeded => "succeeded",
            | Self::Failed => "failed",
            | Self::Cancelled => "cancelled",
            | Self::Skipped => "skipped",
            | Self::SkippedExisting => "skipped_existing",
        })
    }
}
impl Validate for GpuDevice {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if self.name.is_empty() {
            report.add("name", ValidationError::new("nonempty").with_message("GPU name cannot be empty"));
        }
        if self.uuid.is_empty() {
            report.add("uuid", ValidationError::new("nonempty").with_message("GPU UUID cannot be empty"));
        }
        report.finish()
    }
}
impl fmt::Display for GpuMonitoringStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::Complete => "complete",
            | Self::Degraded => "degraded",
            | Self::Interrupted => "interrupted",
            | Self::Unavailable => "unavailable",
        })
    }
}
impl GpuRecovery {
    fn new(interval: Milliseconds) -> Result<Self, JournalError> {
        match interval.is_zero() {
            | true => Err(JournalError::Invalid("GPU monitoring interval must be greater than zero".to_string())),
            | false => Ok(Self {
                devices: BTreeMap::new(),
                error: None,
                finished: false,
                interval,
            }),
        }
    }
    fn finish(&mut self, error: Option<String>) -> Result<(), JournalError> {
        match (self.finished, error.is_none() && self.devices.is_empty()) {
            | (true, _) => Err(JournalError::Invalid("GPU monitoring is already finished".to_string())),
            | (false, true) => Err(JournalError::Invalid(
                "GPU monitoring cannot finish successfully without an inventory".to_string(),
            )),
            | (false, false) => {
                self.error = error;
                self.finished = true;
                Ok(())
            }
        }
    }
    fn inventory(&mut self, devices: Vec<GpuDevice>) -> Result<(), JournalError> {
        match (self.finished, self.devices.is_empty()) {
            | (true, _) => Err(JournalError::Invalid("GPU inventory follows monitoring completion".to_string())),
            | (false, false) => Err(JournalError::Invalid("GPU inventory was already recorded".to_string())),
            | (false, true) => devices
                .validate()
                .map_err(JournalError::from)
                .and_then(|()| validate_gpu_inventory(&devices))
                .map(|()| {
                    self.devices = devices
                        .into_iter()
                        .map(|device| {
                            (
                                device.uuid.clone(),
                                GpuDeviceAggregate {
                                    device,
                                    memory_used_mib: MetricAggregate::default(),
                                    samples: 0,
                                    utilization_percent: MetricAggregate::default(),
                                },
                            )
                        })
                        .collect();
                }),
        }
    }
    fn sample(&mut self, readings: Vec<GpuReading>) -> Result<(), JournalError> {
        match (self.finished, self.devices.is_empty()) {
            | (true, _) => Err(JournalError::Invalid("GPU sample follows monitoring completion".to_string())),
            | (false, true) => Err(JournalError::Invalid("GPU sample precedes inventory".to_string())),
            | (false, false) => validate_gpu_readings(&readings, self.devices.keys()).map(|()| {
                readings.into_iter().for_each(|reading| {
                    if let Some(device) = self.devices.get_mut(&reading.uuid) {
                        device.samples = device.samples.saturating_add(1);
                        reading.memory_used_mib.into_iter().for_each(|value| device.memory_used_mib.update(value));
                        reading
                            .utilization_percent
                            .into_iter()
                            .for_each(|value| device.utilization_percent.update(u64::from(value)));
                    }
                });
            }),
        }
    }
    fn summary(self) -> GpuMonitoringSummary {
        let status = match (self.finished, self.error.is_some(), self.devices.is_empty()) {
            | (false, _, _) => GpuMonitoringStatus::Interrupted,
            | (true, true, true) => GpuMonitoringStatus::Unavailable,
            | (true, true, false) => GpuMonitoringStatus::Degraded,
            | (true, false, _) => GpuMonitoringStatus::Complete,
        };
        let mut devices = self
            .devices
            .into_values()
            .map(|aggregate| GpuDeviceSummary {
                index: aggregate.device.index,
                name: aggregate.device.name,
                memory_total_mib: aggregate.device.memory_total_mib,
                memory_used_mib: aggregate.memory_used_mib.summary(),
                samples: aggregate.samples,
                utilization_percent: aggregate.utilization_percent.summary(),
                uuid: aggregate.device.uuid,
            })
            .collect::<Vec<_>>();
        devices.sort_by(|left, right| (left.index, &left.uuid).cmp(&(right.index, &right.uuid)));
        GpuMonitoringSummary {
            error: self.error,
            interval: self.interval,
            status,
            devices,
        }
    }
}
impl Summary for InputSummary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<Milliseconds>, &mut Milliseconds) {
        (&mut self.state, &mut self.error, &mut self.started_at, &mut self.updated_at)
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
impl JournalModule {
    /// Create a stable module identity from its ordered position, display name, and ancestry.
    pub fn new(index: usize, name: String, module_path: Vec<String>) -> Self {
        Self { index, name, module_path }
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
                    timestamp: timestamp().get(),
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
impl fmt::Display for MemoryEvidenceMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::FixedFallback => "fixed_fallback",
            | Self::MemoryAware => "memory_aware",
        })
    }
}
impl From<MemoryPlanMode> for MemoryEvidenceMode {
    fn from(mode: MemoryPlanMode) -> Self {
        match mode {
            | MemoryPlanMode::FixedFallback => Self::FixedFallback,
            | MemoryPlanMode::MemoryAware => Self::MemoryAware,
        }
    }
}
impl From<&PlannedMemoryFallback> for MemoryFallbackEvidence {
    fn from(fallback: &PlannedMemoryFallback) -> Self {
        match fallback {
            | PlannedMemoryFallback::InputOutOfRange {
                maximum_input_bytes,
                minimum_input_bytes,
            } => Self::InputOutOfRange {
                maximum_input: Memory::kb(*maximum_input_bytes as f64 / 1024.0),
                minimum_input: Memory::kb(*minimum_input_bytes as f64 / 1024.0),
            },
            | PlannedMemoryFallback::InputUnavailable { detail } => Self::InputUnavailable {
                detail: bounded_error(detail),
            },
            | PlannedMemoryFallback::NoInput => Self::NoInput,
        }
    }
}
impl Validate for MemoryFallbackEvidence {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        match self {
            | Self::InputOutOfRange {
                maximum_input,
                minimum_input,
            } => match (minimum_input.checked_bytes(), maximum_input.checked_bytes()) {
                | (Some(minimum), Some(maximum)) if minimum > 0 && minimum <= maximum => (),
                | _ => report.add(
                    "maximum_input",
                    ValidationError::new("range").with_message("memory fallback input range must be positive and ordered"),
                ),
            },
            | Self::InputUnavailable { detail } if detail.is_empty() || detail.len() > MAX_ERROR_BYTES => report.add(
                "detail",
                ValidationError::new("length").with_message("memory fallback detail must be nonempty and bounded"),
            ),
            | Self::InputUnavailable { .. } | Self::NoInput => (),
        }
        report.finish()
    }
}
impl Validate for MemoryInputEvidence {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if let Some(fallback) = &self.fallback {
            if let Err(why) = fallback.validate() {
                report = report.merge("fallback", why);
            }
        }
        let decision_valid = match (
            self.estimated.as_ref().and_then(Memory::checked_bytes),
            self.fallback.as_ref(),
            self.input.as_ref().and_then(Memory::checked_bytes),
            self.reservation.as_ref().and_then(Memory::checked_bytes),
            self.safety_reserve.as_ref().and_then(Memory::checked_bytes),
        ) {
            | (Some(estimated), None, Some(input), Some(reservation), Some(safety)) => {
                estimated > 0 && input > 0 && estimated.checked_add(safety) == Some(reservation) && reservation > 0
            }
            | (None, Some(_), _, None, None) => self
                .input
                .as_ref()
                .is_none_or(|input| input.checked_bytes().is_some_and(|value| value > 0)),
            | _ => false,
        };
        if !decision_valid {
            report.add(
                "decision",
                ValidationError::new("consistent").with_message("memory evidence decision is inconsistent"),
            );
        }
        if self
            .observed_peak
            .as_ref()
            .is_some_and(|peak| peak.checked_bytes().filter(|value| *value > 0).is_none())
        {
            report.add(
                "observed_peak",
                ValidationError::new("positive").with_message("observed peak memory must be finite and greater than zero"),
            );
        }
        report.finish()
    }
}
impl MetricAggregate {
    fn summary(self) -> Option<GpuMetricSummary> {
        (self.samples > 0).then(|| GpuMetricSummary {
            average: u64::try_from((self.sum + u128::from(self.samples / 2)) / u128::from(self.samples)).unwrap_or(u64::MAX),
            maximum: self.maximum,
            samples: self.samples,
        })
    }
    fn update(&mut self, value: u64) {
        self.maximum = self.maximum.max(value);
        self.samples = self.samples.saturating_add(1);
        self.sum = self.sum.saturating_add(u128::from(value));
    }
}
impl Summary for ModuleSummary {
    fn state_fields(&mut self) -> (&mut ExecutionState, &mut Option<String>, &mut Option<Milliseconds>, &mut Milliseconds) {
        (&mut self.state, &mut self.error, &mut self.started_at, &mut self.updated_at)
    }
}
impl RunJournal {
    /// Create a new journal and durably initialize every input and module.
    ///
    /// Existing destinations are never overwritten. No execution should begin until this constructor has successfully returned.
    pub fn create(path: PathBuf, metadata: RunMetadata, inputs: Vec<JournalInput>, modules: Vec<JournalModule>) -> Result<Self, JournalError> {
        Self::create_with_containers(path, metadata, inputs, modules, Vec::new())
    }
    /// Create a journal with run-scoped container declarations.
    pub fn create_with_containers(
        path: PathBuf,
        metadata: RunMetadata,
        inputs: Vec<JournalInput>,
        modules: Vec<JournalModule>,
        containers: Vec<JournalContainer>,
    ) -> Result<Self, JournalError> {
        metadata
            .validate()
            .map_err(JournalError::from)
            .and_then(|()| path.parent().map_or(Ok(()), create_dir_all).map_err(JournalError::from))
            .and_then(|()| OpenOptions::new().write(true).create_new(true).open(&path).map_err(JournalError::from))
            .and_then(|file| {
                let module_states = inputs.iter().map(|_| vec![ExecutionState::NotStarted; modules.len()]).collect::<Vec<_>>();
                let memory = metadata.memory.clone();
                let journal = Self {
                    path,
                    run_id: metadata.run_id.clone(),
                    inputs: inputs.clone(),
                    memory,
                    modules: modules.clone(),
                    containers: containers.clone(),
                    sink: Mutex::new(JournalSink {
                        writer: BufWriter::new(file),
                        next_sequence: 0,
                        input_observations: vec![false; inputs.len()],
                        input_started: vec![false; inputs.len()],
                        input_states: vec![ExecutionState::NotStarted; inputs.len()],
                        module_states,
                        container_states: vec![ContainerState::NotStarted; containers.len()],
                        finalized: false,
                        gpu_monitoring: GpuJournalState::Disabled,
                    }),
                };
                match journal.validate().map_err(JournalError::from) {
                    | Ok(()) => journal
                        .append(JournalEvent::RunStarted {
                            metadata: Box::new(metadata),
                        })
                        .and_then(|()| {
                            containers.into_iter().try_for_each(|container| {
                                journal.append(JournalEvent::ContainerState {
                                    container,
                                    state: ContainerState::NotStarted,
                                    error: None,
                                })
                            })
                        })
                        .and_then(|()| {
                            inputs.into_iter().try_for_each(|input| {
                                journal
                                    .append(JournalEvent::InputState {
                                        input: input.clone(),
                                        state: ExecutionState::NotStarted,
                                        error: None,
                                    })
                                    .and_then(|()| {
                                        modules.iter().try_for_each(|module| {
                                            journal.append(JournalEvent::ModuleState {
                                                input_index: input.index,
                                                input_id: input.input_id.clone(),
                                                module_index: module.index,
                                                module: module.name.clone(),
                                                module_path: module.module_path.clone(),
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
    /// Durably transition one run-scoped container.
    pub fn transition_container(&self, index: usize, state: ContainerState, error: Option<&str>) -> Result<(), JournalError> {
        self.containers
            .get(index)
            .cloned()
            .ok_or_else(|| JournalError::Invalid(format!("unknown container index {index}")))
            .and_then(|container| {
                self.sink.lock().map_err(|_| JournalError::Poisoned).and_then(|mut sink| {
                    let current = sink.container_states[index];
                    validate_container_transition(current, state)
                        .and_then(|()| {
                            sink.append_locked(
                                &self.run_id,
                                JournalEvent::ContainerState {
                                    container,
                                    state,
                                    error: error.map(bounded_error),
                                },
                            )
                        })
                        .map(|()| sink.container_states[index] = state)
                })
            })
    }
    /// Durably record the immutable GPU inventory discovered by the monitor.
    pub(crate) fn record_gpu_inventory(&self, devices: Vec<GpuDevice>) -> Result<(), JournalError> {
        devices
            .validate()
            .map_err(JournalError::from)
            .and_then(|()| validate_gpu_inventory(&devices))
            .and_then(|()| {
                self.sink
                    .lock()
                    .map_err(|_| JournalError::Poisoned)
                    .and_then(|mut sink| match &sink.gpu_monitoring {
                        | GpuJournalState::Started { device_ids: None } => sink
                            .append_locked(&self.run_id, JournalEvent::GpuInventory { devices: devices.clone() })
                            .map(|()| {
                                sink.gpu_monitoring = GpuJournalState::Started {
                                    device_ids: Some(devices.into_iter().map(|device| device.uuid).collect()),
                                };
                            }),
                        | GpuJournalState::Started { device_ids: Some(_) } => {
                            Err(JournalError::Invalid("GPU inventory was already recorded".to_string()))
                        }
                        | GpuJournalState::Disabled => Err(JournalError::Invalid("GPU inventory precedes monitoring start".to_string())),
                        | GpuJournalState::Finished => Err(JournalError::Invalid("GPU inventory follows monitoring completion".to_string())),
                    })
            })
    }
    /// Durably record one run-wide GPU sample.
    pub(crate) fn record_gpu_sample(&self, readings: Vec<GpuReading>) -> Result<(), JournalError> {
        self.sink
            .lock()
            .map_err(|_| JournalError::Poisoned)
            .and_then(|mut sink| match &sink.gpu_monitoring {
                | GpuJournalState::Started {
                    device_ids: Some(device_ids),
                } => validate_gpu_readings(&readings, device_ids.iter())
                    .and_then(|()| sink.append_locked(&self.run_id, JournalEvent::GpuSample { readings })),
                | GpuJournalState::Started { device_ids: None } => Err(JournalError::Invalid("GPU sample precedes inventory".to_string())),
                | GpuJournalState::Disabled => Err(JournalError::Invalid("GPU sample precedes monitoring start".to_string())),
                | GpuJournalState::Finished => Err(JournalError::Invalid("GPU sample follows monitoring completion".to_string())),
            })
    }
    /// Durably start run-wide GPU monitoring.
    pub(crate) fn start_gpu_monitoring(&self, interval: Milliseconds) -> Result<(), JournalError> {
        match interval.is_zero() {
            | true => Err(JournalError::Invalid("GPU monitoring interval must be greater than zero".to_string())),
            | false => self
                .sink
                .lock()
                .map_err(|_| JournalError::Poisoned)
                .and_then(|mut sink| match sink.gpu_monitoring {
                    | GpuJournalState::Disabled => sink
                        .append_locked(&self.run_id, JournalEvent::GpuMonitoringStarted { interval })
                        .map(|()| sink.gpu_monitoring = GpuJournalState::Started { device_ids: None }),
                    | GpuJournalState::Started { .. } | GpuJournalState::Finished => {
                        Err(JournalError::Invalid("GPU monitoring was already started".to_string()))
                    }
                }),
        }
    }
    /// Durably finish run-wide GPU monitoring.
    pub(crate) fn finish_gpu_monitoring(&self, error: Option<&str>) -> Result<(), JournalError> {
        self.sink
            .lock()
            .map_err(|_| JournalError::Poisoned)
            .and_then(|mut sink| match &sink.gpu_monitoring {
                | GpuJournalState::Started { device_ids } if error.is_some() || device_ids.is_some() => sink
                    .append_locked(
                        &self.run_id,
                        JournalEvent::GpuMonitoringFinished {
                            error: error.map(bounded_error),
                        },
                    )
                    .map(|()| sink.gpu_monitoring = GpuJournalState::Finished),
                | GpuJournalState::Started { .. } => Err(JournalError::Invalid(
                    "GPU monitoring cannot finish successfully without an inventory".to_string(),
                )),
                | GpuJournalState::Disabled => Err(JournalError::Invalid("GPU monitoring was not started".to_string())),
                | GpuJournalState::Finished => Err(JournalError::Invalid("GPU monitoring is already finished".to_string())),
            })
    }
    /// Return the journal's filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Durably record one positive platform observation of an input's peak memory.
    pub fn record_memory_peak(&self, input_index: usize, observed_peak: Memory) -> Result<(), JournalError> {
        self.memory
            .as_ref()
            .and_then(|memory| memory.inputs.get(input_index))
            .ok_or_else(|| JournalError::Invalid(format!("input {input_index} has no configured memory evidence")))
            .and_then(|_| {
                self.inputs
                    .get(input_index)
                    .ok_or_else(|| JournalError::Invalid(format!("unknown input index {input_index}")))
            })
            .and_then(|input| {
                self.sink.lock().map_err(|_| JournalError::Poisoned).and_then(|mut sink| {
                    match (
                        observed_peak.checked_bytes().is_some_and(|value| value > 0),
                        sink.input_started[input_index],
                        sink.input_observations[input_index],
                    ) {
                        | (false, _, _) => Err(JournalError::Invalid("observed peak memory must be greater than zero".to_string())),
                        | (_, false, _) => Err(JournalError::Invalid(format!(
                            "input {input_index} peak memory was observed before execution"
                        ))),
                        | (_, _, true) => Err(JournalError::Invalid(format!("input {input_index} peak memory was already observed"))),
                        | (true, true, false) => sink
                            .append_locked(
                                &self.run_id,
                                JournalEvent::InputMemoryObserved {
                                    input_index,
                                    input_id: input.input_id.clone(),
                                    observed_peak,
                                },
                            )
                            .map(|()| sink.input_observations[input_index] = true),
                    }
                })
            })
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
                        .map(|()| {
                            sink.input_states[input_index] = state;
                            if state.is_running() {
                                sink.input_started[input_index] = true;
                            }
                        })
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
                                    module: module.name.clone(),
                                    module_path: module.module_path.clone(),
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
                matches!(sink.gpu_monitoring, GpuJournalState::Started { .. }),
            ) {
                | (true, _, _) => Err(JournalError::Invalid("run is already finalized".to_string())),
                | (false, true, _) => Err(JournalError::Invalid(
                    "a successful run cannot contain unfinished or failed inputs".to_string(),
                )),
                | (false, false, true) => Err(JournalError::Invalid("run cannot finalize while GPU monitoring is active".to_string())),
                | (false, false, false) => sink
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
        let report = self
            .inputs
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
            .0;
        let validation = self
            .memory
            .as_ref()
            .map_or(Ok(()), |memory| memory.validate_for_inputs(self.inputs.len(), false));
        let report = match validation {
            | Ok(()) => report,
            | Err(why) => {
                let mut report = report;
                report.add("memory", ValidationError::new("invalid").with_message(why.to_string()));
                report
            }
        };
        self.modules
            .iter()
            .enumerate()
            .fold(report, |mut report, (expected_index, module)| {
                if module.index != expected_index {
                    report.add(
                        format!("modules[{expected_index}].index"),
                        ValidationError::new("sequence").with_message(format!("Expected module index {expected_index}, found {}", module.index)),
                    );
                }
                if module.name.is_empty() || module.module_path.is_empty() || module.module_path.iter().any(String::is_empty) {
                    report.add(
                        format!("modules[{expected_index}].module_path"),
                        ValidationError::new("nonempty").with_message("Module name and ancestry cannot be empty"),
                    );
                }
                report
            })
            .finish()
    }
}
impl RunMemoryEvidence {
    fn observe(&mut self, input_index: usize, observed_peak: Memory) -> Result<(), JournalError> {
        self.inputs
            .get_mut(input_index)
            .ok_or_else(|| JournalError::Invalid(format!("memory observation references unknown input {input_index}")))
            .and_then(|input| {
                match (
                    observed_peak.checked_bytes().is_some_and(|value| value > 0),
                    input.observed_peak.is_some(),
                ) {
                    | (false, _) => Err(JournalError::Invalid("observed peak memory must be greater than zero".to_string())),
                    | (_, true) => Err(JournalError::Invalid(format!("input {input_index} peak memory was already observed"))),
                    | (true, false) => {
                        input.observed_peak = Some(observed_peak);
                        Ok(())
                    }
                }
            })
    }
    fn validate_for_inputs(&self, count: usize, allow_observations: bool) -> Result<(), JournalError> {
        self.validate().map_err(JournalError::from).and_then(|()| {
            match (
                self.inputs.len() == count,
                allow_observations || self.inputs.iter().all(|input| input.observed_peak.is_none()),
            ) {
                | (false, _) => Err(JournalError::Invalid(format!(
                    "memory evidence contains {} inputs for {count} journal inputs",
                    self.inputs.len()
                ))),
                | (_, false) => Err(JournalError::Invalid("memory evidence contains an observation at run start".to_string())),
                | (true, true) => Ok(()),
            }
        })
    }
}
impl TryFrom<&PlannedMemory> for RunMemoryEvidence {
    type Error = JournalError;

    fn try_from(planned: &PlannedMemory) -> Result<Self, Self::Error> {
        let estimator = MemoryEstimator::new(
            planned.estimator.identity.clone(),
            planned.estimator.intercept.clone(),
            planned.estimator.minimum_input.clone(),
            planned.estimator.maximum_input.clone(),
            planned.estimator.slope_millionths,
            planned.estimator.safety_millionths,
        );
        estimator
            .map_err(|why| JournalError::Invalid(format!("rebuild planned memory estimator: {why}")))
            .and_then(|estimator| {
                planned
                    .inputs
                    .iter()
                    .map(|input| match (&input.fallback, &input.reservation) {
                        | (Some(fallback), None) => Ok(MemoryInputEvidence {
                            estimated: None,
                            fallback: Some(fallback.into()),
                            input: input.input_bytes.map(|bytes| Memory::kb(bytes as f64 / 1024.0)),
                            input_index: input.input_index,
                            observed_peak: None,
                            reservation: None,
                            safety_reserve: None,
                        }),
                        | (None, Some(reservation)) => input
                            .input_bytes
                            .filter(|value| *value > 0)
                            .ok_or_else(|| JournalError::Invalid(format!("planned memory input {} is missing its input size", input.input_index)))
                            .and_then(|input_bytes| {
                                estimator
                                    .reservation_breakdown_bytes(input_bytes)
                                    .map_err(|why| JournalError::Invalid(format!("derive memory evidence for input {}: {why}", input.input_index)))
                                    .and_then(|breakdown| {
                                        let reservation_bytes = breakdown.reservation.checked_bytes().unwrap_or_default();
                                        match reservation.checked_bytes() == Some(reservation_bytes) {
                                            | true => Ok(MemoryInputEvidence {
                                                estimated: Some(breakdown.estimated),
                                                fallback: None,
                                                input: Some(Memory::kb(input_bytes as f64 / 1024.0)),
                                                input_index: input.input_index,
                                                observed_peak: None,
                                                reservation: Some(breakdown.reservation),
                                                safety_reserve: Some(breakdown.safety_reserve),
                                            }),
                                            | false => Err(JournalError::Invalid(format!(
                                                "planned memory reservation changed for input {}",
                                                input.input_index
                                            ))),
                                        }
                                    })
                            }),
                        | _ => Err(JournalError::Invalid(format!(
                            "planned memory input {} has an inconsistent decision",
                            input.input_index
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .and_then(|inputs| {
                let evidence = Self {
                    budget: planned.budget.clone(),
                    estimator_identity: planned.estimator.identity.clone(),
                    inputs,
                    mode: planned.mode.into(),
                    safety_millionths: planned.estimator.safety_millionths,
                };
                evidence.validate_for_inputs(planned.inputs.len(), false).map(|()| evidence)
            })
    }
}
impl Validate for RunMemoryEvidence {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        let budget = self.budget.checked_bytes().filter(|value| *value > 0);
        if budget.is_none() {
            report.add(
                "budget",
                ValidationError::new("positive").with_message("memory evidence budget must be finite and greater than zero"),
            );
        }
        if self.estimator_identity.trim().is_empty() {
            report.add(
                "estimator_identity",
                ValidationError::new("nonempty").with_message("memory evidence estimator identity must not be empty"),
            );
        }
        if self.inputs.is_empty() {
            report.add(
                "inputs",
                ValidationError::new("nonempty").with_message("memory evidence inputs must not be empty"),
            );
        }
        if self.safety_millionths < 1_000_000 {
            report.add(
                "safety_millionths",
                ValidationError::new("minimum").with_message("memory evidence safety multiplier must be at least 1000000 millionths"),
            );
        }
        let mode_valid = match self.mode {
            | MemoryEvidenceMode::FixedFallback => self.inputs.iter().any(|input| input.fallback.is_some()),
            | MemoryEvidenceMode::MemoryAware => self.inputs.iter().all(|input| input.fallback.is_none()),
        };
        if !mode_valid {
            report.add(
                "mode",
                ValidationError::new("consistent").with_message("memory evidence mode does not match its input decisions"),
            );
        }
        report = self.inputs.iter().enumerate().fold(report, |mut report, (expected_index, input)| {
            if input.input_index != expected_index {
                report.add(
                    format!("inputs[{expected_index}].input_index"),
                    ValidationError::new("sequence").with_message(format!("expected input index {expected_index}, found {}", input.input_index)),
                );
            }
            if let Err(why) = input.validate() {
                let path = format!("inputs[{expected_index}]");
                report = report.merge(&path, why);
            }
            if let (Some(budget), Some(reservation)) = (budget, input.reservation.as_ref().and_then(Memory::checked_bytes)) {
                if reservation > budget {
                    report.add(
                        format!("inputs[{expected_index}].reservation"),
                        ValidationError::new("maximum").with_message("memory reservation exceeds the run budget"),
                    );
                }
            }
            report
        });
        report.finish()
    }
}
impl RunMetadata {
    /// Build metadata with the current system time.
    pub fn now(run_id: String, configuration: ConfigurationIdentity, dry_run: bool, sandbox: SandboxEvidence) -> Self {
        Self {
            run_id,
            configuration,
            dry_run,
            memory: None,
            sandbox,
            started_at: timestamp(),
        }
    }
}
impl Validate for RunMetadata {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if self.run_id.is_empty() {
            report.add("run_id", ValidationError::new("nonempty").with_message("Run ID cannot be empty"));
        }
        if let Some(memory) = &self.memory {
            if let Err(why) = memory.validate() {
                report = report.merge("memory", why);
            }
            if memory.inputs.iter().any(|input| input.observed_peak.is_some()) {
                report.add(
                    "memory.inputs",
                    ValidationError::new("unobserved").with_message("memory evidence cannot contain observations at run start"),
                );
            }
        }
        report.finish()
    }
}
impl fmt::Display for RunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            | Self::Succeeded => "succeeded",
            | Self::Failed => "failed",
            | Self::Cancelled => "cancelled",
            | Self::Interrupted => "interrupted",
        })
    }
}
impl RunStatus {
    fn style(self) -> Style {
        match self {
            | Self::Cancelled => Style::new().yellow(),
            | Self::Failed => Style::new().red().bold(),
            | Self::Interrupted => Style::new().yellow().bold(),
            | Self::Succeeded => Style::new().green().bold(),
        }
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
        let stdout = io::stdout();
        let color = stdout.is_terminal() && env::var_os("NO_COLOR").is_none();
        self.render_console_to(&mut stdout.lock(), journal_path, color)
    }
    fn render_console_to(&self, output: &mut impl Write, journal_path: &Path, color: bool) -> Result<(), JournalError> {
        let mut succeeded = 0usize;
        let mut cancelled = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut skipped_existing = 0usize;
        let mut unfinished = 0usize;
        for input in &self.inputs {
            match input.state {
                | ExecutionState::Succeeded => succeeded += 1,
                | ExecutionState::Cancelled => cancelled += 1,
                | ExecutionState::Failed => failed += 1,
                | ExecutionState::Skipped => skipped += 1,
                | ExecutionState::SkippedExisting => skipped_existing += 1,
                | ExecutionState::NotStarted | ExecutionState::Pending | ExecutionState::Running => unfinished += 1,
            }
        }
        let run_id = console_text(&self.run_id, Style::new().cyan().bold(), color);
        let run_status = console_run_status(self.status, color);
        let succeeded = console_text(format!("{succeeded} succeeded"), Style::new().green(), color);
        let cancelled = console_text(format!("{cancelled} cancelled"), Style::new().yellow(), color);
        let failed = console_text(format!("{failed} failed"), Style::new().red(), color);
        let skipped = console_text(format!("{skipped} skipped"), Style::new().dimmed(), color);
        let skipped_existing = console_text(format!("{skipped_existing} skipped existing"), Style::new().dimmed(), color);
        let unfinished = console_text(format!("{unfinished} unfinished"), Style::new().cyan(), color);
        writeln!(output, "Run {run_id}: {run_status}")
            .and_then(|()| {
                writeln!(
                    output,
                    "Inputs: {succeeded}, {cancelled}, {failed}, {skipped}, {skipped_existing}, {unfinished}"
                )
            })
            .and_then(|()| {
                self.memory.as_ref().map_or(Ok(()), |memory| {
                    writeln!(
                        output,
                        "Memory: {} via {}, budget {} bytes",
                        memory.mode,
                        memory.estimator_identity,
                        memory.budget.checked_bytes().unwrap_or_default()
                    )
                    .and_then(|()| {
                        memory.inputs.iter().try_for_each(|input| match &input.fallback {
                            | Some(fallback) => writeln!(output, "Memory input {}: fixed fallback ({fallback})", input.input_index),
                            | None => writeln!(
                                output,
                                "Memory input {}: estimate {} + safety {} = reservation {} bytes{}",
                                input.input_index,
                                input.estimated.as_ref().and_then(Memory::checked_bytes).unwrap_or_default(),
                                input.safety_reserve.as_ref().and_then(Memory::checked_bytes).unwrap_or_default(),
                                input.reservation.as_ref().and_then(Memory::checked_bytes).unwrap_or_default(),
                                input
                                    .observed_peak
                                    .as_ref()
                                    .and_then(Memory::checked_bytes)
                                    .map_or(String::new(), |peak| format!(", observed {peak} bytes"))
                            ),
                        })
                    })
                })
            })
            .and_then(|()| {
                let (removed, retained, external, failed) =
                    self.containers
                        .iter()
                        .fold((0usize, 0usize, 0usize, 0usize), |(removed, retained, external, failed), container| {
                            (
                                removed.saturating_add(usize::from(container.state.is_removed())),
                                retained.saturating_add(usize::from(container.state.is_retained())),
                                external.saturating_add(usize::from(container.state.is_external())),
                                failed.saturating_add(usize::from(container.state.is_failed())),
                            )
                        });
                let removed = console_text(format!("{removed} removed"), Style::new().green(), color);
                let retained = console_text(format!("{retained} retained"), Style::new().yellow(), color);
                let external = console_text(format!("{external} external"), Style::new().cyan(), color);
                let failed = console_text(format!("{failed} failed"), Style::new().red(), color);
                writeln!(output, "Containers: {removed}, {retained}, {external}, {failed}")
            })
            .and_then(|()| match &self.gpu_monitoring {
                | None => Ok(()),
                | Some(monitoring) => writeln!(
                    output,
                    "GPU monitoring: {} at {} ms{}",
                    monitoring.status,
                    monitoring.interval,
                    monitoring.error.as_deref().map_or(String::new(), |error| format!(": {error}"))
                )
                .and_then(|()| {
                    monitoring.devices.iter().try_for_each(|device| {
                        let utilization = device
                            .utilization_percent
                            .as_ref()
                            .map_or_else(|| "n/a".to_string(), |metric| format!("avg {}%, max {}%", metric.average, metric.maximum));
                        let memory = device.memory_used_mib.as_ref().map_or_else(
                            || "n/a".to_string(),
                            |metric| {
                                format!(
                                    "avg {} MiB, max {} MiB{}",
                                    metric.average,
                                    metric.maximum,
                                    device.memory_total_mib.map_or(String::new(), |total| format!(" / {total} MiB"))
                                )
                            },
                        );
                        writeln!(
                            output,
                            "GPU {} {} ({}): {} samples, utilization {utilization}, memory {memory}",
                            device.index, device.name, device.uuid, device.samples
                        )
                    })
                }),
            })
            .and_then(|()| render_console_workflow_table(output, &self.inputs, color))
            .and_then(|()| {
                writeln!(
                    output,
                    "Journal: {}",
                    console_text(journal_path.display(), Style::new().cyan().dimmed(), color)
                )
            })
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
        let mut containers = BTreeMap::<usize, ContainerSummary>::new();
        let mut metadata = None;
        let mut inputs = BTreeMap::<usize, InputSummary>::new();
        let mut finalization = None;
        let mut gpu_monitoring = None::<GpuRecovery>;
        let mut run_id = None::<String>;
        records
            .into_iter()
            .enumerate()
            .try_for_each(|(expected_sequence, record)| {
                let JournalRecord {
                    schema_version,
                    sequence,
                    timestamp: timestamp_value,
                    run_id: record_run_id,
                    event,
                } = record;
                let timestamp = Milliseconds::new(timestamp_value);
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
                                | false => value.validate().map_err(JournalError::from).map(|()| metadata = Some(*value)),
                            }
                        }
                        | JournalEvent::ContainerState { container, state, error } => {
                            let initialized = containers.len();
                            match containers.get_mut(&container.index) {
                                | Some(summary) => {
                                    let changed =
                                        summary.name != container.name || summary.owned != container.owned || summary.runtime != container.runtime;
                                    match changed {
                                        | true => Err(JournalError::Invalid(format!("container identity changed at index {}", container.index))),
                                        | false => validate_container_transition(summary.state, state).map(|()| {
                                            summary.state = state;
                                            summary.error = error;
                                            summary.updated_at = timestamp;
                                        }),
                                    }
                                }
                                | None if state.is_not_started() && container.index == initialized => {
                                    containers.insert(
                                        container.index,
                                        ContainerSummary {
                                            index: container.index,
                                            name: container.name,
                                            owned: container.owned,
                                            runtime: container.runtime,
                                            state,
                                            error,
                                            updated_at: timestamp,
                                        },
                                    );
                                    Ok(())
                                }
                                | None => Err(JournalError::Invalid(format!(
                                    "container {} was not initialized in order",
                                    container.index
                                ))),
                            }
                        }
                        | JournalEvent::GpuInventory { devices } => gpu_monitoring
                            .as_mut()
                            .ok_or_else(|| JournalError::Invalid("GPU inventory precedes monitoring start".to_string()))
                            .and_then(|monitoring| monitoring.inventory(devices)),
                        | JournalEvent::GpuMonitoringFinished { error } => gpu_monitoring
                            .as_mut()
                            .ok_or_else(|| JournalError::Invalid("GPU monitoring completion precedes monitoring start".to_string()))
                            .and_then(|monitoring| monitoring.finish(error)),
                        | JournalEvent::GpuMonitoringStarted { interval } => match gpu_monitoring.is_some() {
                            | true => Err(JournalError::Invalid("GPU monitoring was already started".to_string())),
                            | false => GpuRecovery::new(interval).map(|monitoring| gpu_monitoring = Some(monitoring)),
                        },
                        | JournalEvent::GpuSample { readings } => gpu_monitoring
                            .as_mut()
                            .ok_or_else(|| JournalError::Invalid("GPU sample precedes monitoring start".to_string()))
                            .and_then(|monitoring| monitoring.sample(readings)),
                        | JournalEvent::InputState { input, state, error } => {
                            let initialized_inputs = inputs.len();
                            match inputs.get_mut(&input.index) {
                                | Some(summary) => match summary.input_id != input.input_id || summary.value != input.value {
                                    | true => Err(JournalError::Invalid(format!("input identity changed at index {}", input.index))),
                                    | false => {
                                        validate_transition(summary.state, state, "input").map(|()| summary.update_state(state, error, timestamp))
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
                                            started_at: None,
                                            updated_at: timestamp,
                                            modules: Vec::new(),
                                        },
                                    );
                                    Ok(())
                                }
                                | None => Err(JournalError::Invalid(format!("input {} was not initialized in order", input.index))),
                            }
                        }
                        | JournalEvent::InputMemoryObserved {
                            input_index,
                            input_id,
                            observed_peak,
                        } => inputs
                            .get(&input_index)
                            .ok_or_else(|| JournalError::Invalid(format!("memory observation references unknown input {input_index}")))
                            .and_then(|input| match (input.input_id == input_id, input.started_at.is_some()) {
                                | (false, _) => Err(JournalError::Invalid(format!(
                                    "memory observation input identity changed at index {input_index}"
                                ))),
                                | (_, false) => Err(JournalError::Invalid(format!(
                                    "input {input_index} peak memory was observed before execution"
                                ))),
                                | (true, true) => metadata
                                    .as_mut()
                                    .and_then(|metadata| metadata.memory.as_mut())
                                    .ok_or_else(|| JournalError::Invalid(format!("input {input_index} has no configured memory evidence")))
                                    .and_then(|memory| memory.observe(input_index, observed_peak)),
                            }),
                        | JournalEvent::ModuleState {
                            input_index,
                            input_id,
                            module_index,
                            module,
                            module_path,
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
                                        | Some(summary) => {
                                            match summary.index != module_index || summary.name != module || summary.module_path != module_path {
                                                | true => Err(JournalError::Invalid(format!(
                                                    "module identity changed at input {input_index}, index {module_index}"
                                                ))),
                                                | false => validate_transition(summary.state, state, "module")
                                                    .map(|()| summary.update_state(state, error, timestamp)),
                                            }
                                        }
                                        | None if state.is_not_started() && module_index == initialized_modules => {
                                            input.modules.push(ModuleSummary {
                                                index: module_index,
                                                name: module,
                                                module_path,
                                                state,
                                                error,
                                                started_at: None,
                                                updated_at: timestamp,
                                            });
                                            Ok(())
                                        }
                                        | None => Err(JournalError::Invalid(format!(
                                            "module {module_index} for input {input_index} was not initialized in order"
                                        ))),
                                    }
                                }
                            }),
                        | JournalEvent::RunFinished { status, error } => match gpu_monitoring.as_ref().is_some_and(|monitoring| !monitoring.finished)
                        {
                            | true => Err(JournalError::Invalid("run finalized while GPU monitoring was active".to_string())),
                            | false => {
                                finalization = Some((status, error, timestamp));
                                Ok(())
                            }
                        },
                    })
            })
            .and_then(|()| metadata.ok_or_else(|| JournalError::Invalid("run_started record is missing".to_string())))
            .and_then(|metadata| {
                let memory_validation = metadata
                    .memory
                    .as_ref()
                    .map_or(Ok(()), |memory| memory.validate_for_inputs(inputs.len(), true));
                let invalid_success = matches!(finalization, Some((FinalStatus::Succeeded, _, _)))
                    && inputs.values().any(|input| {
                        !input.state.is_complete() || (input.state.is_succeeded() && input.modules.iter().any(|module| !module.state.is_complete()))
                    });
                memory_validation.and_then(|()| match invalid_success {
                    | true => Err(JournalError::Invalid(
                        "a successful finalization contains unfinished or failed work".to_string(),
                    )),
                    | false => {
                        let (status, error, finished_at) = match finalization {
                            | Some((FinalStatus::Cancelled, error, finished_at)) => (RunStatus::Cancelled, error, Some(finished_at)),
                            | Some((FinalStatus::Succeeded, error, finished_at)) => (RunStatus::Succeeded, error, Some(finished_at)),
                            | Some((FinalStatus::Failed, error, finished_at)) => (RunStatus::Failed, error, Some(finished_at)),
                            | None => (RunStatus::Interrupted, None, None),
                        };
                        let duration = finished_at.map(|finished| Milliseconds::new(finished.get().saturating_sub(metadata.started_at.get())));
                        Ok(RunSummary {
                            schema_version: JOURNAL_SCHEMA_VERSION,
                            run_id: metadata.run_id,
                            configuration: metadata.configuration,
                            dry_run: metadata.dry_run,
                            sandbox: metadata.sandbox,
                            started_at: metadata.started_at,
                            finished_at,
                            duration,
                            status,
                            error,
                            gpu_monitoring: gpu_monitoring.map(GpuRecovery::summary),
                            containers: containers.into_values().collect(),
                            inputs: inputs.into_values().collect(),
                            memory: metadata.memory,
                        })
                    }
                })
            })
    }
}
impl SandboxEvidence {
    /// Record compatibility host execution.
    pub fn off() -> Self {
        Self {
            minimum_isolation: None,
            mode: "off".to_string(),
            policy_digest: None,
            preparation_network: None,
            process_guards: None,
            provider: None,
            realized_capabilities: None,
            resources: None,
            runtime: None,
            session_model: None,
            workload: None,
        }
    }
    /// Record a required sandbox policy without persisting its path grants.
    pub fn required(policy: &EffectiveSandboxPolicy, capabilities: SandboxCapabilities) -> Self {
        Self {
            minimum_isolation: Some(policy.policy.minimum_isolation),
            mode: "required".to_string(),
            policy_digest: Some(policy.digest.clone()),
            preparation_network: Some(policy.policy.preparation_network),
            process_guards: Some(policy.policy.process_guards.clone()),
            provider: Some(policy.provider),
            realized_capabilities: Some(capabilities),
            resources: Some(policy.policy.resources.clone()),
            runtime: policy.runtime,
            session_model: Some("per_input".to_string()),
            workload: Some(policy.workload.clone()),
        }
    }
}
impl From<ExecutionState> for Style {
    fn from(state: ExecutionState) -> Self {
        match state {
            | ExecutionState::Cancelled => Self::new().yellow(),
            | ExecutionState::Failed => Self::new().red().bold(),
            | ExecutionState::NotStarted | ExecutionState::Skipped | ExecutionState::SkippedExisting => Self::new().dimmed(),
            | ExecutionState::Pending | ExecutionState::Running => Self::new().cyan(),
            | ExecutionState::Succeeded => Self::new().green().bold(),
        }
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
fn console_cell(value: &str, maximum: usize) -> String {
    let normalized = value
        .chars()
        .map(|character| match character.is_control() {
            | true => ' ',
            | false => character,
        })
        .collect::<String>();
    match normalized.chars().count() <= maximum {
        | true => normalized,
        | false => format!("{}…", normalized.chars().take(maximum.saturating_sub(1)).collect::<String>()),
    }
}
fn console_pad(value: &str, width: usize) -> String {
    format!("{value}{}", " ".repeat(width.saturating_sub(value.chars().count())))
}
fn console_run_status(status: RunStatus, color: bool) -> String {
    console_text(status, status.style(), color)
}
fn console_table_border(output: &mut impl Write, widths: &[usize; 4], left: &str, middle: &str, right: &str, color: bool) -> io::Result<()> {
    let segments = widths.iter().map(|width| "─".repeat(width.saturating_add(2))).collect::<Vec<_>>();
    writeln!(
        output,
        "{}",
        console_text(format!("{left}{}{right}", segments.join(middle)), Style::new().dimmed(), color)
    )
}
fn console_text(value: impl fmt::Display, style: Style, color: bool) -> String {
    match color {
        | true => style.style(value).to_string(),
        | false => value.to_string(),
    }
}
fn render_console_workflow_table(output: &mut impl Write, inputs: &[InputSummary], color: bool) -> io::Result<()> {
    const DETAILS_WIDTH: usize = 48;
    const INPUT_WIDTH: usize = 24;
    const MODULE_WIDTH: usize = 44;
    let rows = inputs
        .iter()
        .flat_map(|input| {
            let name = console_cell(input.value.as_deref().unwrap_or("<no input>"), INPUT_WIDTH);
            let input_row = (
                name,
                "<input>".to_string(),
                input.state,
                console_cell(input.error.as_deref().unwrap_or_default(), DETAILS_WIDTH),
                true,
            );
            once(input_row)
                .chain(input.modules.iter().map(|module| {
                    (
                        String::new(),
                        console_cell(&module.module_path.join("::"), MODULE_WIDTH),
                        module.state,
                        console_cell(module.error.as_deref().unwrap_or_default(), DETAILS_WIDTH),
                        false,
                    )
                }))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let headers = ["Input", "Module", "State", "Details"];
    let widths = rows.iter().fold(headers.map(str::len), |widths, (input, module, state, details, _)| {
        [
            widths[0].max(input.chars().count()),
            widths[1].max(module.chars().count()),
            widths[2].max(state.to_string().chars().count()),
            widths[3].max(details.chars().count()),
        ]
    });
    writeln!(output, "{}", console_text("Workflow", Style::new().cyan().bold(), color))
        .and_then(|()| console_table_border(output, &widths, "╭", "┬", "╮", color))
        .and_then(|()| {
            writeln!(
                output,
                "│ {} │ {} │ {} │ {} │",
                console_text(console_pad(headers[0], widths[0]), Style::new().bold(), color),
                console_text(console_pad(headers[1], widths[1]), Style::new().bold(), color),
                console_text(console_pad(headers[2], widths[2]), Style::new().bold(), color),
                console_text(console_pad(headers[3], widths[3]), Style::new().bold(), color),
            )
        })
        .and_then(|()| console_table_border(output, &widths, "├", "┼", "┤", color))
        .and_then(|()| {
            rows.iter().try_for_each(|(input, module, state, details, input_row)| {
                let input_style = match input_row {
                    | true => Style::new().bold(),
                    | false => Style::new(),
                };
                let module_style = match input_row {
                    | true => Style::new().dimmed().italic(),
                    | false => Style::new(),
                };
                writeln!(
                    output,
                    "│ {} │ {} │ {} │ {} │",
                    console_text(console_pad(input, widths[0]), input_style, color),
                    console_text(console_pad(module, widths[1]), module_style, color),
                    console_text(console_pad(&state.to_string(), widths[2]), Style::from(*state), color),
                    console_text(
                        console_pad(details, widths[3]),
                        match details.is_empty() {
                            | true => Style::new().dimmed(),
                            | false => Style::new().red(),
                        },
                        color,
                    ),
                )
            })
        })
        .and_then(|()| console_table_border(output, &widths, "╰", "┴", "╯", color))
}
fn timestamp() -> Milliseconds {
    Milliseconds::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    )
}
fn validate_container_transition(current: ContainerState, next: ContainerState) -> Result<(), JournalError> {
    let valid = match current {
        | ContainerState::NotStarted => next.is_ready() || next.is_external() || next.is_failed(),
        | ContainerState::Ready => next.is_removed() || next.is_retained() || next.is_failed(),
        | ContainerState::Removed | ContainerState::Retained | ContainerState::External | ContainerState::Failed => false,
    };
    match valid {
        | true => Ok(()),
        | false => Err(JournalError::Invalid(format!(
            "invalid container transition from {current:?} to {next:?}"
        ))),
    }
}
fn validate_gpu_inventory(devices: &[GpuDevice]) -> Result<(), JournalError> {
    let (_, _, duplicate_index, duplicate_uuid) = devices.iter().fold(
        (BTreeSet::new(), BTreeSet::new(), false, false),
        |(mut indices, mut uuids, duplicate_index, duplicate_uuid), device| {
            let unique_index = indices.insert(device.index);
            let unique_uuid = uuids.insert(device.uuid.as_str());
            (indices, uuids, duplicate_index || !unique_index, duplicate_uuid || !unique_uuid)
        },
    );
    match (devices.is_empty(), duplicate_index, duplicate_uuid) {
        | (true, _, _) => Err(JournalError::Invalid("GPU inventory must not be empty".to_string())),
        | (false, true, _) => Err(JournalError::Invalid("GPU inventory must contain unique indices".to_string())),
        | (false, false, true) => Err(JournalError::Invalid("GPU inventory must contain unique UUIDs".to_string())),
        | (false, false, false) => Ok(()),
    }
}
fn validate_gpu_readings<'a>(readings: &[GpuReading], device_ids: impl Iterator<Item = &'a String>) -> Result<(), JournalError> {
    let expected = device_ids.map(String::as_str).collect::<BTreeSet<_>>();
    let actual = readings.iter().map(|reading| reading.uuid.as_str()).collect::<BTreeSet<_>>();
    let invalid_value = readings
        .iter()
        .any(|reading| reading.uuid.is_empty() || reading.utilization_percent.is_some_and(|value| value > 100));
    match (readings.len() == actual.len(), actual == expected, invalid_value) {
        | (false, _, _) => Err(JournalError::Invalid("GPU sample contains duplicate UUIDs".to_string())),
        | (true, false, _) => Err(JournalError::Invalid("GPU sample does not match the recorded inventory".to_string())),
        | (true, true, true) => Err(JournalError::Invalid("GPU sample contains an invalid value".to_string())),
        | (true, true, false) => Ok(()),
    }
}
fn validate_transition(current: ExecutionState, next: ExecutionState, kind: &str) -> Result<(), JournalError> {
    let valid = match current {
        | ExecutionState::NotStarted => {
            next.is_pending() || next.is_running() || next.is_failed() || next.is_cancelled() || next.is_skipped() || next.is_skipped_existing()
        }
        | ExecutionState::Pending => next.is_running() || next.is_failed() || next.is_cancelled() || next.is_skipped() || next.is_skipped_existing(),
        | ExecutionState::Running => {
            next.is_succeeded() || next.is_failed() || next.is_cancelled() || next.is_skipped() || next.is_skipped_existing()
        }
        | ExecutionState::Succeeded
        | ExecutionState::Failed
        | ExecutionState::Cancelled
        | ExecutionState::Skipped
        | ExecutionState::SkippedExisting => false,
    };
    match valid {
        | true => Ok(()),
        | false => Err(JournalError::Invalid(format!("invalid {kind} transition from {current:?} to {next:?}"))),
    }
}

#[cfg(test)]
mod tests;
