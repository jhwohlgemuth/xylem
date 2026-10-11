//! Canonical durable run bundles and history discovery
extern crate alloc;

use crate::journal::{RunStatus, RunSummary, JOURNAL_SCHEMA_VERSION};
use crate::sandbox::{SandboxLease, SandboxLeaseState};
use crate::util::constants::app::{APPLICATION, ORGANIZATION, QUALIFIER};
use crate::util::timestamp;
use acorn_cmd::{OutputSink, OutputStream};
use acorn_core::time::Milliseconds;
use acorn_host::fs::{project_cache_directory, SafePath};
use acorn_schema::hardware::Memory;
use alloc::sync::Arc;
use bon::Builder;
use core::mem;
use core::sync::atomic::{AtomicU64, Ordering};
use derive_more::Display;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{create_dir, create_dir_all, read, read_dir, read_to_string, rename, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sysinfo::{Pid, System};
use thiserror::Error;

/// Current canonical run-bundle format.
pub const RUN_BUNDLE_VERSION: u32 = 1;

/// Optional run-store root override for tests and embedded deployments.
pub const RUN_STORE_ENV: &str = "XYLEM_RUN_STORE";

/// Status exposed by run history before a bundle is opened in detail.
#[derive(Clone, Copy, Debug, Deserialize, Display, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    /// The run was cancelled by the user.
    #[display("cancelled")]
    Cancelled,
    /// The bundle could not be decoded.
    #[display("corrupt")]
    Corrupt,
    /// The run completed unsuccessfully.
    #[display("failed")]
    Failed,
    /// The run has no terminal summary.
    #[display("interrupted")]
    Interrupted,
    /// The bundle belongs to the current live Xylem process.
    #[display("running")]
    Running,
    /// The run completed successfully.
    #[display("succeeded")]
    Succeeded,
    /// The manifest uses an unsupported bundle version.
    #[display("unsupported")]
    Unsupported,
}

#[derive(Clone, Copy, Debug, Display)]
enum LogStream {
    #[display("stderr")]
    Stderr,
    #[display("stdout")]
    Stdout,
}

/// Failure while creating, writing, or discovering run bundles.
#[derive(Debug, Error)]
pub enum StoreError {
    /// A run bundle contains invalid JSON.
    #[error("invalid run bundle JSON at {path}: {source}")]
    Json {
        /// File that could not be decoded.
        path: PathBuf,
        /// JSON failure.
        #[source]
        source: serde_json::Error,
    },
    /// A run-store filesystem operation failed.
    #[error("run-store operation failed at {path}: {source}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// The platform cache directory could not be resolved.
    #[error("resolve Xylem run store: {0}")]
    Resolve(#[from] acorn_host::fs::FsError),
}

/// One writable canonical run bundle.
#[derive(Clone, Debug)]
pub struct Bundle {
    path: PathBuf,
}

/// Immutable identity written before workflow execution begins.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Manifest {
    /// Canonical run-bundle version.
    pub bundle_version: u32,
    /// SHA-256 digest of the selected configuration bytes.
    pub configuration_digest: String,
    /// Selected configuration path without resolved environment values.
    pub configuration_path: String,
    /// Journal schema used by this bundle.
    pub journal_schema_version: u32,
    /// Process identifier that created the bundle.
    pub process_id: u32,
    /// Unique run identifier.
    pub run_id: String,
    /// Unix start timestamp in milliseconds.
    pub started_at: Milliseconds,
    /// Configured Rayon worker ceiling.
    pub threads: usize,
}

/// One lightweight host and process sample.
#[derive(Builder, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[builder(start_fn(name = init))]
pub struct MetricSample {
    /// Host CPU utilization percentage when available.
    pub host_cpu_percent: Option<f32>,
    /// Aggregate disk data read since the previous sample.
    pub host_disk_read: Option<Memory>,
    /// Aggregate disk data written since the previous sample.
    pub host_disk_written: Option<Memory>,
    /// Aggregate filesystem capacity still available.
    pub host_filesystem_available: Option<Memory>,
    /// Aggregate filesystem capacity.
    pub host_filesystem_total: Option<Memory>,
    /// One-minute host load average when available.
    pub host_load_one: Option<f64>,
    /// Host memory used when available.
    pub host_memory_used: Option<Memory>,
    /// Network data received since the previous sample.
    pub host_network_received: Option<Memory>,
    /// Network data transmitted since the previous sample.
    pub host_network_transmitted: Option<Memory>,
    /// Host swap used when available.
    pub host_swap_used: Option<Memory>,
    /// Aggregate Xylem process-tree CPU utilization percentage when available.
    pub process_cpu_percent: Option<f32>,
    /// Number of processes in the Xylem process tree when available.
    pub process_count: Option<u64>,
    /// Xylem process-tree data read since the previous sample.
    pub process_disk_read: Option<Memory>,
    /// Xylem process-tree data written since the previous sample.
    pub process_disk_written: Option<Memory>,
    /// Xylem resident memory when available.
    pub process_memory: Option<Memory>,
    /// Unix timestamp in milliseconds.
    pub timestamp: Milliseconds,
}

