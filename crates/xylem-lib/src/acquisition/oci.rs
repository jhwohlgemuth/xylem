//! Bounded extraction of verified local OCI artifact trees through ORAS.
use super::git::directory_size;
use super::oci_registry::validate_manifest_bytes;
use super::util::sha256;
use super::{
    args, cmd, thread, AcquisitionError, AcquisitionLimits, Arc, AtomicBool, AtomicU64, CancellationToken, CaptureLimits, Duration, ExecutionOptions,
    Instant, Ordering, Path, Seconds, UnitOfTime, MONITOR_INTERVAL,
};
use std::ffi::OsString;

pub(super) fn pull_tree(reference: &str, destination: &Path, limits: &AcquisitionLimits, started: Instant) -> Result<(), AcquisitionError> {
    let layout = reference.starts_with("oci-layout:");
    let oras_reference = reference
        .strip_prefix("oci://")
        .or_else(|| reference.strip_prefix("oci-layout:"))
        .unwrap_or(reference);
    remaining_timeout(started, limits.git_timeout).and_then(|manifest_timeout| {
        layout
            .then_some(())
            .ok_or_else(|| AcquisitionError::Policy {
                remote: reference.to_string(),
                reason: "ORAS may only read a local OCI layout".to_string(),
            })
            .and_then(|()| {
                run_oras(
                    "oras",
                    "inspect OCI manifest",
                    oras_reference,
                    args!["manifest", "fetch", "--oci-layout", oras_reference],
                    limits,
                    manifest_timeout,
                    destination,
                )
            })
            .and_then(|manifest| {
                oras_reference
                    .rsplit_once('@')
                    .map(|(_, digest)| digest)
                    .ok_or_else(|| AcquisitionError::Identity {
                        value: "OCI artifact".to_string(),
                        reason: "local OCI reference has no digest",
                    })
                    .and_then(|expected| {
                        let calculated = sha256(manifest.as_bytes());
                        match calculated == expected {
                            | true => validate_manifest_bytes(manifest.as_bytes(), limits),
                            | false => Err(AcquisitionError::Sha256Mismatch {
                                calculated,
                                expected: expected.to_string(),
                                remote: reference.to_string(),
                            }),
                        }
                    })
            })
            .and_then(|()| remaining_timeout(started, limits.git_timeout))
            .and_then(|pull_timeout| {
                run_oras(
                    "oras",
                    "pull OCI artifact tree",
                    oras_reference,
                    args!["pull", "--no-tty", "--oci-layout", ("--output", destination.as_os_str()), oras_reference],
                    limits,
                    pull_timeout,
                    destination,
                )
                .map(|_| ())
            })
    })
}
fn remaining_timeout(started: Instant, timeout: Seconds) -> Result<Duration, AcquisitionError> {
    Duration::from(timeout)
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(AcquisitionError::Limit {
            limit: "OCI acquisition seconds",
            maximum: timeout.get(),
            observed: started.elapsed().as_secs(),
        })
}
pub(super) fn run_oras(
    program: &str,
    action: &'static str,
    reference: &str,
    arguments: impl IntoIterator<Item = OsString>,
    limits: &AcquisitionLimits,
    timeout: Duration,
    destination: &Path,
) -> Result<String, AcquisitionError> {
    let cancellation = CancellationToken::new();
    let finished = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicU64::new(0));
    let monitor_finished = Arc::clone(&finished);
    let monitor_observed = Arc::clone(&observed);
    let monitor_cancellation = cancellation.clone();
    let staging = destination.parent().unwrap_or(destination).to_path_buf();
    let maximum_bytes = limits.git_staging.checked_bytes().unwrap_or_default();
    let monitor = thread::spawn(move || {
        while !monitor_finished.load(Ordering::Acquire) {
            if let Ok(size) = directory_size(&staging) {
                let bytes = size.checked_bytes().unwrap_or(u64::MAX);
                monitor_observed.store(bytes, Ordering::Release);
                if bytes > maximum_bytes {
                    monitor_cancellation.cancel();
                    break;
                }
            }
            thread::sleep(MONITOR_INTERVAL);
        }
    });
    let options = ExecutionOptions::new()
        .with_cancellation(cancellation)
        .with_capture_limits(CaptureLimits::new(
            Some(limits.process_output.checked_bytes().unwrap_or_default().min(usize::MAX as u64) as usize),
            None,
        ))
        .with_timeout(timeout);
    let result = cmd!(exec checked program, arguments; opt: &options);
    finished.store(true, Ordering::Release);
    let _ = monitor.join();
    directory_size(destination.parent().unwrap_or(destination)).and_then(|size| {
        let observed = observed.load(Ordering::Acquire).max(size.checked_bytes().unwrap_or(u64::MAX));
        if observed > maximum_bytes {
            Err(AcquisitionError::Limit {
                limit: "OCI staging bytes",
                maximum: maximum_bytes,
                observed,
            })
        } else {
            result
                .map(|result| String::from_utf8_lossy(result.stdout().bytes()).into_owned())
                .map_err(|source| AcquisitionError::Process {
                    action,
                    remote: reference.to_string(),
                    reason: source.to_string(),
                })
        }
    })
}
