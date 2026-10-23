//! Deterministic duration history and fixed-concurrency completion projections
extern crate alloc;

use crate::journal::{ExecutionState, RunStatus, RunSummary};
use acorn_core::time::{Milliseconds, UnitOfTime};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// Confidence source attached to a completion projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionConfidence {
    /// Current-run throughput was used because history was incomplete.
    Low,
    /// Successful matching module histories covered all remaining work.
    Historical,
}
/// Successful per-module duration observations, capped newest first.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DurationHistory {
    samples: BTreeMap<Vec<String>, Vec<u64>>,
}
/// Fixed-concurrency estimate for unfinished module-input work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProgressProjection {
    /// Number of terminal module-input steps.
    pub completed_steps: usize,
    /// Source quality for the projection.
    pub confidence: ProjectionConfidence,
    /// Median remaining duration.
    pub eta: Milliseconds,
    /// Optimistic remaining duration.
    pub p10: Milliseconds,
    /// Conservative remaining duration.
    pub p90: Milliseconds,
    /// Total number of module-input steps.
    pub total_steps: usize,
}
impl DurationHistory {
    /// Build history from up to fifty newest successful matching summaries.
    pub fn from_summaries<'a>(summaries: impl IntoIterator<Item = &'a RunSummary>) -> Self {
        let samples = summaries
            .into_iter()
            .filter(|summary| summary.status == RunStatus::Succeeded)
            .take(50)
            .flat_map(|summary| summary.inputs.iter())
            .flat_map(|input| input.modules.iter())
            .filter(|module| module.state == ExecutionState::Succeeded)
            .filter_map(|module| {
                module
                    .started_at
                    .map(|started| (module.module_path.clone(), module.updated_at.get().saturating_sub(started.get())))
            })
            .fold(BTreeMap::<Vec<String>, Vec<u64>>::new(), |mut samples, (path, duration)| {
                let values = samples.entry(path).or_default();
                if values.len() < 20 {
                    values.push(duration);
                }
                samples
            });
        Self { samples }
    }
    /// Project remaining work using historical module durations or current throughput.
    pub fn project(&self, summary: &RunSummary, now_ms: u64, concurrency: usize) -> Option<ProgressProjection> {
        let modules = summary.inputs.iter().flat_map(|input| input.modules.iter()).collect::<Vec<_>>();
        let completed_steps = modules.iter().filter(|module| module.state.is_complete()).count();
        let total_steps = modules.len();
        let remaining = modules.iter().filter(|module| !module.state.is_complete()).collect::<Vec<_>>();
        match (total_steps, remaining.is_empty()) {
            | (0, _) | (_, true) => None,
            | _ => {
                let historical = remaining
                    .iter()
                    .map(|module| self.samples.get(&module.module_path))
                    .collect::<Option<Vec<_>>>();
                historical.map_or_else(
                    || {
                        let elapsed = now_ms.saturating_sub(summary.started_at.get());
                        (completed_steps > 0).then(|| {
                            let per_step = elapsed.checked_div(completed_steps as u64).unwrap_or_default();
                            let total = per_step.saturating_mul(remaining.len() as u64);
                            let divisor = concurrency.max(1) as u64;
                            let eta = total.div_ceil(divisor);
                            ProgressProjection {
                                completed_steps,
                                confidence: ProjectionConfidence::Low,
                                eta: Milliseconds::new(eta),
                                p10: Milliseconds::new(eta),
                                p90: Milliseconds::new(eta),
                                total_steps,
                            }
                        })
                    },
                    |samples| {
                        let estimate = |percentile| {
                            let total = samples
                                .iter()
                                .map(|sample| {
                                    let mut values = sample.to_vec();
                                    values.sort_unstable();
                                    let index = values.len().saturating_sub(1).saturating_mul(percentile) / 100;
                                    values.get(index).copied().unwrap_or_default()
                                })
                                .fold(0_u64, u64::saturating_add);
                            let divisor = concurrency.max(1) as u64;
                            total.div_ceil(divisor)
                        };
                        Some(ProgressProjection {
                            completed_steps,
                            confidence: ProjectionConfidence::Historical,
                            eta: Milliseconds::new(estimate(50)),
                            p10: Milliseconds::new(estimate(10)),
                            p90: Milliseconds::new(estimate(90)),
                            total_steps,
                        })
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests;
