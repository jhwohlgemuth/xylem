//! Execution limits and the scoped worker pool used by Xylem orchestration
extern crate alloc;

use crate::memory::{MemoryAdmissionError, MemoryAdmissionQueue};
use acorn_schema::hardware::Memory;
use alloc::collections::VecDeque;
use core::fmt;
use core::num::NonZeroUsize;
use core::str::FromStr;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::sync::{Condvar, Mutex, PoisonError};

/// Default maximum number of Rayon workers used by one Xylem run.
pub const DEFAULT_THREADS: usize = 10;
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
        ThreadPoolBuilder::new()
            .num_threads(self.input_concurrency(batch))
            .thread_name(|index| format!("xylem-input-{index}"))
            .build()
            .map(|pool| pool.install(|| inputs.par_iter().map(operation).collect()))
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
        struct Admission {
            next: usize,
            stopped: bool,
        }
        let workers = self.input_concurrency(batch).min(inputs.len().max(1));
        ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(|index| format!("xylem-input-{index}"))
            .build()
            .map(|pool| {
                let admission = Mutex::new(Admission { next: 0, stopped: false });
                let outputs = (0..inputs.len()).map(|_| Mutex::new(None)).collect::<Vec<_>>();
                pool.scope(|scope| {
                    for _ in 0..workers {
                        scope.spawn(|_| loop {
                            let index = {
                                let mut admission = admission.lock().unwrap_or_else(PoisonError::into_inner);
                                if admission.stopped || admission.next >= inputs.len() {
                                    break;
                                }
                                let index = admission.next;
                                admission.next = admission.next.saturating_add(1);
                                index
                            };
                            let Some((input, output_slot)) = inputs.get(index).zip(outputs.get(index)) else {
                                break;
                            };
                            let output = operation(index, input);
                            let should_stop = failed(&output);
                            *output_slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(output);
                            if should_stop {
                                admission.lock().unwrap_or_else(PoisonError::into_inner).stopped = true;
                            }
                        });
                    }
                });
                outputs
                    .into_iter()
                    .map(|output| output.into_inner().unwrap_or_else(PoisonError::into_inner))
                    .collect()
            })
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
        struct AdmissionState {
            error: Option<MemoryAdmissionError>,
            queue: MemoryAdmissionQueue,
            ready: VecDeque<usize>,
            stopped: bool,
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
                        ThreadPoolBuilder::new()
                            .num_threads(workers)
                            .thread_name(|index| format!("xylem-input-{index}"))
                            .build()
                            .map_err(MemoryDispatchError::ThreadPool)
                            .map(|pool| {
                                let admission = Mutex::new(AdmissionState {
                                    error: None,
                                    queue,
                                    ready,
                                    stopped: false,
                                });
                                let available = Condvar::new();
                                let outputs = (0..inputs.len()).map(|_| Mutex::new(None)).collect::<Vec<_>>();
                                pool.scope(|scope| {
                                    for _ in 0..workers {
                                        scope.spawn(|_| loop {
                                            let index = {
                                                let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                                                loop {
                                                    match state.ready.pop_front() {
                                                        | Some(index) => break Some(index),
                                                        | None if state.stopped || state.error.is_some() || state.queue.is_complete() => break None,
                                                        | None => {
                                                            state = available.wait(state).unwrap_or_else(PoisonError::into_inner);
                                                        }
                                                    }
                                                }
                                            };
                                            match index {
                                                | None => break,
                                                | Some(index) => {
                                                    let Some((input, output_slot)) = inputs.get(index).zip(outputs.get(index)) else {
                                                        break;
                                                    };
                                                    let output = operation(index, input);
                                                    let should_stop = failed(&output);
                                                    *output_slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(output);
                                                    let mut state = admission.lock().unwrap_or_else(PoisonError::into_inner);
                                                    let released = match should_stop || state.stopped {
                                                        | true => state.queue.release_only(index).map(|()| Vec::new()),
                                                        | false => state.queue.release(index),
                                                    };
                                                    if should_stop {
                                                        state.stopped = true;
                                                    }
                                                    match released {
                                                        | Ok(admissions) => {
                                                            state.ready.extend(admissions.into_iter().map(|admission| admission.index()))
                                                        }
                                                        | Err(error) => {
                                                            state.error = Some(error);
                                                            state.stopped = true;
                                                        }
                                                    }
                                                    available.notify_all();
                                                }
                                            }
                                        });
                                    }
                                });
                                let error = admission.into_inner().unwrap_or_else(PoisonError::into_inner).error;
                                let outputs = outputs
                                    .into_iter()
                                    .map(|output| output.into_inner().unwrap_or_else(PoisonError::into_inner))
                                    .collect::<Vec<_>>();
                                (error, outputs)
                            })
                            .and_then(|(error, outputs)| match error {
                                | Some(error) => Err(MemoryDispatchError::Admission(error)),
                                | None => Ok(outputs),
                            })
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
