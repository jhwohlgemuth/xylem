//! Pure memory estimation and deterministic admission primitives.
extern crate alloc;

use crate::execution::InputBatch;
use acorn_schema::hardware::Memory;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::fmt;
use serde::Serialize;
use std::fs::{read_dir, symlink_metadata};
use std::path::{Path, PathBuf};

const FIXED_POINT_SCALE: u64 = 1_000_000;

/// Failure to construct or update a memory admission queue.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryAdmissionError {
    /// The configured run budget is invalid or zero.
    InvalidBudget {
        /// Configured run budget.
        budget: Memory,
    },
    /// One input reservation is invalid or zero.
    InvalidReservation {
        /// Stable input position.
        index: usize,
        /// Required reservation.
        reservation: Memory,
    },
    /// The input is not currently active and cannot release a reservation.
    NotActive {
        /// Stable input position.
        index: usize,
    },
    /// One input reservation exceeds the entire run budget.
    Oversized {
        /// Configured run budget.
        budget: Memory,
        /// Stable input position.
        index: usize,
        /// Required reservation.
        reservation: Memory,
    },
}
/// Failure to calculate a memory reservation safely.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryCalculationError {
    /// The aggregate input size is invalid or zero.
    InvalidInput {
        /// Aggregate input size supplied to the estimator.
        input: Memory,
    },
    /// Checked fixed-point arithmetic exceeded the supported byte range.
    Overflow {
        /// Estimator identity used for the calculation.
        estimator: String,
        /// Aggregate input size supplied to the estimator.
        input: Memory,
    },
}
/// A supported reservation or a typed reason to retain fixed batching.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryEstimate {
    /// The estimator cannot safely cover this request.
    Fallback(MemoryFallback),
    /// Positive bytes that must be reserved before admitting the input.
    Reservation(Memory),
}
/// Failure to construct a safe fixed-point estimator.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryEstimatorError {
    /// The estimator identity is empty.
    EmptyIdentity,
    /// The measured input range has its bounds reversed.
    InvalidInputRange {
        /// Largest configured input size.
        maximum: Memory,
        /// Smallest configured input size.
        minimum: Memory,
    },
    /// A configured memory quantity is invalid.
    InvalidMemory {
        /// Estimator attribute containing the invalid quantity.
        attribute: &'static str,
        /// Invalid memory quantity.
        memory: Memory,
    },
    /// The safety multiplier is below one.
    UnsafeMultiplier {
        /// Fixed-point multiplier scaled by one million.
        safety_millionths: u64,
    },
    /// Both the intercept and slope are zero.
    ZeroPrediction,
}
/// Reason that memory-aware admission cannot be used for an input.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryFallback {
    /// The input size is outside the estimator's measured range.
    InputOutOfRange {
        /// Estimator identity selected by the caller.
        estimator: String,
        /// Aggregate input size supplied to the estimator.
        input: Memory,
        /// Largest measured aggregate input size.
        maximum: Memory,
        /// Smallest measured aggregate input size.
        minimum: Memory,
    },
}
/// Per-input reason that a run must retain fixed batching.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannedMemoryFallback {
    /// The input size is outside the estimator's calibrated range.
    InputOutOfRange {
        /// Largest calibrated aggregate input size.
        maximum_input_bytes: u64,
        /// Smallest calibrated aggregate input size.
        minimum_input_bytes: u64,
    },
    /// The input could not be measured safely.
    InputUnavailable {
        /// Stable diagnostic explaining why sizing was unavailable.
        detail: String,
    },
    /// The workflow has no concrete input to size.
    NoInput,
}
/// One newly admitted input and its reservation.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryAdmission {
    index: usize,
    reservation: Memory,
}
/// Deterministic FIFO memory admission state.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryAdmissionQueue {
    active: BTreeMap<usize, Memory>,
    batch: InputBatch,
    budget: Memory,
    next: usize,
    reservations: Vec<Memory>,
    reserved: u64,
}
/// Module-agnostic fixed-point memory estimator derived from measured data.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEstimator {
    identity: String,
    intercept: Memory,
    maximum_input: Memory,
    minimum_input: Memory,
    safety_millionths: u64,
    slope_millionths: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MemoryReservationBreakdown {
    pub(crate) estimated: Memory,
    pub(crate) reservation: Memory,
    pub(crate) safety_reserve: Memory,
}
/// Read-only sizing and reservation result for one resolved input.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlannedMemoryInput {
    /// Reason memory-aware admission cannot cover this input.
    pub fallback: Option<PlannedMemoryFallback>,
    /// Aggregate logical bytes across regular files.
    pub input_bytes: Option<u64>,
    /// Stable position in resolved input order.
    pub input_index: usize,
    /// Estimated peak memory including the configured safety multiplier.
    pub reservation: Option<Memory>,
}

