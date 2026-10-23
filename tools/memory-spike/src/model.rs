use alloc::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

pub(crate) const OBSERVATION_SCHEMA_VERSION: u32 = 1;
pub(crate) const SPIKE_SCHEMA_VERSION: u32 = 1;
pub(crate) const CASE_REPETITIONS: usize = 5;
pub(crate) const MIX_REPETITIONS: usize = 3;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Case {
    pub(crate) case_id: String,
    pub(crate) config: PathBuf,
    pub(crate) estimator_key: String,
    pub(crate) features: InputFeatures,
    pub(crate) input: Option<PathBuf>,
    pub(crate) partition: Partition,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Estimator {
    pub(crate) baseline_case_id: String,
    pub(crate) key: String,
    pub(crate) modules: Vec<ModuleIdentity>,
    pub(crate) required_for_go: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputFeatures {
    pub(crate) decoded_bytes: u64,
    pub(crate) file_count: u64,
    pub(crate) input_bytes: u64,
    pub(crate) rasters: Vec<RasterFeature>,
}
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostIdentity {
    pub(crate) architecture: String,
    pub(crate) available_parallelism: usize,
    pub(crate) cgroup_version: u8,
    pub(crate) cpu_model: String,
    pub(crate) kernel_release: String,
    pub(crate) page_size_bytes: u64,
    pub(crate) total_memory_bytes: u64,
    pub(crate) xylem_version: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) cases: Vec<Case>,
    pub(crate) estimators: Vec<Estimator>,
    #[serde(default)]
    pub(crate) mixes: Vec<Mix>,
    pub(crate) schema_version: u32,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Mix {
    pub(crate) case_ids: Vec<String>,
    pub(crate) estimator_key: String,
    pub(crate) mix_id: String,
    pub(crate) partition: Partition,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModuleIdentity {
    pub(crate) name: String,
    pub(crate) revision: String,
    pub(crate) version: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RasterFeature {
    pub(crate) bands: u64,
    pub(crate) bits_per_sample: u16,
    pub(crate) compression: String,
    pub(crate) format: String,
    pub(crate) height: u64,
    pub(crate) width: u64,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Partition {
    Baseline,
    Holdout,
    Synthetic,
    Training,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub(crate) anon_peak_bytes: u64,
    pub(crate) command_succeeded: bool,
    pub(crate) concurrency: usize,
    pub(crate) config_sha256: String,
    pub(crate) duration_millis: u64,
    pub(crate) estimator_key: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) file_peak_bytes: u64,
    pub(crate) host: HostIdentity,
    pub(crate) observation_id: String,
    pub(crate) oom_events: u64,
    pub(crate) oom_kill_events: u64,
    pub(crate) partition: Partition,
    pub(crate) peak_bytes: u64,
    pub(crate) repetition: usize,
    pub(crate) sampled_current_peak_bytes: u64,
    pub(crate) schema_version: u32,
    pub(crate) subject_id: String,
    pub(crate) subject_type: SubjectType,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubjectType {
    Case,
    Mix,
}

impl Manifest {
    pub(crate) fn case(&self, case_id: &str) -> Option<&Case> {
        self.cases.iter().find(|case| case.case_id == case_id)
    }
    pub(crate) fn estimator(&self, key: &str) -> Option<&Estimator> {
        self.estimators.iter().find(|estimator| estimator.key == key)
    }
    pub(crate) fn read(path: &Path) -> Result<Self, String> {
        read_to_string(path)
            .map_err(|error| format!("read manifest {}: {error}", path.display()))
            .and_then(|content| serde_json::from_str(&content).map_err(|error| format!("parse manifest {}: {error}", path.display())))
            .map(|mut manifest: Self| {
                let parent = path.parent().unwrap_or_else(|| Path::new("."));
                manifest.cases.iter_mut().for_each(|case| {
                    if case.config.is_relative() {
                        case.config = parent.join(&case.config);
                    }
                    if let Some(input) = &case.input {
                        if input.is_relative() {
                            case.input = Some(parent.join(input));
                        }
                    }
                });
                manifest
            })
            .and_then(|manifest| manifest.validate().map(|()| manifest))
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        let estimator_keys = self.estimators.iter().map(|estimator| estimator.key.as_str()).collect::<BTreeSet<_>>();
        let case_ids = self.cases.iter().map(|case| case.case_id.as_str()).collect::<BTreeSet<_>>();
        let mix_ids = self.mixes.iter().map(|mix| mix.mix_id.as_str()).collect::<BTreeSet<_>>();
        let duplicate_estimator = self.estimators.len() != estimator_keys.len();
        let duplicate_case = self.cases.len() != case_ids.len();
        let duplicate_mix = self.mixes.len() != mix_ids.len();
        let missing_required_estimator = self.estimators.is_empty() || !self.estimators.iter().any(|estimator| estimator.required_for_go);
        let empty_identity = self.estimators.iter().any(|estimator| {
            !valid_identifier(&estimator.key)
                || estimator.modules.is_empty()
                || estimator
                    .modules
                    .iter()
                    .any(|module| module.name.trim().is_empty() || module.revision.trim().is_empty() || module.version.trim().is_empty())
        });
        let unknown_case_estimator = self.cases.iter().find(|case| !estimator_keys.contains(case.estimator_key.as_str()));
        let invalid_case = self.cases.iter().find(|case| {
            let real = matches!(case.partition, Partition::Holdout | Partition::Training);
            let baseline = case.partition == Partition::Baseline;
            !valid_identifier(&case.case_id)
                || case.config.extension().is_none_or(|extension| extension != "json")
                || (!baseline && case.input.is_none())
                || (baseline
                    && (case.features.file_count != 0
                        || case.features.input_bytes != 0
                        || case.features.decoded_bytes != 0
                        || !case.features.rasters.is_empty()))
                || (!baseline && case.features.file_count == 0)
                || (real && (case.features.input_bytes == 0 || case.features.decoded_bytes == 0))
                || (real && case.features.rasters.is_empty())
                || case.features.decoded_bytes_checked() != Some(case.features.decoded_bytes)
        });
        let invalid_baseline = self.estimators.iter().find(|estimator| {
            self.case(&estimator.baseline_case_id)
                .is_none_or(|case| case.estimator_key != estimator.key || case.partition != Partition::Baseline || case.input.is_some())
        });
        let invalid_mix = self.mixes.iter().find(|mix| {
            let unique_cases = mix.case_ids.iter().collect::<BTreeSet<_>>();
            !valid_identifier(&mix.mix_id)
                || mix.case_ids.len() < 2
                || unique_cases.len() != mix.case_ids.len()
                || mix.partition != Partition::Holdout
                || !estimator_keys.contains(mix.estimator_key.as_str())
                || mix.case_ids.iter().any(|case_id| {
                    self.case(case_id).is_none_or(|case| {
                        case.estimator_key != mix.estimator_key
                            || case.partition == Partition::Baseline
                            || case.config != self.case(&mix.case_ids[0]).map_or_else(PathBuf::new, |first| first.config.clone())
                    })
                })
        });
        let baseline_ids = self
            .estimators
            .iter()
            .map(|estimator| estimator.baseline_case_id.as_str())
            .collect::<BTreeSet<_>>();
        let additional_error = self
            .cases
            .iter()
            .find(|case| case.partition == Partition::Baseline && !baseline_ids.contains(case.case_id.as_str()))
            .map(|case| format!("baseline case {} is not referenced by an estimator", case.case_id))
            .or_else(|| {
                self.estimators.iter().find_map(|estimator| {
                    let configs = self
                        .cases
                        .iter()
                        .filter(|case| case.estimator_key == estimator.key && matches!(case.partition, Partition::Holdout | Partition::Training))
                        .map(|case| &case.config)
                        .collect::<BTreeSet<_>>();
                    (configs.len() != 1).then(|| format!("real cases for estimator {} must use one measurement configuration", estimator.key))
                })
            });
        match (
            self.schema_version,
            duplicate_estimator,
            duplicate_case,
            duplicate_mix,
            missing_required_estimator,
            empty_identity,
            unknown_case_estimator,
            invalid_case,
            invalid_baseline,
            invalid_mix,
            additional_error,
        ) {
            | (version, _, _, _, _, _, _, _, _, _, _) if version != SPIKE_SCHEMA_VERSION => {
                Err(format!("unsupported manifest schema version {version}"))
            }
            | (_, true, _, _, _, _, _, _, _, _, _) => Err("estimator keys must be unique".to_string()),
            | (_, _, true, _, _, _, _, _, _, _, _) => Err("case IDs must be unique".to_string()),
            | (_, _, _, true, _, _, _, _, _, _, _) => Err("mix IDs must be unique".to_string()),
            | (_, _, _, _, true, _, _, _, _, _, _) => Err("at least one estimator must be required for the go decision".to_string()),
            | (_, _, _, _, _, true, _, _, _, _, _) => Err("estimator and module identities must be nonempty".to_string()),
            | (_, _, _, _, _, _, Some(case), _, _, _, _) => Err(format!("case {} references unknown estimator {}", case.case_id, case.estimator_key)),
            | (_, _, _, _, _, _, _, Some(case), _, _, _) => Err(format!("case {} has invalid identity, config, input, or features", case.case_id)),
            | (_, _, _, _, _, _, _, _, Some(estimator), _, _) => {
                Err(format!("estimator {} must reference its own input-free baseline case", estimator.key))
            }
            | (_, _, _, _, _, _, _, _, _, Some(mix), _) => {
                Err(format!("mix {} must contain at least two distinct compatible holdout cases", mix.mix_id))
            }
            | (_, _, _, _, _, _, _, _, _, _, Some(error)) => Err(error),
            | _ => self.validate_coverage(),
        }
    }
    fn validate_coverage(&self) -> Result<(), String> {
        self.estimators.iter().try_for_each(|estimator| {
            let counts =
                self.cases
                    .iter()
                    .filter(|case| case.estimator_key == estimator.key)
                    .fold(BTreeMap::<&str, usize>::new(), |mut counts, case| {
                        let key = match case.partition {
                            | Partition::Baseline => "baseline",
                            | Partition::Holdout => "holdout",
                            | Partition::Synthetic => "synthetic",
                            | Partition::Training => "training",
                        };
                        let count = counts.entry(key).or_default();
                        *count = count.saturating_add(1);
                        counts
                    });
            let training = counts.get("training").copied().unwrap_or_default();
            let holdout = counts.get("holdout").copied().unwrap_or_default();
            match (training >= 3, holdout >= 1) {
                | (true, true) => Ok(()),
                | _ => Err(format!(
                    "estimator {} requires at least three training cases and one holdout case",
                    estimator.key
                )),
            }
        })
    }
}
impl InputFeatures {
    fn decoded_bytes_checked(&self) -> Option<u64> {
        self.rasters.iter().try_fold(0_u64, |total, raster| {
            let bytes_per_sample = u64::from(raster.bits_per_sample).checked_add(7).map(|bits| bits / 8);
            match (
                raster.width > 0,
                raster.height > 0,
                raster.bands > 0,
                raster.bits_per_sample > 0,
                !raster.format.trim().is_empty(),
                !raster.compression.trim().is_empty(),
                bytes_per_sample,
            ) {
                | (true, true, true, true, true, true, Some(bytes_per_sample)) => raster
                    .width
                    .checked_mul(raster.height)
                    .and_then(|pixels| pixels.checked_mul(raster.bands))
                    .and_then(|samples| samples.checked_mul(bytes_per_sample))
                    .and_then(|bytes| total.checked_add(bytes)),
                | _ => None,
            }
        })
    }
}
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_byte_calculation_rejects_overflow() {
        let features = InputFeatures {
            decoded_bytes: 0,
            file_count: 1,
            input_bytes: 1,
            rasters: vec![RasterFeature {
                bands: 2,
                bits_per_sample: 8,
                compression: "none".to_string(),
                format: "tiff".to_string(),
                height: 2,
                width: u64::MAX,
            }],
        };
        assert_eq!(features.decoded_bytes_checked(), None);
    }
    #[test]
    fn example_manifest_is_valid() {
        let manifest: Manifest = serde_json::from_str(include_str!("../manifest.example.json")).expect("example manifest should parse");
        assert_eq!(manifest.validate(), Ok(()));
    }
    #[test]
    fn manifest_rejects_path_like_identifiers() {
        let mut manifest: Manifest = serde_json::from_str(include_str!("../manifest.example.json")).expect("example manifest should parse");
        manifest.cases[0].case_id = "../escape".to_string();
        assert!(manifest.validate().is_err());
    }
}
