use super::*;

const MODULE_A_P_ESTIMATOR: &str = "module-a-p-v1";
const MODULE_A_P_O_ESTIMATOR: &str = "module-a-p-o-v1";

fn accepted_estimators() -> [MemoryEstimator; 2] {
    [
        MemoryEstimator::new(
            MODULE_A_P_ESTIMATOR,
            memory(3_754_352_538),
            memory(529_854_516),
            memory(1_143_813_163),
            29_679_659,
            1_100_000,
        )
        .expect("valid Module A/P estimator"),
        MemoryEstimator::new(
            MODULE_A_P_O_ESTIMATOR,
            memory(3_696_455_731),
            memory(529_854_516),
            memory(1_143_813_163),
            29_776_231,
            1_100_000,
        )
        .expect("valid Module A/P/O estimator"),
    ]
}
fn batch(value: usize) -> InputBatch {
    InputBatch::try_from(value).expect("positive batch")
}
fn memory(value: u64) -> Memory {
    Memory::kb(value as f64 / 1024.0)
}
fn reservation(estimate: MemoryEstimate) -> u64 {
    match estimate {
        | MemoryEstimate::Reservation(memory) => memory.checked_bytes().expect("valid reservation"),
        | MemoryEstimate::Fallback(reason) => panic!("unexpected fallback: {reason:?}"),
    }
}