impl MemoryAdmission {
    /// Return the stable input position.
    pub fn index(&self) -> usize {
        self.index
    }
    /// Return the admitted reservation.
    pub fn reservation(&self) -> &Memory {
        &self.reservation
    }
}
impl core::error::Error for MemoryAdmissionError {}
impl fmt::Display for MemoryAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::InvalidBudget { .. } => formatter.write_str("memory budget must be finite and greater than zero"),
            | Self::InvalidReservation { index, .. } => write!(formatter, "input {index} memory reservation must be finite and greater than zero"),
            | Self::NotActive { index } => write!(formatter, "input {index} has no active memory reservation"),
            | Self::Oversized { budget, index, reservation } => write!(
                formatter,
                "input {index} requires {} memory bytes, exceeding the {}-byte budget",
                reservation.checked_bytes().unwrap_or_default(),
                budget.checked_bytes().unwrap_or_default()
            ),
        }
    }
}
impl MemoryAdmissionQueue {
    /// Build a queue after rejecting reservations larger than the full budget.
    pub fn new(budget: Memory, batch: InputBatch, reservations: Vec<Memory>) -> Result<Self, MemoryAdmissionError> {
        match budget.checked_bytes().filter(|value| *value > 0) {
            | None => Err(MemoryAdmissionError::InvalidBudget { budget }),
            | Some(capacity) => {
                let values = reservations
                    .iter()
                    .position(|reservation| reservation.checked_bytes().filter(|value| *value > 0).is_none());
                match values {
                    | Some(index) => Err(MemoryAdmissionError::InvalidReservation {
                        index,
                        reservation: reservations[index].clone(),
                    }),
                    | None => match reservations
                        .iter()
                        .position(|reservation| reservation.checked_bytes().is_some_and(|required| required > capacity))
                    {
                        | Some(index) => Err(MemoryAdmissionError::Oversized {
                            budget,
                            index,
                            reservation: reservations[index].clone(),
                        }),
                        | None => Ok(Self {
                            active: BTreeMap::new(),
                            batch,
                            budget,
                            next: 0,
                            reservations,
                            reserved: 0,
                        }),
                    },
                }
            }
        }
    }
    /// Admit the longest FIFO prefix that fits both budget and batch ceilings.
    pub fn admit(&mut self) -> Vec<MemoryAdmission> {
        let available = self.budget.checked_bytes().unwrap_or_default().saturating_sub(self.reserved);
        let available_slots = self.batch.get().saturating_sub(self.active.len());
        let admissions = self
            .reservations
            .iter()
            .cloned()
            .enumerate()
            .skip(self.next)
            .take(available_slots)
            .scan(available, |remaining, (index, reservation)| {
                let required = reservation.checked_bytes().unwrap_or_default();
                match required <= *remaining {
                    | true => {
                        *remaining = remaining.saturating_sub(required);
                        Some(MemoryAdmission { index, reservation })
                    }
                    | false => None,
                }
            })
            .collect::<Vec<_>>();
        let admitted = admissions
            .iter()
            .filter_map(|admission| admission.reservation.checked_bytes())
            .fold(0, u64::saturating_add);
        admissions.iter().for_each(|admission| {
            self.active.insert(admission.index, admission.reservation.clone());
        });
        self.next = self.next.saturating_add(admissions.len());
        self.reserved = self.reserved.saturating_add(admitted);
        admissions
    }
    /// Return the number of active reservations.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
    /// Return whether no pending or active inputs remain.
    pub fn is_complete(&self) -> bool {
        self.next == self.reservations.len() && self.active.is_empty()
    }
    /// Release one active reservation and immediately admit the next FIFO prefix.
    pub fn release(&mut self, index: usize) -> Result<Vec<MemoryAdmission>, MemoryAdmissionError> {
        self.release_only(index).map(|()| self.admit())
    }
    pub(crate) fn release_only(&mut self, index: usize) -> Result<(), MemoryAdmissionError> {
        self.active.remove(&index).map_or_else(
            || Err(MemoryAdmissionError::NotActive { index }),
            |reservation| {
                self.reserved = self.reserved.saturating_sub(reservation.checked_bytes().unwrap_or_default());
                Ok(())
            },
        )
    }
    /// Return the active reservation total.
    pub fn reserved(&self) -> Memory {
        Memory::kb(self.reserved as f64 / 1024.0)
    }
}
impl core::error::Error for MemoryCalculationError {}
impl fmt::Display for MemoryCalculationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::InvalidInput { .. } => formatter.write_str("memory estimator input must be finite and greater than zero"),
            | Self::Overflow { estimator, input } => write!(
                formatter,
                "memory estimator {estimator} overflowed for {} input bytes",
                input.checked_bytes().unwrap_or_default()
            ),
        }
    }
}
impl MemoryEstimator {
    /// Build a module-agnostic fixed-point estimator from validated measurements.
    pub fn new(
        identity: impl Into<String>,
        intercept: Memory,
        minimum_input: Memory,
        maximum_input: Memory,
        slope_millionths: u64,
        safety_millionths: u64,
    ) -> Result<Self, MemoryEstimatorError> {
        let identity = identity.into();
        let intercept_value = intercept.checked_bytes();
        let minimum_value = minimum_input.checked_bytes().filter(|value| *value > 0);
        let maximum_value = maximum_input.checked_bytes().filter(|value| *value > 0);
        match (intercept_value, minimum_value, maximum_value) {
            | (None, _, _) => Err(MemoryEstimatorError::InvalidMemory {
                attribute: "intercept",
                memory: intercept,
            }),
            | (_, None, _) => Err(MemoryEstimatorError::InvalidMemory {
                attribute: "minimum_input",
                memory: minimum_input,
            }),
            | (_, _, None) => Err(MemoryEstimatorError::InvalidMemory {
                attribute: "maximum_input",
                memory: maximum_input,
            }),
            | (Some(intercept_value), Some(minimum_value), Some(maximum_value)) => match (
                identity.trim().is_empty(),
                minimum_value > maximum_value,
                intercept_value == 0 && slope_millionths == 0,
                safety_millionths < FIXED_POINT_SCALE,
            ) {
                | (true, _, _, _) => Err(MemoryEstimatorError::EmptyIdentity),
                | (_, true, _, _) => Err(MemoryEstimatorError::InvalidInputRange {
                    maximum: maximum_input,
                    minimum: minimum_input,
                }),
                | (_, _, true, _) => Err(MemoryEstimatorError::ZeroPrediction),
                | (_, _, _, true) => Err(MemoryEstimatorError::UnsafeMultiplier { safety_millionths }),
                | (false, false, false, false) => Ok(Self {
                    identity,
                    intercept,
                    maximum_input,
                    minimum_input,
                    safety_millionths,
                    slope_millionths,
                }),
            },
        }
    }
    fn calculate(&self, input: Memory, input_value: u64) -> Result<MemoryReservationBreakdown, MemoryCalculationError> {
        self.intercept
            .checked_bytes()
            .map(u128::from)
            .and_then(|intercept| intercept.checked_mul(u128::from(FIXED_POINT_SCALE)))
            .and_then(|intercept| {
                u128::from(self.slope_millionths)
                    .checked_mul(u128::from(input_value))
                    .and_then(|variable| intercept.checked_add(variable))
            })
            .and_then(|scaled| {
                scaled
                    .checked_add(u128::from(FIXED_POINT_SCALE) - 1)
                    .map(|adjusted| adjusted / u128::from(FIXED_POINT_SCALE))
            })
            .and_then(|estimated| {
                estimated
                    .checked_mul(u128::from(self.safety_millionths))
                    .and_then(|safe| {
                        safe.checked_add(u128::from(FIXED_POINT_SCALE) - 1)
                            .map(|adjusted| adjusted / u128::from(FIXED_POINT_SCALE))
                    })
                    .map(|reservation| (estimated, reservation))
            })
            .and_then(|(estimated, reservation)| u64::try_from(estimated).ok().zip(u64::try_from(reservation).ok()))
            .filter(|(estimated, reservation)| *estimated > 0 && *reservation >= *estimated)
            .and_then(|(estimated_bytes, reservation_bytes)| {
                let estimated = Memory::kb(estimated_bytes as f64 / 1024.0);
                let reservation = Memory::kb(reservation_bytes as f64 / 1024.0);
                reservation
                    .checked_bytes()
                    .filter(|value| *value > 0 && *value >= estimated_bytes)
                    .map(|actual_reservation_bytes| MemoryReservationBreakdown {
                        estimated,
                        reservation,
                        safety_reserve: Memory::kb(actual_reservation_bytes.saturating_sub(estimated_bytes) as f64 / 1024.0),
                    })
            })
            .ok_or(MemoryCalculationError::Overflow {
                estimator: self.identity.clone(),
                input,
            })
    }
    /// Estimate a reservation or return an out-of-range fallback.
    pub fn estimate(&self, input: Memory) -> Result<MemoryEstimate, MemoryCalculationError> {
        match input.checked_bytes().filter(|value| *value > 0) {
            | None => Err(MemoryCalculationError::InvalidInput { input }),
            | Some(input_value) => match input_value < self.minimum_input.checked_bytes().unwrap_or_default()
                || input_value > self.maximum_input.checked_bytes().unwrap_or_default()
            {
                | true => Ok(MemoryEstimate::Fallback(MemoryFallback::InputOutOfRange {
                    estimator: self.identity.clone(),
                    input,
                    maximum: self.maximum_input.clone(),
                    minimum: self.minimum_input.clone(),
                })),
                | false => self
                    .calculate(input, input_value)
                    .map(|breakdown| MemoryEstimate::Reservation(breakdown.reservation)),
            },
        }
    }
    pub(crate) fn reservation_breakdown_bytes(&self, input_bytes: u64) -> Result<MemoryReservationBreakdown, MemoryCalculationError> {
        let input = Memory::kb(input_bytes as f64 / 1024.0);
        match input_bytes {
            | 0 => Err(MemoryCalculationError::InvalidInput { input }),
            | input_value => self.calculate(input, input_value),
        }
    }
    /// Inspect resolved inputs and calculate deterministic memory reservations.
    pub fn plan(&self, inputs: &[Option<String>]) -> Result<Vec<PlannedMemoryInput>, MemoryCalculationError> {
        inputs
            .iter()
            .enumerate()
            .map(|(input_index, input)| match input {
                | None => Ok(PlannedMemoryInput {
                    fallback: Some(PlannedMemoryFallback::NoInput),
                    input_bytes: None,
                    input_index,
                    reservation: None,
                }),
                | Some(input) => match aggregate_input_bytes(Path::new(input)) {
                    | Err(detail) => Ok(PlannedMemoryInput {
                        fallback: Some(PlannedMemoryFallback::InputUnavailable { detail }),
                        input_bytes: None,
                        input_index,
                        reservation: None,
                    }),
                    | Ok(input_bytes) if input_bytes == 0 => Ok(PlannedMemoryInput {
                        fallback: Some(PlannedMemoryFallback::InputOutOfRange {
                            maximum_input_bytes: self.maximum_input_bytes(),
                            minimum_input_bytes: self.minimum_input_bytes(),
                        }),
                        input_bytes: Some(input_bytes),
                        input_index,
                        reservation: None,
                    }),
                    | Ok(input_bytes) => {
                        let input_memory = Memory::kb(input_bytes as f64 / 1024.0);
                        self.estimate(input_memory).map(|estimate| match estimate {
                            | MemoryEstimate::Fallback(MemoryFallback::InputOutOfRange { maximum, minimum, .. }) => PlannedMemoryInput {
                                fallback: Some(PlannedMemoryFallback::InputOutOfRange {
                                    maximum_input_bytes: maximum.checked_bytes().unwrap_or_default(),
                                    minimum_input_bytes: minimum.checked_bytes().unwrap_or_default(),
                                }),
                                input_bytes: Some(input_bytes),
                                input_index,
                                reservation: None,
                            },
                            | MemoryEstimate::Reservation(reservation) => PlannedMemoryInput {
                                fallback: None,
                                input_bytes: Some(input_bytes),
                                input_index,
                                reservation: Some(reservation),
                            },
                        })
                    }
                },
            })
            .collect()
    }
    /// Return the estimator identity used for plans and telemetry.
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Return the largest calibrated aggregate input size in bytes.
    pub fn maximum_input_bytes(&self) -> u64 {
        self.maximum_input.checked_bytes().unwrap_or_default()
    }
    /// Return the smallest calibrated aggregate input size in bytes.
    pub fn minimum_input_bytes(&self) -> u64 {
        self.minimum_input.checked_bytes().unwrap_or_default()
    }
}
impl core::error::Error for MemoryEstimatorError {}
impl fmt::Display for MemoryEstimatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::EmptyIdentity => formatter.write_str("memory estimator identity must not be empty"),
            | Self::InvalidInputRange { maximum, minimum } => write!(
                formatter,
                "memory estimator input range is invalid: {}..={}",
                minimum.checked_bytes().unwrap_or_default(),
                maximum.checked_bytes().unwrap_or_default()
            ),
            | Self::InvalidMemory { attribute, .. } => write!(formatter, "memory estimator {attribute} must be finite and non-negative"),
            | Self::UnsafeMultiplier { safety_millionths } => write!(
                formatter,
                "memory estimator safety multiplier must be at least 1000000 millionths, found {safety_millionths}"
            ),
            | Self::ZeroPrediction => formatter.write_str("memory estimator intercept and slope cannot both be zero"),
        }
    }
}
impl fmt::Display for PlannedMemoryFallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::InputOutOfRange {
                maximum_input_bytes,
                minimum_input_bytes,
            } => write!(
                formatter,
                "input size is outside the calibrated range {minimum_input_bytes}..={maximum_input_bytes} bytes"
            ),
            | Self::InputUnavailable { detail } => formatter.write_str(detail),
            | Self::NoInput => formatter.write_str("the workflow has no concrete input to size"),
        }
    }
}
fn aggregate_input_bytes(root: &Path) -> Result<u64, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    while let Some(path) = pending.pop() {
        let metadata = match symlink_metadata(&path) {
            | Ok(metadata) => metadata,
            | Err(error) => return Err(format!("inspect {}: {error}", path.display())),
        };
        let file_type = metadata.file_type();
        match (file_type.is_symlink(), file_type.is_file(), file_type.is_dir()) {
            | (true, _, _) => return Err(format!("symbolic links are not supported at {}", path.display())),
            | (false, true, _) => {
                total = match total.checked_add(metadata.len()) {
                    | Some(total) => total,
                    | None => return Err(format!("aggregate input size overflow at {}", path.display())),
                };
            }
            | (false, false, true) => {
                let mut children = match read_dir(&path) {
                    | Ok(entries) => {
                        let paths = entries
                            .map(|entry| {
                                entry
                                    .map(|entry| entry.path())
                                    .map_err(|error| format!("inspect {}: {error}", path.display()))
                            })
                            .collect::<Result<Vec<PathBuf>, _>>();
                        match paths {
                            | Ok(children) => children,
                            | Err(error) => return Err(error),
                        }
                    }
                    | Err(error) => return Err(format!("inspect {}: {error}", path.display())),
                };
                children.sort();
                pending.extend(children.into_iter().rev());
            }
            | (false, false, false) => return Err(format!("unsupported filesystem entry at {}", path.display())),
        }
    }
    Ok(total)
}
#[cfg(test)]
mod tests;
