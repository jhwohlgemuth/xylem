use crate::config::FailurePolicy;
use crate::execution::{
    DispatchClassification, DispatchOutcome, DispatchStopReason, ExecutionContext, ExecutionLimits, InputBatch, InvalidThreadCount,
};
use crate::run::RunCancellation;
use acorn_schema::hardware::Memory;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::time::Duration;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::sync::Barrier;
use std::thread;
use std::time::Instant;

fn measure_peak(context: &ExecutionContext, work_items: usize) -> (usize, usize) {
    let threads = context.limits().threads();
    let active = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let entered = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let gate = Barrier::new(threads.saturating_add(1));
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            context.install(|| {
                (0..work_items).into_par_iter().for_each(|_| {
                    let current = active.fetch_add(1, Ordering::SeqCst).saturating_add(1);
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
fn test_dispatch_report_cancels_active_siblings_and_preserves_failure() {
    let context = ExecutionContext::new(ExecutionLimits::try_from(4).expect("valid thread limit")).expect("execution context");
    let batch = InputBatch::try_from(2).expect("valid input batch");
    let cancellation = RunCancellation::default();
    let both_started = Barrier::new(2);
    let report = context.dispatch_inputs_report(
        &[0, 1, 2, 3],
        Some(batch),
        FailurePolicy::FailFast,
        &cancellation,
        |_, input| {
            if *input < 2 {
                both_started.wait();
            }
            match *input {
                | 0 => {
                    while !cancellation.is_requested() {
                        thread::yield_now();
                    }
                    DispatchClassification::Cancelled
                }
                | 1 => DispatchClassification::Failed,
                | _ => DispatchClassification::Succeeded,
            }
        },
        |classification| *classification,
    );
    assert_eq!(report.effective_concurrency(), 2);
    assert!(report.admission_stopped_early());
    assert_eq!(report.stop_reason(), Some(DispatchStopReason::Failure { input_index: 1 }));
    assert_eq!(
        report.outcomes(),
        [
            DispatchOutcome::Cancelled(Some(DispatchClassification::Cancelled)),
            DispatchOutcome::Failed(DispatchClassification::Failed),
            DispatchOutcome::NotStarted,
            DispatchOutcome::NotStarted,
        ]
    );
}
#[test]
fn test_dispatch_report_continue_admits_every_input() {
    let context = ExecutionContext::new(ExecutionLimits::try_from(2).expect("valid thread limit")).expect("execution context");
    let cancellation = RunCancellation::default();
    let report = context.dispatch_inputs_report(
        &[0, 1, 2],
        None,
        FailurePolicy::Continue,
        &cancellation,
        |_, input| match *input {
            | 1 => DispatchClassification::Failed,
            | _ => DispatchClassification::Succeeded,
        },
        |classification| *classification,
    );
    assert!(!report.admission_stopped_early());
    assert_eq!(report.stop_reason(), None);
    assert!(!cancellation.is_requested());
    assert_eq!(
        report.outcomes(),
        [
            DispatchOutcome::Succeeded(DispatchClassification::Succeeded),
            DispatchOutcome::Failed(DispatchClassification::Failed),
            DispatchOutcome::Succeeded(DispatchClassification::Succeeded),
        ]
    );
}
#[test]
fn test_dispatch_report_external_cancellation_marks_unstarted_inputs() {
    let context = ExecutionContext::new(ExecutionLimits::try_from(2).expect("valid thread limit")).expect("execution context");
    let cancellation = RunCancellation::default();
    cancellation.force();
    let report = context.dispatch_inputs_report(
        &[0, 1, 2],
        None,
        FailurePolicy::FailFast,
        &cancellation,
        |_, _| DispatchClassification::Succeeded,
        |classification| *classification,
    );
    assert!(report.admission_stopped_early());
    assert_eq!(report.stop_reason(), Some(DispatchStopReason::ExternalCancellation));
    assert_eq!(
        report.outcomes(),
        [
            DispatchOutcome::Cancelled(None),
            DispatchOutcome::Cancelled(None),
            DispatchOutcome::Cancelled(None)
        ]
    );
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
    let attempts = core::array::from_fn::<_, 4, _>(|_| AtomicUsize::new(0));
    let both_started = Barrier::new(2);
    let outputs = context
        .dispatch_inputs_fail_fast_memory(
            &[0, 1, 2, 3],
            Some(batch),
            Memory::kb(2.0 / 1024.0),
            (0..4).map(|_| Memory::kb(1.0 / 1024.0)).collect(),
            |_, input| {
                attempts[*input].fetch_add(1, Ordering::SeqCst);
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
    assert_eq!(attempts.map(|attempts| attempts.load(Ordering::SeqCst)), [1, 1, 0, 0]);
}
#[test]
fn test_memory_dispatch_report_cancels_active_siblings() {
    let context = ExecutionContext::new(ExecutionLimits::try_from(4).expect("valid thread limit")).expect("execution context");
    let batch = InputBatch::try_from(2).expect("valid input batch");
    let cancellation = RunCancellation::default();
    let both_started = Barrier::new(2);
    let report = context
        .dispatch_inputs_report_memory(
            &[0, 1, 2],
            Some(batch),
            Memory::kb(2.0 / 1024.0),
            (0..3).map(|_| Memory::kb(1.0 / 1024.0)).collect(),
            FailurePolicy::FailFast,
            &cancellation,
            |_, input| {
                if *input < 2 {
                    both_started.wait();
                }
                match *input {
                    | 0 => {
                        while !cancellation.is_requested() {
                            thread::yield_now();
                        }
                        DispatchClassification::Cancelled
                    }
                    | 1 => DispatchClassification::Failed,
                    | _ => DispatchClassification::Succeeded,
                }
            },
            |classification| *classification,
        )
        .expect("memory-aware input pool should run");
    assert_eq!(report.stop_reason(), Some(DispatchStopReason::Failure { input_index: 1 }));
    assert_eq!(
        report.outcomes(),
        [
            DispatchOutcome::Cancelled(Some(DispatchClassification::Cancelled)),
            DispatchOutcome::Failed(DispatchClassification::Failed),
            DispatchOutcome::NotStarted,
        ]
    );
}
#[test]
fn rejects_zero_threads() {
    assert_eq!(ExecutionLimits::try_from(0), Err(InvalidThreadCount));
    assert_eq!("0".parse::<ExecutionLimits>(), Err(InvalidThreadCount.to_string()));
}