/// Root used to create and discover canonical run bundles.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

/// Read-only history entry derived without a shared mutable index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredRun {
    /// Configuration path recorded by the manifest.
    pub configuration_path: String,
    /// Bundle directory.
    pub path: PathBuf,
    /// Unique run identifier.
    pub run_id: String,
    /// Unix start timestamp in milliseconds.
    pub started_at: Milliseconds,
    /// Recovered outer status.
    pub status: BundleStatus,
}

#[derive(Default)]
struct TaskLogBuffers {
    stderr: Vec<u8>,
    stdout: Vec<u8>,
}

struct TaskLogInner {
    buffers: Mutex<TaskLogBuffers>,
    bundle: Bundle,
    path: PathBuf,
    sequence: AtomicU64,
}

/// One sanitized, attributed task-log record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskLogRecord {
    /// Monotonically increasing stream-local sequence.
    pub sequence: u64,
    /// `stdout` or `stderr`.
    pub stream: String,
    /// Sanitized UTF-8 text.
    pub text: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: Milliseconds,
}

/// Cloneable ACORN output sink that writes attributed JSONL task logs.
#[derive(Clone)]
pub struct TaskLogSink {
    inner: Arc<TaskLogInner>,
}

impl Bundle {
    /// Append one JSON record and durably flush it.
    pub fn append_json_line<T>(&self, relative: impl AsRef<Path>, value: &T) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        let path = self.path.join(relative);
        path.parent()
            .map_or(Ok(()), |parent| create_dir_all(parent).map_err(|source| io_error(parent, source)))
            .and_then(|()| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|source| io_error(&path, source))
            })
            .and_then(|file| {
                let mut writer = BufWriter::new(file);
                serde_json::to_writer(&mut writer, value)
                    .map_err(|source| StoreError::Json { path: path.clone(), source })
                    .and_then(|()| writer.write_all(b"\n").map_err(|source| io_error(&path, source)))
                    .and_then(|()| writer.flush().map_err(|source| io_error(&path, source)))
                    .and_then(|()| writer.get_ref().sync_data().map_err(|source| io_error(&path, source)))
            })
    }
    /// Return this bundle's journal path.
    pub fn journal_path(&self) -> PathBuf {
        self.path.join("journal.jsonl")
    }
    /// Return this bundle's root directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Atomically write the terminal recovered summary.
    pub fn write_summary(&self, summary: &RunSummary) -> Result<(), StoreError> {
        write_json_atomic(&self.path.join("summary.json"), summary)
    }
}

impl From<RunStatus> for BundleStatus {
    fn from(status: RunStatus) -> Self {
        match status {
            | RunStatus::Cancelled => Self::Cancelled,
            | RunStatus::Failed => Self::Failed,
            | RunStatus::Interrupted => Self::Interrupted,
            | RunStatus::Succeeded => Self::Succeeded,
        }
    }
}

impl From<OutputStream> for LogStream {
    fn from(stream: OutputStream) -> Self {
        match stream {
            | OutputStream::Stderr => Self::Stderr,
            | OutputStream::Stdout => Self::Stdout,
        }
    }
}

impl Manifest {
    /// Build a manifest from the selected configuration file.
    pub fn from_config(run_id: String, configuration_path: &Path, threads: usize) -> Result<Self, StoreError> {
        read(configuration_path)
            .map_err(|source| io_error(configuration_path, source))
            .map(|bytes| Self {
                bundle_version: RUN_BUNDLE_VERSION,
                configuration_digest: format!("{:x}", Sha256::digest(bytes)),
                configuration_path: configuration_path.display().to_string(),
                journal_schema_version: JOURNAL_SCHEMA_VERSION,
                process_id: std::process::id(),
                run_id,
                started_at: timestamp(),
                threads,
            })
    }
}

