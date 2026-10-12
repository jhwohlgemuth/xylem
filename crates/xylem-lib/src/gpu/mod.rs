//! Run-wide NVIDIA GPU monitoring through `nvidia-smi`.
extern crate alloc;

use crate::journal::{GpuDevice, GpuReading, JournalError, RunJournal};
use acorn_cmd::CommandSpec;
use acorn_core::time::{Milliseconds, UnitOfTime};
use alloc::sync::Arc;
use core::fmt;
use core::str::FromStr;
use core::time::Duration;
use csv::ReaderBuilder;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::LazyLock;
use std::thread::{Builder, JoinHandle};
use which::which;

const FORMAT_ARGUMENT: &str = "--format=csv,noheader,nounits";
const GPU_QUERY_TIMEOUT_SECONDS: u64 = 5;
/// Fixed interval between GPU samples.
pub static GPU_SAMPLE_INTERVAL_MS: LazyLock<Milliseconds> = LazyLock::new(|| Milliseconds::new(1000));
const NVIDIA_SMI: &str = "nvidia-smi";
const QUERY_ARGUMENT: &str = "--query-gpu=index,uuid,name,memory.total,utilization.gpu,memory.used";

/// GPU monitor startup, persistence, or worker failure.
#[derive(Debug, thiserror::Error)]
pub enum GpuMonitorError {
    /// Durable journal operation failed.
    #[error("record GPU monitoring: {0}")]
    Journal(#[source] JournalError),
    /// `nvidia-smi` was unavailable or returned unusable telemetry.
    #[error("{0}")]
    Runtime(String),
    /// Monitor worker creation or execution failed.
    #[error("GPU monitor worker failed: {0}")]
    Thread(String),
}
/// Active run-wide GPU monitor.
#[derive(Debug)]
pub struct GpuMonitor {
    stop: Sender<()>,
    worker: Option<JoinHandle<Result<(), GpuMonitorError>>>,
}
#[derive(Debug)]
struct GpuQuery {
    devices: Vec<GpuDevice>,
    readings: Vec<GpuReading>,
}
impl Drop for GpuMonitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}
impl GpuMonitor {
    /// Start monitoring every NVIDIA GPU visible to `nvidia-smi`.
    pub fn start(journal: Arc<RunJournal>) -> Result<Self, GpuMonitorError> {
        journal
            .start_gpu_monitoring(*GPU_SAMPLE_INTERVAL_MS)
            .map_err(GpuMonitorError::Journal)
            .and_then(|()| {
                which(NVIDIA_SMI)
                    .map_err(|error| format!("GPU monitoring requires {NVIDIA_SMI}: {error}"))
                    .and_then(|program| GpuQuery::try_from(program.as_path()).map(|initial| (program, initial)))
                    .map_err(|message| journal.gpu_monitor_unavailable(message))
            })
            .and_then(|(program, initial)| {
                journal
                    .record_gpu_inventory(initial.devices.clone())
                    .and_then(|()| journal.record_gpu_sample(initial.readings))
                    .map_err(GpuMonitorError::Journal)
                    .and_then(|()| {
                        let (stop, receiver) = mpsc::channel();
                        let devices = initial.devices;
                        let worker_journal = Arc::clone(&journal);
                        Builder::new()
                            .name("xylem-gpu-monitor".to_string())
                            .spawn(move || monitor(program, devices, worker_journal, receiver))
                            .map(|worker| Self { stop, worker: Some(worker) })
                            .map_err(|error| {
                                let message = format!("start GPU monitor worker: {error}");
                                match journal.finish_gpu_monitoring(Some(&message)) {
                                    | Ok(()) => GpuMonitorError::Thread(message),
                                    | Err(journal_error) => GpuMonitorError::Journal(journal_error),
                                }
                            })
                    })
            })
    }
    /// Stop monitoring after taking a final boundary sample.
    pub fn stop(mut self) -> Result<(), GpuMonitorError> {
        let _ = self.stop.send(());
        self.worker.take().map_or(Ok(()), |worker| {
            worker
                .join()
                .map_err(|_| GpuMonitorError::Thread("worker panicked".to_string()))
                .and_then(|result| result)
        })
    }
}
impl GpuQuery {
    fn record(self, devices: &[GpuDevice], journal: &RunJournal) -> Result<(), GpuMonitorError> {
        match self.devices == devices {
            | true => journal.record_gpu_sample(self.readings).map_err(GpuMonitorError::Journal),
            | false => Err(GpuMonitorError::Runtime("NVIDIA GPU inventory changed during monitoring".to_string())),
        }
    }
}
impl TryFrom<&Path> for GpuQuery {
    type Error = String;