#[test]
fn admission_is_fifo_and_does_not_skip_a_waiting_input() {
    let mut queue = MemoryAdmissionQueue::new(memory(10), batch(3), vec![memory(6), memory(5), memory(4)]).expect("valid queue");
    assert_eq!(queue.admit().iter().map(|admission| admission.index()).collect::<Vec<_>>(), vec![0]);
    assert_eq!(
        queue
            .release(0)
            .expect("release")
            .iter()
            .map(|admission| admission.index())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}
#[test]
fn admission_rejects_an_input_one_byte_over_budget() {
    let result = MemoryAdmissionQueue::new(memory(10), batch(1), vec![memory(11)]);
    assert_eq!(
        result,
        Err(MemoryAdmissionError::Oversized {
            budget: memory(10),
            index: 0,
            reservation: memory(11),
        })
    );
}
#[test]
fn admission_rejects_zero_budget() {
    assert_eq!(
        MemoryAdmissionQueue::new(memory(0), batch(1), vec![memory(1)]),
        Err(MemoryAdmissionError::InvalidBudget { budget: memory(0) })
    );
}
#[test]
fn admission_respects_batch_when_memory_has_capacity() {
    let mut queue = MemoryAdmissionQueue::new(memory(100), batch(2), vec![memory(10), memory(10), memory(10)]).expect("valid queue");
    assert_eq!(queue.admit().len(), 2);
    assert_eq!(queue.active_count(), 2);
    assert_eq!(queue.reserved().checked_bytes(), Some(20));
}
#[test]
fn admission_respects_memory_when_batch_has_capacity() {
    let mut queue = MemoryAdmissionQueue::new(memory(10), batch(3), vec![memory(6), memory(6)]).expect("valid queue");
    assert_eq!(queue.admit().len(), 1);
    assert_eq!(queue.active_count(), 1);
    assert_eq!(queue.reserved().checked_bytes(), Some(6));
}
#[test]
fn estimator_arithmetic_rejects_overflow() {
    let input = Memory::tb(8_000_000.0);
    let estimator = MemoryEstimator::new("overflow", memory(1), memory(1), input.clone(), u64::MAX, u64::MAX).expect("structurally valid estimator");
    assert_eq!(
        estimator.estimate(input.clone()),
        Err(MemoryCalculationError::Overflow {
            estimator: "overflow".to_string(),
            input,
        })
    );
}
#[test]
fn estimator_constructor_rejects_invalid_models() {
    assert_eq!(
        MemoryEstimator::new("", memory(1), memory(1), memory(2), 1, 1_000_000),
        Err(MemoryEstimatorError::EmptyIdentity)
    );
    assert_eq!(
        MemoryEstimator::new("range", memory(1), memory(2), memory(1), 1, 1_000_000),
        Err(MemoryEstimatorError::InvalidInputRange {
            maximum: memory(1),
            minimum: memory(2),
        })
    );
    assert_eq!(
        MemoryEstimator::new("zero", memory(0), memory(1), memory(2), 0, 1_000_000),
        Err(MemoryEstimatorError::ZeroPrediction)
    );
    assert_eq!(
        MemoryEstimator::new("unsafe", memory(1), memory(1), memory(2), 1, 999_999),
        Err(MemoryEstimatorError::UnsafeMultiplier { safety_millionths: 999_999 })
    );
}
#[test]
fn estimator_includes_measured_range_boundaries() {
    accepted_estimators().iter().for_each(|estimator| {
        assert!(matches!(estimator.estimate(memory(529_854_516)), Ok(MemoryEstimate::Reservation(_))));
        assert!(matches!(estimator.estimate(memory(1_143_813_163)), Ok(MemoryEstimate::Reservation(_))));
    });
}
#[test]
fn estimator_returns_typed_out_of_range_fallbacks() {
    let estimators = accepted_estimators();
    assert_eq!(
        estimators[0].estimate(memory(529_854_515)),
        Ok(MemoryEstimate::Fallback(MemoryFallback::InputOutOfRange {
            estimator: MODULE_A_P_ESTIMATOR.to_string(),
            input: memory(529_854_515),
            maximum: memory(1_143_813_163),
            minimum: memory(529_854_516),
        }))
    );
    assert_eq!(
        estimators[1].estimate(memory(1_143_813_164)),
        Ok(MemoryEstimate::Fallback(MemoryFallback::InputOutOfRange {
            estimator: MODULE_A_P_O_ESTIMATOR.to_string(),
            input: memory(1_143_813_164),
            maximum: memory(1_143_813_163),
            minimum: memory(529_854_516),
        }))
    );
}
#[test]
fn estimator_supports_arbitrary_module_sets() {
    let estimator =
        MemoryEstimator::new("custom-modules-x-y-z-v1", memory(100), memory(1), memory(100), 2_000_000, 1_250_000).expect("valid custom estimator");
    assert_eq!(estimator.identity(), "custom-modules-x-y-z-v1");
    assert_eq!(reservation(estimator.estimate(memory(10)).expect("estimate")), 150);
}
#[test]
fn estimators_use_conservative_fixed_point_coefficients() {
    let estimators = accepted_estimators();
    assert_eq!(
        reservation(estimators[0].estimate(memory(678_219_617)).expect("estimate")),
        26_272_047_448
    );
    assert_eq!(
        reservation(estimators[1].estimate(memory(678_219_617)).expect("estimate")),
        26_280_407_688
    );
}
#[test]
fn exact_budget_fit_is_admitted() {
    let mut queue = MemoryAdmissionQueue::new(memory(10), batch(1), vec![memory(10)]).expect("valid queue");
    assert_eq!(
        queue.admit(),
        vec![MemoryAdmission {
            index: 0,
            reservation: memory(10)
        }]
    );
}
#[test]
fn invariant_sequences_never_exceed_budget_or_batch() {
    (1..=4).for_each(|batch_limit| {
        (4..=12).for_each(|budget| {
            let reservations = [1, 2, 3, 4, 5].into_iter().map(|value| memory(value.min(budget))).collect::<Vec<_>>();
            let mut queue = MemoryAdmissionQueue::new(memory(budget), batch(batch_limit), reservations).expect("valid queue");
            let mut admitted = queue.admit().into_iter().map(|admission| admission.index()).collect::<Vec<_>>();
            while !queue.is_complete() {
                assert!(queue.reserved().checked_bytes().is_some_and(|reserved| reserved <= budget));
                assert!(queue.active_count() <= batch_limit);
                let index = *queue.active.keys().next().expect("active input while incomplete");
                admitted.extend(queue.release(index).expect("release").into_iter().map(|admission| admission.index()));
            }
            assert_eq!(admitted, (0..5).collect::<Vec<_>>());
            assert_eq!(queue.reserved().checked_bytes(), Some(0));
        });
    });
}
#[test]
fn release_admits_waiting_work_immediately_and_only_once() {
    let mut queue = MemoryAdmissionQueue::new(memory(10), batch(2), vec![memory(6), memory(4), memory(6)]).expect("valid queue");
    assert_eq!(queue.admit().iter().map(|admission| admission.index()).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(
        queue
            .release(0)
            .expect("release")
            .iter()
            .map(|admission| admission.index())
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(queue.release(0), Err(MemoryAdmissionError::NotActive { index: 0 }));
}