impl Store {
    /// Create a run store at an injected root.
    pub fn new(root: PathBuf) -> Result<Self, StoreError> {
        create_dir_all(&root).map_err(|source| io_error(&root, source)).map(|()| Self { root })
    }
    /// Resolve Xylem's canonical platform run store.
    pub fn system() -> Result<Self, StoreError> {
        env::var_os(RUN_STORE_ENV).map_or_else(
            || {
                SafePath::new("runs")
                    .map_err(StoreError::from)
                    .and_then(|namespace| project_cache_directory(QUALIFIER, ORGANIZATION, APPLICATION, &namespace).map_err(StoreError::from))
                    .and_then(Self::new)
            },
            |root| Self::new(PathBuf::from(root)),
        )
    }
    /// Discover every run newest first without a shared index.
    pub fn list(&self) -> Result<Vec<StoredRun>, StoreError> {
        let system = System::new_all();
        read_dir(&self.root)
            .map_err(|source| io_error(&self.root, source))
            .and_then(|entries| {
                entries
                    .map(|entry| entry.map_err(|source| io_error(&self.root, source)))
                    .filter_map(|entry| match entry {
                        | Ok(entry) if entry.path().is_dir() => Some(read_stored_run(entry.path(), &system)),
                        | Ok(_) => None,
                        | Err(why) => Some(Err(why)),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .map(|mut runs| {
                runs.sort_by(|left, right| (right.started_at, &right.run_id).cmp(&(left.started_at, &left.run_id)));
                runs
            })
    }
    /// Recover durable sandbox leases for conservative stale-resource reconciliation.
    pub fn sandbox_leases(&self) -> Result<Vec<SandboxLease>, StoreError> {
        self.list().map(|runs| {
            let mut leases = runs
                .into_iter()
                .map(|run| SandboxLease {
                    run_id: run.run_id,
                    state: match run.status {
                        | BundleStatus::Running => SandboxLeaseState::Active,
                        | BundleStatus::Corrupt | BundleStatus::Unsupported => SandboxLeaseState::Indeterminate,
                        | BundleStatus::Cancelled | BundleStatus::Failed | BundleStatus::Interrupted | BundleStatus::Succeeded => {
                            SandboxLeaseState::Terminal
                        }
                    },
                })
                .collect::<Vec<_>>();
            leases.sort_by(|left, right| left.run_id.cmp(&right.run_id));
            leases
        })
    }
    /// Return the run-store root.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Create a new bundle without overwriting an existing run identifier.
    pub fn start(&self, manifest: &Manifest) -> Result<Bundle, StoreError> {
        let path = self.root.join(&manifest.run_id);
        create_dir(&path)
            .map_err(|source| io_error(&path, source))
            .and_then(|()| create_dir_all(path.join("logs")).map_err(|source| io_error(&path, source)))
            .and_then(|()| write_json_atomic(&path.join("manifest.json"), manifest))
            .map(|()| Bundle { path })
    }
}

impl OutputSink for TaskLogSink {
    fn write(&self, stream: OutputStream, bytes: &[u8]) -> std::io::Result<()> {
        self.inner
            .buffers
            .lock()
            .map_err(|_| std::io::Error::other("task-log buffer lock is poisoned"))
            .and_then(|mut buffers| {
                let buffer = match stream {
                    | OutputStream::Stderr => &mut buffers.stderr,
                    | OutputStream::Stdout => &mut buffers.stdout,
                };
                buffer.extend_from_slice(bytes);
                let split = buffer.iter().rposition(|byte| *byte == b'\n').map(|index| index.saturating_add(1));
                split.map_or(Ok(()), |split| {
                    let complete = buffer.drain(..split).collect::<Vec<_>>();
                    complete
                        .split(|byte| *byte == b'\n')
                        .filter(|line| !line.is_empty())
                        .try_for_each(|line| self.write_line(stream, line))
                })
            })
    }
}

impl TaskLogSink {
    /// Create a sink for one input/module log.
    pub fn module(bundle: Bundle, input_index: usize, module_index: usize) -> Self {
        Self {
            inner: Arc::new(TaskLogInner {
                buffers: Mutex::new(TaskLogBuffers::default()),
                bundle,
                path: PathBuf::from(format!("logs/input-{input_index}/module-{module_index}.jsonl")),
                sequence: AtomicU64::new(0),
            }),
        }
    }
    /// Flush unterminated stream fragments after command completion.
    pub fn flush(&self) -> Result<(), StoreError> {
        self.inner
            .buffers
            .lock()
            .map_err(|_| StoreError::Io {
                path: self.inner.path.clone(),
                source: std::io::Error::other("task-log buffer lock is poisoned"),
            })
            .and_then(|mut buffers| {
                let stderr = mem::take(&mut buffers.stderr);
                let stdout = mem::take(&mut buffers.stdout);
                drop(buffers);
                [(OutputStream::Stdout, stdout), (OutputStream::Stderr, stderr)]
                    .into_iter()
                    .filter(|(_, bytes)| !bytes.is_empty())
                    .try_for_each(|(stream, bytes)| self.write_line(stream, &bytes).map_err(|source| io_error(&self.inner.path, source)))
            })
    }
    fn write_line(&self, stream: OutputStream, bytes: &[u8]) -> std::io::Result<()> {
        let text = String::from_utf8_lossy(bytes)
            .chars()
            .flat_map(|character| match character {
                | '\t' => "\t".chars().collect::<Vec<_>>(),
                | character if character.is_control() => format!("\\u{{{:04x}}}", u32::from(character)).chars().collect(),
                | character => vec![character],
            })
            .collect::<String>();
        let record = TaskLogRecord {
            sequence: self.inner.sequence.fetch_add(1, Ordering::AcqRel),
            stream: LogStream::from(stream).to_string(),
            text,
            timestamp: timestamp(),
        };
        self.inner
            .bundle
            .append_json_line(&self.inner.path, &record)
            .map_err(std::io::Error::other)
    }
}

fn io_error(path: impl AsRef<Path>, source: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.as_ref().to_path_buf(),
        source,
    }
}

fn read_stored_run(path: PathBuf, system: &System) -> Result<StoredRun, StoreError> {
    let manifest_path = path.join("manifest.json");
    let manifest = read_to_string(&manifest_path)
        .map_err(|source| io_error(&manifest_path, source))
        .and_then(|content| serde_json::from_str::<Manifest>(&content).map_err(|source| StoreError::Json { path: manifest_path, source }));
    match manifest {
        | Ok(manifest) => {
            let status = match manifest.bundle_version == RUN_BUNDLE_VERSION && manifest.journal_schema_version == JOURNAL_SCHEMA_VERSION {
                | false => BundleStatus::Unsupported,
                | true => match read_to_string(path.join("summary.json")) {
                    | Ok(content) => serde_json::from_str::<RunSummary>(&content).map_or(BundleStatus::Corrupt, |summary| {
                        match summary.schema_version == JOURNAL_SCHEMA_VERSION {
                            | false => BundleStatus::Corrupt,
                            | true => summary.status.into(),
                        }
                    }),
                    | Err(source) if source.kind() == std::io::ErrorKind::NotFound && system.process(Pid::from_u32(manifest.process_id)).is_some() => {
                        BundleStatus::Running
                    }
                    | Err(source) if source.kind() == std::io::ErrorKind::NotFound => BundleStatus::Interrupted,
                    | Err(_) => BundleStatus::Corrupt,
                },
            };
            Ok(StoredRun {
                configuration_path: manifest.configuration_path,
                path,
                run_id: manifest.run_id,
                started_at: manifest.started_at,
                status,
            })
        }
        | Err(_) => Ok(StoredRun {
            configuration_path: String::new(),
            run_id: path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            started_at: Milliseconds::default(),
            status: BundleStatus::Corrupt,
            path,
        }),
    }
}

fn write_json_atomic<T>(path: &Path, value: &T) -> Result<(), StoreError>
where
    T: Serialize,
{
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    File::create(&temporary)
        .map_err(|source| io_error(&temporary, source))
        .and_then(|file| {
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, value)
                .map_err(|source| StoreError::Json {
                    path: temporary.clone(),
                    source,
                })
                .and_then(|()| writer.write_all(b"\n").map_err(|source| io_error(&temporary, source)))
                .and_then(|()| writer.flush().map_err(|source| io_error(&temporary, source)))
                .and_then(|()| writer.get_ref().sync_all().map_err(|source| io_error(&temporary, source)))
        })
        .and_then(|()| rename(&temporary, path).map_err(|source| io_error(path, source)))
}
