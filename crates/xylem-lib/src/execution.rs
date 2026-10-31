//! Execution limits and the scoped worker pool used by Xylem orchestration
extern crate alloc;

use crate::config::FailurePolicy;
use crate::memory::{MemoryAdmissionError, MemoryAdmissionQueue};
use crate::run::RunCancellation;
use acorn_schema::hardware::Memory;
use alloc::collections::VecDeque;
use core::fmt;
use core::num::NonZeroUsize;
use core::str::FromStr;
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::sync::{Condvar, Mutex, PoisonError};

/// Default maximum number of Rayon workers used by one Xylem run.
pub const DEFAULT_THREADS: usize = 10;
/// Classification returned by an input operation to the dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DispatchClassification {
    /// The input completed successfully.
    Succeeded,
    /// The input completed unsuccessfully.
    Failed,
    /// The input stopped because cancellation was requested.
    Cancelled,
}
/// Ordered disposition of one configured input.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DispatchOutcome<T> {
    /// The input was cancelled, with an output when execution had started.
    Cancelled(Option<T>),
    /// The input completed unsuccessfully.
    Failed(T),
    /// The input was never admitted after fail-fast stopped the run.
    NotStarted,
    /// The input completed successfully.
    Succeeded(T),
}
/// Reason input admission stopped before every configured input began.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DispatchStopReason {
    /// Cancellation was requested by the run caller.
    ExternalCancellation,
    /// Fail-fast stopped admission after the identified input failed.
    Failure {
        /// Configured position of the input that triggered fail-fast.
        input_index: usize,
    },
}
/// Ordered report returned by policy-aware input dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use]
pub struct DispatchReport<T> {
    admission_stopped_early: bool,
    effective_concurrency: usize,
    outcomes: Vec<DispatchOutcome<T>>,
    stop_reason: Option<DispatchStopReason>,
}
/// A reusable, run-local execution boundary.
///
/// Keeping the pool local avoids conflicts with applications that have
/// already initialized Rayon's global pool.
pub struct ExecutionContext {
    limits: ExecutionLimits,
    pool: ThreadPool,
}
/// Limits applied to Xylem-owned work during a run.
///
/// This limit controls Rayon workers used by Xylem. It does not constrain
/// threads created by child commands or third-party runtimes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    threads: NonZeroUsize,
}
/// Positive ceiling on concurrently active top-level workflow inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct InputBatch(NonZeroUsize);
/// Error returned when an input batch limit is zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidInputBatch;
/// Error returned when a thread limit is zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidThreadCount;
/// Failure while constructing or running memory-aware input dispatch.
#[derive(Debug, thiserror::Error)]
pub enum MemoryDispatchError {
    /// The memory admission queue rejected its budget or reservations.
    #[error(transparent)]
    Admission(MemoryAdmissionError),
    /// The input and reservation vectors do not describe the same work.
    #[error("memory plan contains {reservations} reservations for {inputs} inputs")]
    ReservationCount {
        /// Number of workflow inputs.
        inputs: usize,
        /// Number of planned reservations.
        reservations: usize,
    },
    /// The scoped input worker pool could not be constructed.
    #[error(transparent)]
    ThreadPool(ThreadPoolBuildError),
}
impl<T> DispatchOutcome<T> {
    /// Borrow the operation output when execution began.
    pub fn output(&self) -> Option<&T> {
        match self {
            | Self::Cancelled(output) => output.as_ref(),
            | Self::Failed(output) | Self::Succeeded(output) => Some(output),
            | Self::NotStarted => None,
        }
    }
    /// Return whether this input completed successfully.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded(_))
    }
    fn into_output(self) -> Option<T> {
        match self {
            | Self::Cancelled(output) => output,
            | Self::Failed(output) | Self::Succeeded(output) => Some(output),
            | Self::NotStarted => None,
        }
    }
}
impl<T> DispatchReport<T> {
    /// Return whether admission stopped before every input began.
    pub fn admission_stopped_early(&self) -> bool {
        self.admission_stopped_early
    }
    /// Return the effective maximum number of concurrently active inputs.
    pub fn effective_concurrency(&self) -> usize {
        self.effective_concurrency
    }
    /// Consume this report and return ordered input dispositions.
    pub fn into_outcomes(self) -> Vec<DispatchOutcome<T>> {
        self.outcomes
    }
    /// Return whether every configured input completed successfully.
    pub fn is_success(&self) -> bool {
        self.outcomes.iter().all(DispatchOutcome::is_success)
    }
    /// Borrow ordered input dispositions.
    pub fn outcomes(&self) -> &[DispatchOutcome<T>] {
        &self.outcomes
    }
    /// Return why admission stopped, when it did.
    pub fn stop_reason(&self) -> Option<DispatchStopReason> {
        self.stop_reason
    }
}
impl ExecutionContext {
    /// Build a run-local worker pool using the supplied limits.
    pub fn new(limits: ExecutionLimits) -> Result<Self, ThreadPoolBuildError> {
        ThreadPoolBuilder::new()
            .num_threads(limits.threads())
            .thread_name(|index| format!("xylem-worker-{index}"))
            .build()
            .map(|pool| Self { limits, pool })
    }
    /// Run an operation inside this context's worker pool.
    pub fn install<OP, R>(&self, operation: OP) -> R
    where
        OP: FnOnce() -> R + Send,
        R: Send,
    {
        self.pool.install(operation)
    }
    /// Return the limits used to construct this context.
    pub fn limits(&self) -> ExecutionLimits {
        self.limits
    }
    /// Return the effective top-level input concurrency for this run.
    pub fn input_concurrency(&self, batch: Option<InputBatch>) -> usize {
        batch.map_or_else(|| self.limits.threads(), |batch| batch.get().min(self.limits.threads()))
    }
    /// Apply an operation to each input with bounded rolling admission.
    ///
    /// Results retain input order even when operations finish out of order.
    pub fn dispatch_inputs<I, OUTPUT, OPERATION>(
        &self,
        inputs: &[I],
        batch: Option<InputBatch>,
        operation: OPERATION,
    ) -> Result<Vec<OUTPUT>, ThreadPoolBuildError>
    where
        I: Sync,
        OUTPUT: Send,
        OPERATION: Fn(&I) -> OUTPUT + Send + Sync,
    {
        let cancellation = RunCancellation::default();
        let report = self.dispatch_inputs_report(
            inputs,
            batch,
            FailurePolicy::Continue,
            &cancellation,
            |_, input| operation(input),
            |_| DispatchClassification::Succeeded,
        );
        Ok(report.into_outcomes().into_iter().filter_map(DispatchOutcome::into_output).collect())
    }
    /// Apply an operation with bounded rolling admission and stop after the first failed output.
    ///
    /// Work admitted before the failure is allowed to settle.
    /// Results retain input order, and entries that were never admitted remain `None`.
    pub fn dispatch_inputs_fail_fast<I, OUTPUT, OPERATION, FAILED>(
        &self,
        inputs: &[I],
        batch: Option<InputBatch>,
        operation: OPERATION,
        failed: FAILED,
    ) -> Result<Vec<Option<OUTPUT>>, ThreadPoolBuildError>
    where
        I: Sync,
        OUTPUT: Send,
        OPERATION: Fn(usize, &I) -> OUTPUT + Send + Sync,
        FAILED: Fn(&OUTPUT) -> bool + Send + Sync,
    {
        let cancellation = RunCancellation::default();
        let report = self.dispatch_inputs_report(inputs, batch, FailurePolicy::FailFast, &cancellation, operation, |output| {
            match failed(output) {
                | true => DispatchClassification::Failed,
                | false => DispatchClassification::Succeeded,
            }
        });
        Ok(report.into_outcomes().into_iter().map(DispatchOutcome::into_output).collect())
    }
    /// Apply an operation using FIFO memory admission and stop after the first failed output.
    ///
    /// Work admitted before a failure is allowed to settle.
    /// Results retain input order, and entries that were never admitted remain `None`.
    pub fn dispatch_inputs_fail_fast_memory<I, OUTPUT, OPERATION, FAILED>(
        &self,
        inputs: &[I],
        batch: Option<InputBatch>,
        budget: Memory,
        reservations: Vec<Memory>,
        operation: OPERATION,
        failed: FAILED,
    ) -> Result<Vec<Option<OUTPUT>>, MemoryDispatchError>
    where
        I: Sync,
        OUTPUT: Send,
        OPERATION: Fn(usize, &I) -> OUTPUT + Send + Sync,
        FAILED: Fn(&OUTPUT) -> bool + Send + Sync,
    {
        let cancellation = RunCancellation::default();
        self.dispatch_inputs_report_memory(
            inputs,
            batch,
            budget,
            reservations,
            FailurePolicy::FailFast,
            &cancellation,
            operation,
            |output| match failed(output) {
                | true => DispatchClassification::Failed,
                | false => DispatchClassification::Succeeded,
            },
        )
        .map(|report| report.into_outcomes().into_iter().map(DispatchOutcome::into_output).collect())
    }
    /// Apply policy-aware rolling admission and return one ordered disposition per input.
    pub fn dispatch_inputs_report<I, OUTPUT, OPERATION, CLASSIFY>(
        &self,
        inputs: &[I],
        batch: Option<InputBatch>,
        failure_policy: FailurePolicy,
        cancellation: &RunCancellation,
        operation: OPERATION,
        classify: CLASSIFY,
    ) -> DispatchReport<OUTPUT>
    where
        I: Sync,
        OUTPUT: Send,
        OPERATION: Fn(usize, &I) -> OUTPUT + Send + Sync,
        CLASSIFY: Fn(&OUTPUT) -> DispatchClassification + Send + Sync,
    {
        struct AdmissionState {
            next: usize,
            stop_reason: Option<DispatchStopReason>,
        }
        let workers = self.input_concurrency(batch).min(inputs.len().max(1));
        let admission = Mutex::new(AdmissionState { next: 0, stop_reason: None });
        let outputs = (0..inputs.len()).map(|_| Mutex::new(None)).collect::<Vec<_>>();
        self.pool.scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|_| loop {
                    let index = {
                        let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                        if state.stop_reason.is_none() && cancellation.is_requested() {
                            state.stop_reason = Some(DispatchStopReason::ExternalCancellation);
                        }
                        match (state.stop_reason, state.next < inputs.len()) {
                            | (None, true) => {
                                let index = state.next;
                                state.next = state.next.saturating_add(1);
                                Some(index)
                            }
                            | (None, false) | (Some(_), _) => None,
                        }
                    };
                    let Some(index) = index else {
                        break;
                    };
                    let Some((input, output_slot)) = inputs.get(index).zip(outputs.get(index)) else {
                        break;
                    };
                    let output = operation(index, input);
                    let classification = classify(&output);
                    let outcome = match classification {
                        | DispatchClassification::Cancelled => DispatchOutcome::Cancelled(Some(output)),
                        | DispatchClassification::Failed => DispatchOutcome::Failed(output),
                        | DispatchClassification::Succeeded => DispatchOutcome::Succeeded(output),
                    };
                    *output_slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
                    let cancel_siblings = {
                        let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                        if state.stop_reason.is_none() && cancellation.is_requested() {
                            state.stop_reason = Some(DispatchStopReason::ExternalCancellation);
                        }
                        match (state.stop_reason, classification, failure_policy) {
                            | (None, DispatchClassification::Failed, FailurePolicy::FailFast) => {
                                state.stop_reason = Some(DispatchStopReason::Failure { input_index: index });
                                true
                            }
                            | _ => false,
                        }
                    };
                    if cancel_siblings {
                        cancellation.force();
                    }
                });
            }
        });
        let state = admission.into_inner().unwrap_or_else(PoisonError::into_inner);
        let stop_reason = state.stop_reason;
        let outcomes = outputs
            .into_iter()
            .map(|output| output.into_inner().unwrap_or_else(PoisonError::into_inner))
            .map(|output| {
                output.unwrap_or_else(|| match stop_reason {
                    | Some(DispatchStopReason::ExternalCancellation) => DispatchOutcome::Cancelled(None),
                    | None | Some(DispatchStopReason::Failure { .. }) => DispatchOutcome::NotStarted,
                })
            })
            .collect::<Vec<_>>();
        DispatchReport {
            admission_stopped_early: state.next < inputs.len(),
            effective_concurrency: workers,
            outcomes,
            stop_reason,
        }
    }
    /// Apply policy-aware FIFO memory admission and return ordered input dispositions.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_inputs_report_memory<I, OUTPUT, OPERATION, CLASSIFY>(
        &self,
        inputs: &[I],
        batch: Option<InputBatch>,
        budget: Memory,
        reservations: Vec<Memory>,
        failure_policy: FailurePolicy,
        cancellation: &RunCancellation,
        operation: OPERATION,
        classify: CLASSIFY,
    ) -> Result<DispatchReport<OUTPUT>, MemoryDispatchError>
    where
        I: Sync,
        OUTPUT: Send,
        OPERATION: Fn(usize, &I) -> OUTPUT + Send + Sync,
        CLASSIFY: Fn(&OUTPUT) -> DispatchClassification + Send + Sync,
    {
        struct AdmissionState {
            error: Option<MemoryAdmissionError>,
            queue: MemoryAdmissionQueue,
            ready: VecDeque<usize>,
            stop_reason: Option<DispatchStopReason>,
        }
        match inputs.len() == reservations.len() {
            | false => Err(MemoryDispatchError::ReservationCount {
                inputs: inputs.len(),
                reservations: reservations.len(),
            }),
            | true => {
                let workers = self.input_concurrency(batch).min(inputs.len().max(1));
                let queue_batch = InputBatch(NonZeroUsize::new(workers).unwrap_or(NonZeroUsize::MIN));
                MemoryAdmissionQueue::new(budget, queue_batch, reservations)
                    .map_err(MemoryDispatchError::Admission)
                    .and_then(|mut queue| {
                        let ready = queue.admit().into_iter().map(|admission| admission.index()).collect();
                        let admission = Mutex::new(AdmissionState {
                            error: None,
                            queue,
                            ready,
                            stop_reason: None,
                        });
                        let available = Condvar::new();
                        let outputs = (0..inputs.len()).map(|_| Mutex::new(None)).collect::<Vec<_>>();
                        self.pool.scope(|scope| {
                            for _ in 0..workers {
                                scope.spawn(|_| loop {
                                    let index = {
                                        let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                                        loop {
                                            if state.stop_reason.is_none() && cancellation.is_requested() {
                                                state.stop_reason = Some(DispatchStopReason::ExternalCancellation);
                                            }
                                            match (state.stop_reason, state.ready.pop_front()) {
                                                | (None, Some(index)) => break Some(index),
                                                | (Some(_), _) => break None,
                                                | (None, None) if state.error.is_some() || state.queue.is_complete() => break None,
                                                | (None, None) => {
                                                    state = available.wait(state).unwrap_or_else(PoisonError::into_inner);
                                                }
                                            }
                                        }
                                    };
                                    let Some(index) = index else {
                                        break;
                                    };
                                    let Some((input, output_slot)) = inputs.get(index).zip(outputs.get(index)) else {
                                        break;
                                    };
                                    let output = operation(index, input);
                                    let classification = classify(&output);
                                    let outcome = match classification {
                                        | DispatchClassification::Cancelled => DispatchOutcome::Cancelled(Some(output)),
                                        | DispatchClassification::Failed => DispatchOutcome::Failed(output),
                                        | DispatchClassification::Succeeded => DispatchOutcome::Succeeded(output),
                                    };
                                    *output_slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
                                    let cancel_siblings = {
                                        let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                                        if state.stop_reason.is_none() && cancellation.is_requested() {
                                            state.stop_reason = Some(DispatchStopReason::ExternalCancellation);
                                        }
                                        let cancel_siblings = match (state.stop_reason, classification, failure_policy) {
                                            | (None, DispatchClassification::Failed, FailurePolicy::FailFast) => {
                                                state.stop_reason = Some(DispatchStopReason::Failure { input_index: index });
                                                true
                                            }
                                            | _ => false,
                                        };
                                        let released = match state.stop_reason {
                                            | Some(_) => state.queue.release_only(index).map(|()| Vec::new()),
                                            | None => state.queue.release(index),
                                        };
                                        match released {
                                            | Ok(admissions) => state.ready.extend(admissions.into_iter().map(|admission| admission.index())),
                                            | Err(error) => state.error = Some(error),
                                        }
                                        available.notify_all();
                                        cancel_siblings
                                    };
                                    if cancel_siblings {
                                        cancellation.force();
                                    }
                                });
                            }
                        });
                        let state = admission.into_inner().unwrap_or_else(PoisonError::into_inner);
                        match state.error {
                            | Some(error) => Err(MemoryDispatchError::Admission(error)),
                            | None => {
                                let stop_reason = state.stop_reason;
                                let outcomes = outputs
                                    .into_iter()
                                    .map(|output| output.into_inner().unwrap_or_else(PoisonError::into_inner))
                                    .map(|output| {
                                        output.unwrap_or_else(|| match stop_reason {
                                            | Some(DispatchStopReason::ExternalCancellation) => DispatchOutcome::Cancelled(None),
                                            | None | Some(DispatchStopReason::Failure { .. }) => DispatchOutcome::NotStarted,
                                        })
                                    })
                                    .collect::<Vec<_>>();
                                let admission_stopped_early = outcomes
                                    .iter()
                                    .any(|outcome| matches!(outcome, DispatchOutcome::NotStarted | DispatchOutcome::Cancelled(None)));
                                Ok(DispatchReport {
                                    admission_stopped_early,
                                    effective_concurrency: workers,
                                    outcomes,
                                    stop_reason,
                                })
                            }
                        }
                    })
            }
        }
    }
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            threads: NonZeroUsize::new(DEFAULT_THREADS).unwrap_or(NonZeroUsize::MIN),
        }
    }
}
impl ExecutionLimits {
    /// Return the maximum number of Xylem Rayon workers.
    pub fn threads(self) -> usize {
        self.threads.get()
    }
}
impl fmt::Display for ExecutionLimits {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.threads.fmt(formatter)
    }
}
impl From<NonZeroUsize> for ExecutionLimits {
    fn from(threads: NonZeroUsize) -> Self {
        Self { threads }
    }
}
impl FromStr for ExecutionLimits {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .parse::<usize>()
            .map_err(|why| format!("invalid thread count: {why}"))
            .and_then(|threads| Self::try_from(threads).map_err(|why| why.to_string()))
    }
}
impl TryFrom<usize> for ExecutionLimits {
    type Error = InvalidThreadCount;

    fn try_from(threads: usize) -> Result<Self, Self::Error> {
        NonZeroUsize::new(threads).map(|threads| Self { threads }).ok_or(InvalidThreadCount)
    }
}
impl fmt::Display for InputBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl InputBatch {
    /// Return the configured input concurrency ceiling.
    pub fn get(self) -> usize {
        self.0.get()
    }
}
impl TryFrom<usize> for InputBatch {
    type Error = InvalidInputBatch;

    fn try_from(batch: usize) -> Result<Self, Self::Error> {
        NonZeroUsize::new(batch).map(Self).ok_or(InvalidInputBatch)
    }
}
impl<'de> Deserialize<'de> for InputBatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        usize::deserialize(deserializer).and_then(|batch| Self::try_from(batch).map_err(D::Error::custom))
    }
}
impl core::error::Error for InvalidInputBatch {}
impl fmt::Display for InvalidInputBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("input batch must be greater than zero")
    }
}
impl core::error::Error for InvalidThreadCount {}
impl fmt::Display for InvalidThreadCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("thread count must be greater than zero")
    }
}
