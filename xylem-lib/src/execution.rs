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
                                admission.next += 1;
                                index
                            };
                            let output = operation(index, &inputs[index]);
                            let should_stop = failed(&output);
                            *outputs[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(output);
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
                                                    let output = operation(index, &inputs[index]);
                                                    let should_stop = failed(&output);
                                                    *outputs[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(output);
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use core::time::Duration;
    use std::sync::Barrier;
    use std::thread;
    use std::time::Instant;

    fn measure_peak(context: &ExecutionContext, work_items: usize) -> (usize, usize) {
        let threads = context.limits().threads();
        let active = AtomicUsize::new(0);
        let completed = AtomicUsize::new(0);
        let entered = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let gate = Barrier::new(threads + 1);
        thread::scope(|scope| {
            let worker = scope.spawn(|| {
                context.install(|| {
                    (0..work_items).into_par_iter().for_each(|_| {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        let position = entered.fetch_add(1, Ordering::SeqCst);
                        if position < threads {
                            gate.wait();
                        }
                        completed.fetch_add(1, Ordering::SeqCst);
                        active.fetch_sub(1, Ordering::SeqCst);
                    });
                });
            });
            gate.wait();
            worker.join().expect("bounded work should complete");
        });
        (peak.load(Ordering::SeqCst), completed.load(Ordering::SeqCst))
    }

    #[test]
    fn fail_fast_dispatch_leaves_unadmitted_inputs_empty() {
        let context = ExecutionContext::new(ExecutionLimits::try_from(4).expect("valid thread limit")).expect("execution context");
        let batch = InputBatch::try_from(2).expect("valid input batch");
        let both_started = Barrier::new(2);
        let outputs = context
            .dispatch_inputs_fail_fast(
                &[0, 1, 2, 3, 4],
                Some(batch),
                |_, input| {
                    if *input < 2 {
                        both_started.wait();
                    }
                    if *input == 1 {
                        thread::sleep(Duration::from_millis(50));
                    }
                    (*input, *input == 0)
                },
                |(_, failed)| *failed,
            )
            .expect("input pool should build");
        assert_eq!(outputs[0], Some((0, true)));
        assert_eq!(outputs[1], Some((1, false)));
        assert!(outputs[2..].iter().all(Option::is_none));
    }
    #[test]
    fn input_dispatch_admits_work_as_soon_as_capacity_is_available() {
        let context = ExecutionContext::new(ExecutionLimits::try_from(4).expect("valid thread limit")).expect("execution context");
        let batch = InputBatch::try_from(2).expect("valid input batch");
        let third_started = AtomicBool::new(false);
        let outputs = context
            .dispatch_inputs(&[0, 1, 2], Some(batch), |input| {
                match input {
                    | 0 => {
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while !third_started.load(Ordering::SeqCst) && Instant::now() < deadline {
                            thread::sleep(Duration::from_millis(1));
                        }
                        assert!(third_started.load(Ordering::SeqCst), "third input should start before the first finishes");
                    }
                    | 2 => third_started.store(true, Ordering::SeqCst),
                    | _ => (),
                }
                *input
            })
            .expect("input pool should build");
        assert_eq!(outputs, vec![0, 1, 2]);
    }
    #[test]
    fn input_dispatch_enforces_batch_and_thread_limits_without_losing_order() {
        for (threads, batch, expected_peak) in [(10, Some(2), 2), (1, Some(2), 1), (3, None, 3)] {
            let context = ExecutionContext::new(ExecutionLimits::try_from(threads).expect("valid thread limit")).expect("execution context");
            let batch = batch.map(|value| InputBatch::try_from(value).expect("valid input batch"));
            let inputs = (0..8).collect::<Vec<_>>();
            let active = AtomicUsize::new(0);
            let entered = AtomicUsize::new(0);
            let peak = AtomicUsize::new(0);
            let gate = Barrier::new(expected_peak + 1);
            let outputs = thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    context.dispatch_inputs(&inputs, batch, |input| {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        if entered.fetch_add(1, Ordering::SeqCst) < expected_peak {
                            gate.wait();
                        }
                        active.fetch_sub(1, Ordering::SeqCst);
                        input * 2
                    })
                });
                gate.wait();
                worker.join().expect("input dispatch should complete").expect("input pool should build")
            });
            assert_eq!(outputs, inputs.iter().map(|input| input * 2).collect::<Vec<_>>());
            assert_eq!(entered.load(Ordering::SeqCst), inputs.len());
            assert_eq!(peak.load(Ordering::SeqCst), expected_peak);
        }
    }
    #[test]
    fn local_pool_enforces_limits_after_global_pool_initialization() {
        let _ = ThreadPoolBuilder::new().num_threads(2).build_global();
        for threads in [1, 3] {
            let limits = ExecutionLimits::try_from(threads).expect("positive limits should be valid");
            let context = ExecutionContext::new(limits).expect("a local pool should be constructed");
            let work_items = threads * 4;
            let (peak, completed) = measure_peak(&context, work_items);
            assert_eq!(completed, work_items);
            assert_eq!(peak, threads);
            assert!(peak <= context.limits().threads());
        }
    }
    #[test]
    fn memory_dispatch_enforces_budget_and_batch_without_losing_order() {
        let context = ExecutionContext::new(ExecutionLimits::try_from(3).expect("valid thread limit")).expect("execution context");
        let batch = InputBatch::try_from(3).expect("valid input batch");
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let started = Barrier::new(2);
        let outputs = context
            .dispatch_inputs_fail_fast_memory(
                &[0, 1, 2],
                Some(batch),
                Memory::kb(10.0 / 1024.0),
                [6_u64, 4, 6].into_iter().map(|bytes| Memory::kb(bytes as f64 / 1024.0)).collect(),
                |_, input| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    if *input < 2 {
                        started.wait();
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                    input * 2
                },
                |_| false,
            )
            .expect("memory-aware input pool should run");
        assert_eq!(outputs, [Some(0), Some(2), Some(4)]);
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn memory_dispatch_stops_new_admission_after_failure() {
        let context = ExecutionContext::new(ExecutionLimits::try_from(4).expect("valid thread limit")).expect("execution context");
        let batch = InputBatch::try_from(2).expect("valid input batch");
        let both_started = Barrier::new(2);
        let outputs = context
            .dispatch_inputs_fail_fast_memory(
                &[0, 1, 2, 3],
                Some(batch),
                Memory::kb(2.0 / 1024.0),
                (0..4).map(|_| Memory::kb(1.0 / 1024.0)).collect(),
                |_, input| {
                    if *input < 2 {
                        both_started.wait();
                    }
                    if *input == 1 {
                        thread::sleep(Duration::from_millis(50));
                    }
                    (*input, *input == 0)
                },
                |(_, failed)| *failed,
            )
            .expect("memory-aware input pool should run");
        assert_eq!(outputs[0], Some((0, true)));
        assert_eq!(outputs[1], Some((1, false)));
        assert!(outputs[2..].iter().all(Option::is_none));
    }
    #[test]
    fn rejects_zero_threads() {
        assert_eq!(ExecutionLimits::try_from(0), Err(InvalidThreadCount));
        assert_eq!("0".parse::<ExecutionLimits>(), Err(InvalidThreadCount.to_string()));
    }
}
