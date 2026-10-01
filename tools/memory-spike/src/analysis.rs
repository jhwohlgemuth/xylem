use crate::model::{
    Estimator, HostIdentity, InputFeatures, Manifest, ModuleIdentity, Observation, Partition, SubjectType, CASE_REPETITIONS, MIX_REPETITIONS,
    OBSERVATION_SCHEMA_VERSION,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::cmp::Ordering;
use serde::Serialize;

const MIB: u64 = 1024 * 1024;
const REQUIRED_MEDIAN_REDUCTION: f64 = 0.20;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Analysis {
    pub(crate) decision: Decision,
    pub(crate) estimators: Vec<EstimatorAnalysis>,
    pub(crate) host: HostIdentity,
    pub(crate) observation_schema_version: u32,
    pub(crate) schema_version: u32,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CandidateAnalysis {
    pub(crate) accepted: bool,
    pub(crate) bucket_training_peaks: BTreeMap<u64, u64>,
    pub(crate) extrapolations: usize,
    pub(crate) feature_maximum: u64,
    pub(crate) feature_minimum: u64,
    pub(crate) intercept: f64,
    pub(crate) maximum_underprediction_bytes: u64,
    pub(crate) mean_absolute_error_bytes: f64,
    pub(crate) median_reservation_reduction: f64,
    pub(crate) mix_underpredictions: usize,
    pub(crate) model: ModelKind,
    pub(crate) p95_overreservation_bytes: u64,
    pub(crate) safety_multiplier: f64,
    pub(crate) slope: f64,
    pub(crate) underpredictions: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Decision {
    Go,
    NoGo,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EstimatorAnalysis {
    pub(crate) baseline_bytes: u64,
    pub(crate) candidates: Vec<CandidateAnalysis>,
    pub(crate) decision: Decision,
    pub(crate) fixed_worst_case_bytes: u64,
    pub(crate) key: String,
    pub(crate) modules: Vec<ModuleIdentity>,
    pub(crate) required_for_go: bool,
    pub(crate) selected_model: Option<ModelKind>,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelKind {
    DecodedBytes,
    InputBytes,
    SizeBucket,
}
#[derive(Clone, Copy)]
struct CasePeak<'a> {
    case_id: &'a str,
    features: &'a InputFeatures,
    incremental_bytes: u64,
    partition: Partition,
}
struct EvaluationContext<'a> {
    baseline_bytes: u64,
    estimator: &'a Estimator,
    fixed_worst_case_bytes: u64,
    manifest: &'a Manifest,
    observations: &'a [Observation],
    peaks: &'a [CasePeak<'a>],
}
enum CandidateModel {
    Bucket {
        buckets: BTreeMap<u64, u64>,
        maximum_feature: u64,
        minimum_feature: u64,
        safety_multiplier: f64,
    },
    Linear {
        intercept: f64,
        maximum_feature: u64,
        minimum_feature: u64,
        safety_multiplier: f64,
        slope: f64,
    },
}

impl CandidateModel {
    fn bucket_training_peaks(&self) -> BTreeMap<u64, u64> {
        match self {
            | Self::Bucket { buckets, .. } => buckets.clone(),
            | Self::Linear { .. } => BTreeMap::new(),
        }
    }
    fn coefficients(&self) -> (f64, f64, f64) {
        match self {
            | Self::Bucket { safety_multiplier, .. } => (0.0, 0.0, *safety_multiplier),
            | Self::Linear {
                intercept,
                safety_multiplier,
                slope,
                ..
            } => (*intercept, *slope, *safety_multiplier),
        }
    }
    fn reserve(&self, feature: u64) -> Option<u64> {
        match self {
            | Self::Bucket {
                buckets,
                maximum_feature,
                minimum_feature,
                safety_multiplier,
            } if feature >= *minimum_feature && feature <= *maximum_feature => buckets
                .get(&feature_bucket(feature))
                .map(|value| round_up_mib((*value as f64 * safety_multiplier).ceil() as u64)),
            | Self::Linear {
                intercept,
                maximum_feature,
                minimum_feature,
                safety_multiplier,
                slope,
            } if feature >= *minimum_feature && feature <= *maximum_feature => {
                let predicted = (intercept + slope * feature as f64).max(1.0);
                Some(round_up_mib((predicted * safety_multiplier).ceil() as u64))
            }
            | _ => None,
        }
    }
    fn training_range(&self) -> (u64, u64) {
        match self {
            | Self::Bucket {
                maximum_feature,
                minimum_feature,
                ..
            }
            | Self::Linear {
                maximum_feature,
                minimum_feature,
                ..
            } => (*minimum_feature, *maximum_feature),
        }
    }
}

pub(crate) fn analyze(manifest: &Manifest, observations: &[Observation]) -> Result<Analysis, String> {
    validate_observations(manifest, observations).and_then(|()| {
        observations.first().map_or_else(
            || Err("analysis requires observations".to_string()),
            |first| {
                manifest
                    .estimators
                    .iter()
                    .map(|estimator| analyze_estimator(manifest, observations, estimator))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|estimators| {
                        let decision = match estimators
                            .iter()
                            .filter(|estimator| estimator.required_for_go)
                            .all(|estimator| estimator.decision == Decision::Go)
                        {
                            | true => Decision::Go,
                            | false => Decision::NoGo,
                        };
                        Analysis {
                            decision,
                            estimators,
                            host: first.host.clone(),
                            observation_schema_version: OBSERVATION_SCHEMA_VERSION,
                            schema_version: 1,
                        }
                    })
            },
        )
    })
}
pub(crate) fn render_markdown(analysis: &Analysis) -> String {
    let decision = match analysis.decision {
        | Decision::Go => "GO",
        | Decision::NoGo => "NO-GO",
    };
    let rows = analysis
        .estimators
        .iter()
        .map(|estimator| {
            let selected = estimator
                .selected_model
                .map_or_else(|| "none".to_string(), |model| format!("{model:?}").to_lowercase());
            format!(
                "| {} | {:?} | {} | {} | {} |",
                estimator.key, estimator.decision, selected, estimator.baseline_bytes, estimator.fixed_worst_case_bytes
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let candidate_rows = analysis
        .estimators
        .iter()
        .flat_map(|estimator| {
            estimator.candidates.iter().map(|candidate| {
                format!(
                    "| {} | {:?} | {} | {:.3} | {:.3} | {:.2} | {:.0} | {} | {} | {} | {:.1}% |",
                    estimator.key,
                    candidate.model,
                    candidate.accepted,
                    candidate.intercept,
                    candidate.slope,
                    candidate.safety_multiplier,
                    candidate.mean_absolute_error_bytes,
                    candidate.underpredictions,
                    candidate.mix_underpredictions,
                    candidate.extrapolations,
                    candidate.median_reservation_reduction * 100.0,
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Memory-aware scheduling spike results\n\n## Decision\n\n**{decision}**\n\nSynthetic observations validate the harness only. The decision above uses real training and held-out observations, requires zero held-out underprediction, and requires at least a 20% median reservation reduction versus fixed worst-case reservation.\n\n## Measurement host\n\n- Xylem: {}\n- Kernel: {}\n- Architecture: {}\n- CPU: {}\n- Available parallelism: {}\n- Total memory bytes: {}\n- Page size bytes: {}\n- Cgroup version: {}\n\n## Estimators\n\n| Key | Decision | Selected model | Baseline bytes | Fixed worst-case bytes |\n|---|---|---|---:|---:|\n{rows}\n\n## Candidate evidence\n\n| Key | Model | Accepted | Intercept | Slope | Safety multiplier | Mean absolute error bytes | Case underpredictions | Mix underpredictions | Extrapolations | Median reduction |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n{candidate_rows}\n\nExact feature ranges, bucket peaks, maximum underprediction, and p95 overreservation are retained in the JSON analysis artifact.\n\n## Required operator additions\n\nBefore accepting this report, record the corpus coverage, delegated cgroup path, sanitized observation checksum, measurement noise, limitations, and operational recommendation.\n",
        analysis.host.xylem_version,
        analysis.host.kernel_release,
        analysis.host.architecture,
        analysis.host.cpu_model,
        analysis.host.available_parallelism,
        analysis.host.total_memory_bytes,
        analysis.host.page_size_bytes,
        analysis.host.cgroup_version,
    )
}
fn analyze_estimator(manifest: &Manifest, observations: &[Observation], estimator: &Estimator) -> Result<EstimatorAnalysis, String> {
    let baseline = subject_peak(observations, SubjectType::Case, &estimator.baseline_case_id)
        .ok_or_else(|| format!("estimator {} has no successful baseline observations", estimator.key));
    baseline.and_then(|baseline_bytes| {
        let peaks = manifest
            .cases
            .iter()
            .filter(|case| case.estimator_key == estimator.key && matches!(case.partition, Partition::Training | Partition::Holdout))
            .map(|case| {
                subject_peak(observations, SubjectType::Case, &case.case_id)
                    .map(|peak| CasePeak {
                        case_id: &case.case_id,
                        features: &case.features,
                        incremental_bytes: peak.saturating_sub(baseline_bytes),
                        partition: case.partition,
                    })
                    .ok_or_else(|| format!("case {} has no successful observations", case.case_id))
            })
            .collect::<Result<Vec<_>, _>>();
        peaks.map(|peaks| {
            let fixed_worst_case_bytes = peaks.iter().map(|peak| peak.incremental_bytes).max().unwrap_or_default();
            let context = EvaluationContext {
                baseline_bytes,
                estimator,
                fixed_worst_case_bytes,
                manifest,
                observations,
                peaks: &peaks,
            };
            let kinds = [ModelKind::DecodedBytes, ModelKind::InputBytes, ModelKind::SizeBucket];
            let evaluations = kinds
                .into_iter()
                .filter_map(|kind| fit(kind, &peaks).map(|model| (kind, model)))
                .map(|(kind, model)| evaluate_candidate(&context, kind, &model))
                .collect::<Vec<_>>();
            let selected_model = evaluations
                .iter()
                .filter(|candidate| candidate.accepted)
                .min_by(|left, right| {
                    right
                        .median_reservation_reduction
                        .partial_cmp(&left.median_reservation_reduction)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| left.model.cmp(&right.model))
                })
                .map(|candidate| candidate.model);
            EstimatorAnalysis {
                baseline_bytes,
                candidates: evaluations,
                decision: selected_model.map_or(Decision::NoGo, |_| Decision::Go),
                fixed_worst_case_bytes,
                key: estimator.key.clone(),
                modules: estimator.modules.clone(),
                required_for_go: estimator.required_for_go,
                selected_model,
            }
        })
    })
}
fn evaluate_candidate(context: &EvaluationContext<'_>, kind: ModelKind, model: &CandidateModel) -> CandidateAnalysis {
    let holdout = context
        .peaks
        .iter()
        .filter(|peak| peak.partition == Partition::Holdout)
        .collect::<Vec<_>>();
    let reservations = holdout
        .iter()
        .map(|peak| (peak, model.reserve(feature(kind, peak.features))))
        .collect::<Vec<_>>();
    let extrapolations = reservations.iter().filter(|(_, reservation)| reservation.is_none()).count();
    let underpredictions = reservations
        .iter()
        .filter(|(peak, reservation)| reservation.is_some_and(|reservation| reservation < peak.incremental_bytes))
        .count();
    let maximum_underprediction_bytes = reservations
        .iter()
        .filter_map(|(peak, reservation)| reservation.map(|reservation| peak.incremental_bytes.saturating_sub(reservation)))
        .max()
        .unwrap_or_default();
    let absolute_errors = reservations
        .iter()
        .filter_map(|(peak, reservation)| reservation.map(|reservation| reservation.abs_diff(peak.incremental_bytes)))
        .collect::<Vec<_>>();
    let mean_absolute_error_bytes = match absolute_errors.is_empty() {
        | true => 0.0,
        | false => absolute_errors.iter().map(|value| *value as f64).sum::<f64>() / absolute_errors.len() as f64,
    };
    let p95_overreservation_bytes = percentile(
        reservations
            .iter()
            .filter_map(|(peak, reservation)| reservation.map(|reservation| reservation.saturating_sub(peak.incremental_bytes)))
            .collect(),
        95,
    );
    let reductions = reservations
        .iter()
        .filter_map(|(_, reservation)| *reservation)
        .map(|reservation| 1.0 - reservation as f64 / context.fixed_worst_case_bytes.max(1) as f64)
        .collect::<Vec<_>>();
    let median_reservation_reduction = median(reductions);
    let peak_by_id = context.peaks.iter().map(|peak| (peak.case_id, peak)).collect::<BTreeMap<_, _>>();
    let mix_underpredictions = context
        .manifest
        .mixes
        .iter()
        .filter(|mix| mix.estimator_key == context.estimator.key && mix.partition == Partition::Holdout)
        .filter(|mix| {
            let reserved = mix.case_ids.iter().try_fold(context.baseline_bytes, |total, case_id| {
                peak_by_id
                    .get(case_id.as_str())
                    .and_then(|peak| model.reserve(feature(kind, peak.features)))
                    .and_then(|reservation| total.checked_add(reservation))
            });
            let observed = subject_peak(context.observations, SubjectType::Mix, &mix.mix_id);
            matches!((reserved, observed), (Some(reserved), Some(observed)) if reserved < observed) || reserved.is_none() || observed.is_none()
        })
        .count();
    let (intercept, slope, safety_multiplier) = model.coefficients();
    let (feature_minimum, feature_maximum) = model.training_range();
    let accepted =
        extrapolations == 0 && underpredictions == 0 && mix_underpredictions == 0 && median_reservation_reduction >= REQUIRED_MEDIAN_REDUCTION;
    CandidateAnalysis {
        accepted,
        bucket_training_peaks: model.bucket_training_peaks(),
        extrapolations,
        feature_maximum,
        feature_minimum,
        intercept,
        maximum_underprediction_bytes,
        mean_absolute_error_bytes,
        median_reservation_reduction,
        mix_underpredictions,
        model: kind,
        p95_overreservation_bytes,
        safety_multiplier,
        slope,
        underpredictions,
    }
}
fn feature(kind: ModelKind, features: &InputFeatures) -> u64 {
    match kind {
        | ModelKind::DecodedBytes => features.decoded_bytes,
        | ModelKind::InputBytes | ModelKind::SizeBucket => features.input_bytes,
    }
}
fn feature_bucket(value: u64) -> u64 {
    value.max(1).checked_next_power_of_two().unwrap_or(u64::MAX)
}
fn fit(kind: ModelKind, peaks: &[CasePeak<'_>]) -> Option<CandidateModel> {
    let training = peaks.iter().filter(|peak| peak.partition == Partition::Training).collect::<Vec<_>>();
    let range = (
        training.iter().map(|peak| feature(kind, peak.features)).min(),
        training.iter().map(|peak| feature(kind, peak.features)).max(),
    );
    match (kind, range) {
        | (ModelKind::SizeBucket, (Some(minimum_feature), Some(maximum_feature))) => {
            let buckets = training.iter().fold(BTreeMap::new(), |mut buckets, peak| {
                let bucket = feature_bucket(feature(kind, peak.features));
                let maximum = buckets.entry(bucket).or_insert(0);
                *maximum = (*maximum).max(peak.incremental_bytes);
                buckets
            });
            Some(CandidateModel::Bucket {
                buckets,
                maximum_feature,
                minimum_feature,
                safety_multiplier: 1.05,
            })
        }
        | (ModelKind::DecodedBytes | ModelKind::InputBytes, (Some(minimum_feature), Some(maximum_feature))) => {
            let count = training.len() as f64;
            let mean_x = training.iter().map(|peak| feature(kind, peak.features) as f64).sum::<f64>() / count;
            let mean_y = training.iter().map(|peak| peak.incremental_bytes as f64).sum::<f64>() / count;
            let numerator = training
                .iter()
                .map(|peak| (feature(kind, peak.features) as f64 - mean_x) * (peak.incremental_bytes as f64 - mean_y))
                .sum::<f64>();
            let denominator = training
                .iter()
                .map(|peak| (feature(kind, peak.features) as f64 - mean_x).powi(2))
                .sum::<f64>();
            let slope = match denominator > 0.0 {
                | true => (numerator / denominator).max(0.0),
                | false => 0.0,
            };
            let intercept = (mean_y - slope * mean_x).max(0.0);
            let ratio = training
                .iter()
                .map(|peak| {
                    let predicted = (intercept + slope * feature(kind, peak.features) as f64).max(1.0);
                    peak.incremental_bytes as f64 / predicted
                })
                .fold(1.0_f64, f64::max);
            let safety_multiplier = (ratio * 20.0).ceil() / 20.0 + 0.05;
            Some(CandidateModel::Linear {
                intercept,
                maximum_feature,
                minimum_feature,
                safety_multiplier,
                slope,
            })
        }
        | (_, _) => None,
    }
}
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    match values.len() {
        | 0 => 0.0,
        | length if length % 2 == 0 => (values[length / 2 - 1] + values[length / 2]) / 2.0,
        | length => values[length / 2],
    }
}
fn percentile(mut values: Vec<u64>, percentile: usize) -> u64 {
    values.sort_unstable();
    match values.len() {
        | 0 => 0,
        | length => values[(length * percentile).div_ceil(100).saturating_sub(1).min(length - 1)],
    }
}
fn round_up_mib(value: u64) -> u64 {
    value.saturating_add(MIB - 1) / MIB * MIB
}
fn subject_peak(observations: &[Observation], subject_type: SubjectType, subject_id: &str) -> Option<u64> {
    observations
        .iter()
        .filter(|observation| {
            observation.subject_type == subject_type
                && observation.subject_id == subject_id
                && observation.command_succeeded
                && observation.oom_events == 0
                && observation.oom_kill_events == 0
        })
        .map(|observation| observation.peak_bytes)
        .max()
}
fn validate_observations(manifest: &Manifest, observations: &[Observation]) -> Result<(), String> {
    let hosts = observations.iter().map(|observation| &observation.host).collect::<BTreeSet<_>>();
    let invalid = observations.iter().find(|observation| {
        observation.schema_version != OBSERVATION_SCHEMA_VERSION
            || observation.peak_bytes == 0
            || observation.sampled_current_peak_bytes > observation.peak_bytes
            || observation.anon_peak_bytes > observation.sampled_current_peak_bytes
            || observation.file_peak_bytes > observation.sampled_current_peak_bytes
            || observation.host.cgroup_version != 2
            || observation.host.available_parallelism == 0
            || observation.host.page_size_bytes == 0
            || observation.host.total_memory_bytes == 0
            || observation.host.architecture.trim().is_empty()
            || observation.host.cpu_model.trim().is_empty()
            || observation.host.kernel_release.trim().is_empty()
            || observation.host.xylem_version.trim().is_empty()
            || manifest.estimator(&observation.estimator_key).is_none()
            || match observation.subject_type {
                | SubjectType::Case => manifest
                    .case(&observation.subject_id)
                    .is_none_or(|case| case.estimator_key != observation.estimator_key || case.partition != observation.partition),
                | SubjectType::Mix => manifest
                    .mixes
                    .iter()
                    .find(|mix| mix.mix_id == observation.subject_id)
                    .is_none_or(|mix| mix.estimator_key != observation.estimator_key || mix.partition != observation.partition),
            }
    });
    match (hosts.len(), invalid) {
        | (1, None) => validate_observation_sets(manifest, observations),
        | (count, None) => Err(format!("observations must describe exactly one host and Xylem version, found {count}")),
        | (_, Some(observation)) => Err(format!(
            "observation {} has an unsupported schema or unknown subject",
            observation.observation_id
        )),
    }
}
fn validate_observation_sets(manifest: &Manifest, observations: &[Observation]) -> Result<(), String> {
    let cases = manifest.cases.iter().map(|case| {
        let matching = observations
            .iter()
            .filter(|observation| observation.subject_type == SubjectType::Case && observation.subject_id == case.case_id)
            .collect::<Vec<_>>();
        validate_observation_set(&case.case_id, &matching, CASE_REPETITIONS, 1)
    });
    let mixes = manifest.mixes.iter().map(|mix| {
        let matching = observations
            .iter()
            .filter(|observation| observation.subject_type == SubjectType::Mix && observation.subject_id == mix.mix_id)
            .collect::<Vec<_>>();
        validate_observation_set(&mix.mix_id, &matching, MIX_REPETITIONS, mix.case_ids.len())
    });
    cases.chain(mixes).collect::<Result<Vec<_>, _>>().map(|_| ())
}
fn validate_observation_set(subject_id: &str, observations: &[&Observation], repetitions: usize, concurrency: usize) -> Result<(), String> {
    let seen = observations.iter().map(|observation| observation.repetition).collect::<BTreeSet<_>>();
    let expected = (0..repetitions).collect::<BTreeSet<_>>();
    let checksums = observations
        .iter()
        .map(|observation| observation.config_sha256.as_str())
        .collect::<BTreeSet<_>>();
    let invalid_checksum = checksums
        .iter()
        .any(|checksum| checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    let failed = observations.iter().any(|observation| {
        !observation.command_succeeded || observation.oom_events != 0 || observation.oom_kill_events != 0 || observation.concurrency != concurrency
    });
    match (
        observations.len() == repetitions,
        seen == expected,
        checksums.len() == 1,
        invalid_checksum,
        failed,
    ) {
        | (true, true, true, false, false) => Ok(()),
        | _ => Err(format!(
            "subject {subject_id} requires {repetitions} unique successful observations with one valid checksum and concurrency {concurrency}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Case, Estimator, HostIdentity, InputFeatures, Manifest, Mix, ModuleIdentity, Observation, RasterFeature, SubjectType};
    use std::path::PathBuf;

    fn case(case_id: &str, partition: Partition, input_bytes: u64, decoded_bytes: u64) -> Case {
        let rasters = match decoded_bytes {
            | 0 => Vec::new(),
            | width => vec![RasterFeature {
                bands: 1,
                bits_per_sample: 8,
                compression: "none".to_string(),
                format: "tiff".to_string(),
                height: 1,
                width,
            }],
        };
        Case {
            case_id: case_id.to_string(),
            config: PathBuf::from("config.json"),
            estimator_key: "imagery".to_string(),
            features: InputFeatures {
                decoded_bytes,
                file_count: 1,
                input_bytes,
                rasters,
            },
            input: (partition != Partition::Baseline).then(|| PathBuf::from(case_id)),
            partition,
        }
    }
    fn manifest() -> Manifest {
        Manifest {
            cases: vec![
                case("baseline", Partition::Baseline, 0, 0),
                case("train-a", Partition::Training, 10, 10),
                case("train-b", Partition::Training, 20, 20),
                case("train-c", Partition::Training, 30, 30),
                case("holdout-a", Partition::Holdout, 15, 15),
                case("holdout-b", Partition::Holdout, 25, 25),
            ],
            estimators: vec![Estimator {
                baseline_case_id: "baseline".to_string(),
                key: "imagery".to_string(),
                modules: vec![ModuleIdentity {
                    name: "probe".to_string(),
                    revision: "abc".to_string(),
                    version: "1.0.0".to_string(),
                }],
                required_for_go: true,
            }],
            mixes: vec![Mix {
                case_ids: vec!["holdout-a".to_string(), "holdout-b".to_string()],
                estimator_key: "imagery".to_string(),
                mix_id: "mix".to_string(),
                partition: Partition::Holdout,
            }],
            schema_version: 1,
        }
    }
    fn observation(subject_id: &str, subject_type: SubjectType, peak_bytes: u64) -> Observation {
        let partition = match subject_id {
            | "baseline" => Partition::Baseline,
            | "holdout-a" | "holdout-b" | "mix" => Partition::Holdout,
            | _ => Partition::Training,
        };
        Observation {
            anon_peak_bytes: peak_bytes,
            command_succeeded: true,
            concurrency: 1,
            config_sha256: "0".repeat(64),
            duration_millis: 1,
            estimator_key: "imagery".to_string(),
            exit_code: Some(0),
            file_peak_bytes: 0,
            host: HostIdentity {
                architecture: "test".to_string(),
                available_parallelism: 1,
                cgroup_version: 2,
                cpu_model: "test".to_string(),
                kernel_release: "test".to_string(),
                page_size_bytes: 4096,
                total_memory_bytes: 1024 * MIB,
                xylem_version: "xylem 1.0.3".to_string(),
            },
            observation_id: format!("{subject_id}-0"),
            oom_events: 0,
            oom_kill_events: 0,
            partition,
            peak_bytes,
            repetition: 0,
            sampled_current_peak_bytes: peak_bytes,
            schema_version: 1,
            subject_id: subject_id.to_string(),
            subject_type,
        }
    }
    fn repeated(observations: Vec<Observation>) -> Vec<Observation> {
        observations
            .into_iter()
            .flat_map(|observation| {
                let repetitions = match observation.subject_type {
                    | SubjectType::Case => CASE_REPETITIONS,
                    | SubjectType::Mix => MIX_REPETITIONS,
                };
                (0..repetitions).map(move |repetition| {
                    let mut repeated = observation.clone();
                    repeated.observation_id = format!("{}-{repetition}", observation.subject_id);
                    repeated.repetition = repetition;
                    repeated.concurrency = match observation.subject_type {
                        | SubjectType::Case => 1,
                        | SubjectType::Mix => 2,
                    };
                    repeated
                })
            })
            .collect()
    }
    #[test]
    fn accepts_a_safe_useful_model() {
        let manifest = manifest();
        let mib = MIB;
        let observations = repeated(vec![
            observation("baseline", SubjectType::Case, 10 * mib),
            observation("train-a", SubjectType::Case, 20 * mib),
            observation("train-b", SubjectType::Case, 30 * mib),
            observation("train-c", SubjectType::Case, 40 * mib),
            observation("holdout-a", SubjectType::Case, 25 * mib),
            observation("holdout-b", SubjectType::Case, 35 * mib),
            observation("mix", SubjectType::Mix, 48 * mib),
        ]);
        let analysis = analyze(&manifest, &observations).expect("analysis should succeed");
        assert_eq!(analysis.decision, Decision::Go);
        assert!(analysis.estimators[0].selected_model.is_some());
    }
    #[test]
    fn failed_or_incomplete_observations_block_analysis() {
        let manifest = manifest();
        let mib = MIB;
        let observations = repeated(vec![
            observation("baseline", SubjectType::Case, 10 * mib),
            observation("train-a", SubjectType::Case, 20 * mib),
            observation("train-b", SubjectType::Case, 30 * mib),
            observation("train-c", SubjectType::Case, 40 * mib),
            observation("holdout-a", SubjectType::Case, 25 * mib),
            observation("holdout-b", SubjectType::Case, 35 * mib),
            observation("mix", SubjectType::Mix, 48 * mib),
        ]);
        let mut failed = observations.clone();
        failed[0].oom_events = 1;
        failed[0].command_succeeded = false;
        assert!(analyze(&manifest, &failed).is_err());
        let mut incomplete = observations;
        incomplete.pop();
        assert!(analyze(&manifest, &incomplete).is_err());
    }
    #[test]
    fn rejects_holdout_extrapolation() {
        let mut manifest = manifest();
        manifest
            .cases
            .iter_mut()
            .find(|case| case.case_id == "holdout-b")
            .expect("holdout case")
            .features
            .input_bytes = 40;
        manifest
            .cases
            .iter_mut()
            .find(|case| case.case_id == "holdout-b")
            .expect("holdout case")
            .features
            .decoded_bytes = 40;
        let mib = MIB;
        let observations = repeated(vec![
            observation("baseline", SubjectType::Case, 10 * mib),
            observation("train-a", SubjectType::Case, 20 * mib),
            observation("train-b", SubjectType::Case, 30 * mib),
            observation("train-c", SubjectType::Case, 40 * mib),
            observation("holdout-a", SubjectType::Case, 25 * mib),
            observation("holdout-b", SubjectType::Case, 50 * mib),
            observation("mix", SubjectType::Mix, 65 * mib),
        ]);
        let analysis = analyze(&manifest, &observations).expect("analysis should succeed");
        assert_eq!(analysis.decision, Decision::NoGo);
        assert!(analysis.estimators[0].candidates.iter().all(|candidate| !candidate.accepted));
    }
    #[test]
    fn rounds_safety_and_reservations_up() {
        assert_eq!(round_up_mib(1), MIB);
        assert_eq!(round_up_mib(MIB), MIB);
        assert_eq!(round_up_mib(MIB + 1), 2 * MIB);
        let peaks = [
            CasePeak {
                case_id: "a",
                features: &InputFeatures {
                    decoded_bytes: 10,
                    file_count: 1,
                    input_bytes: 10,
                    rasters: Vec::new(),
                },
                incremental_bytes: 11,
                partition: Partition::Training,
            },
            CasePeak {
                case_id: "b",
                features: &InputFeatures {
                    decoded_bytes: 20,
                    file_count: 1,
                    input_bytes: 20,
                    rasters: Vec::new(),
                },
                incremental_bytes: 19,
                partition: Partition::Training,
            },
        ];
        let model = fit(ModelKind::InputBytes, &peaks).expect("model should fit");
        let (_, _, safety) = model.coefficients();
        assert!(safety >= 1.05);
        assert_eq!(model.reserve(15).expect("in-range reservation") % MIB, 0);
    }
    #[test]
    fn synthetic_observations_do_not_change_the_decision() {
        let manifest = manifest();
        let mib = MIB;
        let mut observations = repeated(vec![
            observation("baseline", SubjectType::Case, 10 * mib),
            observation("train-a", SubjectType::Case, 20 * mib),
            observation("train-b", SubjectType::Case, 30 * mib),
            observation("train-c", SubjectType::Case, 40 * mib),
            observation("holdout-a", SubjectType::Case, 25 * mib),
            observation("holdout-b", SubjectType::Case, 35 * mib),
            observation("mix", SubjectType::Mix, 48 * mib),
        ]);
        let without_synthetic = analyze(&manifest, &observations).expect("analysis should succeed");
        let mut manifest_with_synthetic = manifest.clone();
        manifest_with_synthetic.cases.push(case("synthetic", Partition::Synthetic, 1, 1));
        let mut synthetic = observation("synthetic", SubjectType::Case, 10_000 * mib);
        synthetic.partition = Partition::Synthetic;
        synthetic.observation_id = "synthetic".to_string();
        observations.extend(repeated(vec![synthetic]));
        let with_synthetic = analyze(&manifest_with_synthetic, &observations).expect("analysis should succeed");
        assert_eq!(without_synthetic.decision, with_synthetic.decision);
        assert_eq!(
            without_synthetic.estimators[0].selected_model,
            with_synthetic.estimators[0].selected_model
        );
    }
}