    fn try_from(program: &Path) -> Result<Self, Self::Error> {
        CommandSpec::new(program.as_os_str())
            .args([QUERY_ARGUMENT, FORMAT_ARGUMENT])
            .output_with_timeout(Duration::from_secs(GPU_QUERY_TIMEOUT_SECONDS))
            .map_err(|error| format!("run {NVIDIA_SMI}: {error}"))
            .and_then(|output| match output.status.success() {
                | true => parse_query(&output.stdout),
                | false => {
                    let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    match details.is_empty() {
                        | true => Err(format!("{NVIDIA_SMI} exited with {}", output.status)),
                        | false => Err(format!("{NVIDIA_SMI} exited with {}: {details}", output.status)),
                    }
                }
            })
    }
}
impl RunJournal {
    fn gpu_monitor_unavailable(&self, message: String) -> GpuMonitorError {
        match self.finish_gpu_monitoring(Some(&message)) {
            | Ok(()) => GpuMonitorError::Runtime(message),
            | Err(error) => GpuMonitorError::Journal(error),
        }
    }
}
fn monitor(program: PathBuf, devices: Vec<GpuDevice>, journal: Arc<RunJournal>, stop: Receiver<()>) -> Result<(), GpuMonitorError> {
    let interval = Duration::from(*GPU_SAMPLE_INTERVAL_MS);
    loop {
        match stop.recv_timeout(interval) {
            | Err(RecvTimeoutError::Timeout) => match GpuQuery::try_from(program.as_path())
                .map_err(GpuMonitorError::Runtime)
                .and_then(|sample| sample.record(&devices, &journal))
            {
                | Ok(()) => (),
                | Err(GpuMonitorError::Runtime(message)) => break journal.finish_gpu_monitoring(Some(&message)).map_err(GpuMonitorError::Journal),
                | Err(error) => break Err(error),
            },
            | Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                break match GpuQuery::try_from(program.as_path())
                    .map_err(GpuMonitorError::Runtime)
                    .and_then(|sample| sample.record(&devices, &journal))
                {
                    | Ok(()) => journal.finish_gpu_monitoring(None).map_err(GpuMonitorError::Journal),
                    | Err(GpuMonitorError::Runtime(message)) => journal.finish_gpu_monitoring(Some(&message)).map_err(GpuMonitorError::Journal),
                    | Err(error) => Err(error),
                }
            }
        }
    }
}
fn parse_optional<T>(value: &str, field: &str) -> Result<Option<T>, String>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let value = value.trim();
    match value {
        | "" | "-" | "N/A" | "[N/A]" => Ok(None),
        | _ => value
            .parse::<T>()
            .map(Some)
            .map_err(|error| format!("parse {field} value {value:?}: {error}")),
    }
}
fn parse_query(output: &[u8]) -> Result<GpuQuery, String> {
    let rows = ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(output)
        .records()
        .map(|record| {
            record
                .map_err(|error| format!("parse {NVIDIA_SMI} CSV: {error}"))
                .and_then(|record| match record.len() {
                    | 6 => {
                        let uuid = record[1].trim().to_string();
                        let name = record[2].trim().to_string();
                        parse_optional::<u32>(&record[0], "GPU index")
                            .and_then(|index| index.ok_or_else(|| "GPU index is unavailable".to_string()))
                            .and_then(|index| {
                                parse_optional::<u64>(&record[3], "total GPU memory").and_then(|memory_total_mib| {
                                    parse_optional::<u8>(&record[4], "GPU utilization").and_then(|utilization_percent| {
                                        match utilization_percent.is_some_and(|value| value > 100) {
                                            | true => Err("GPU utilization exceeds 100 percent".to_string()),
                                            | false => parse_optional::<u64>(&record[5], "used GPU memory").map(|memory_used_mib| {
                                                (
                                                    GpuDevice {
                                                        index,
                                                        name,
                                                        memory_total_mib,
                                                        uuid: uuid.clone(),
                                                    },
                                                    GpuReading {
                                                        memory_used_mib,
                                                        utilization_percent,
                                                        uuid,
                                                    },
                                                )
                                            }),
                                        }
                                    })
                                })
                            })
                    }
                    | fields => Err(format!("expected 6 {NVIDIA_SMI} CSV fields, found {fields}")),
                })
        })
        .collect::<Result<Vec<_>, _>>();
    rows.and_then(|mut rows| match rows.is_empty() {
        | true => Err(format!("{NVIDIA_SMI} returned no GPUs")),
        | false => {
            rows.sort_by(|(left, _), (right, _)| (left.index, &left.uuid).cmp(&(right.index, &right.uuid)));
            let (devices, readings) = rows.into_iter().unzip();
            Ok(GpuQuery { devices, readings })
        }
    })
}
#[cfg(test)]
mod tests;
