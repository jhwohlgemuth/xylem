//! Execution limits and the scoped worker pool used by Xylem orchestration
use core::fmt;
use core::num::NonZeroUsize;
use core::str::FromStr;
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

/// Default maximum number of Rayon workers used by one Xylem run.
pub const DEFAULT_THREADS: usize = 10;
/// Limits applied to Xylem-owned work during a run.
///
/// This limit controls Rayon workers used by Xylem. It does not constrain
/// threads created by child commands or third-party runtimes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    threads: NonZeroUsize,
}
/// Error returned when a thread limit is zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidThreadCount;
/// A reusable, run-local execution boundary.
///
/// Keeping the pool local avoids conflicts with applications that have
/// already initialized Rayon's global pool.
pub struct ExecutionContext {
    limits: ExecutionLimits,
    pool: ThreadPool,
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
}
impl ExecutionLimits {
    /// Return the maximum number of Xylem Rayon workers.
    pub fn threads(self) -> usize {
        self.threads.get()
    }
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            threads: NonZeroUsize::new(DEFAULT_THREADS).expect("the default thread count is non-zero"),
        }
    }
}
impl TryFrom<usize> for ExecutionLimits {
    type Error = InvalidThreadCount;

    fn try_from(threads: usize) -> Result<Self, Self::Error> {
        NonZeroUsize::new(threads).map(|threads| Self { threads }).ok_or(InvalidThreadCount)
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
impl fmt::Display for ExecutionLimits {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.threads.fmt(formatter)
    }
}
impl fmt::Display for InvalidThreadCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("thread count must be greater than zero")
    }
}
impl core::error::Error for InvalidThreadCount {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use rayon::prelude::*;
    use std::sync::Barrier;
    use std::thread;

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
    fn rejects_zero_threads() {
        assert_eq!(ExecutionLimits::try_from(0), Err(InvalidThreadCount));
        assert_eq!("0".parse::<ExecutionLimits>(), Err(InvalidThreadCount.to_string()));
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
}
