//! Safe reconciliation of expired OCI sandbox resources.
extern crate alloc;

use super::{
    EffectiveSandboxPolicy, SandboxError, SandboxLease, SandboxLeaseState, SandboxReconciliationOutcome, SandboxResourceDisposition,
    SandboxResourceEvidence, SandboxResourcePhase, SANDBOX_CREATED_LABEL, SANDBOX_INPUT_LABEL, SANDBOX_MANAGED_LABEL, SANDBOX_PHASE_LABEL,
    SANDBOX_POLICY_LABEL, SANDBOX_PROVIDER_LABEL, SANDBOX_RESOURCE_VERSION, SANDBOX_RUN_LABEL, SANDBOX_STAGING_LABEL, SANDBOX_VERSION_LABEL,
};
use crate::script::EnvironmentValue;
use crate::util::timestamp;
use acorn_cmd::{args, CommandSpec};
use alloc::collections::{BTreeMap, BTreeSet};
use core::time::Duration;
use std::fs::remove_dir_all;
use std::path::{Component, Path, PathBuf};

const MAX_RECONCILIATION_ERROR_BYTES: usize = 4096;

const RECONCILIATION_TIMEOUT: Duration = Duration::from_secs(10);
#[derive(Clone, Debug)]
pub(super) struct ReconciliationResource {
    pub(super) artifact_staging: Option<PathBuf>,
    pub(super) created_at: u64,
    pub(super) identifier: String,
    pub(super) input_index: usize,
    pub(super) phase: SandboxResourcePhase,
    pub(super) run_id: String,
}
#[derive(Debug, Default)]
struct RunCleanup {
    artifact_staging: BTreeSet<PathBuf>,
    ready: bool,
}
impl EffectiveSandboxPolicy {
    pub(super) fn argument(&self) -> Vec<std::ffi::OsString> {
        args![
            "ps",
            "--all",
            ("--filter", format!("label={SANDBOX_MANAGED_LABEL}=true")),
            ("--filter", format!("label={SANDBOX_POLICY_LABEL}={}", self.digest)),
            ("--filter", format!("label={SANDBOX_PROVIDER_LABEL}=oci")),
            ("--filter", format!("label={SANDBOX_VERSION_LABEL}={SANDBOX_RESOURCE_VERSION}")),
            ("--format", "{{.Names}}")
        ]
    }
    fn inspect(&self, identifier: &str) -> Result<ReconciliationResource, SandboxError> {
        self.runtime().and_then(|runtime| {
            CommandSpec::new(runtime.to_string())
                .args(args!["inspect", "--format", "{{json .Config.Labels}}", identifier])
                .checked_output_with_timeout(RECONCILIATION_TIMEOUT)
                .map_err(|why| SandboxError::Runtime(format!("inspect stale sandbox {identifier}: {why}")))
                .and_then(|output| {
                    serde_json::from_slice::<BTreeMap<String, String>>(&output.stdout)
                        .map_err(|why| SandboxError::Runtime(format!("parse labels for stale sandbox {identifier}: {why}")))
                })
                .and_then(|labels| self.parse_resource(identifier, &labels))
        })
    }
    fn list(&self) -> Result<Vec<String>, SandboxError> {
        self.runtime().and_then(|runtime| {
            CommandSpec::new(runtime.to_string())
                .args(self.argument())
                .checked_output_with_timeout(RECONCILIATION_TIMEOUT)
                .map_err(|why| SandboxError::Runtime(format!("list managed sandboxes: {why}")))
                .map(|output| {
                    let mut identifiers = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::trim)
                        .filter(|identifier| !identifier.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    identifiers.sort();
                    identifiers.dedup();
                    identifiers
                })
        })
    }
    pub(super) fn parse_resource(&self, identifier: &str, labels: &BTreeMap<String, String>) -> Result<ReconciliationResource, SandboxError> {
        let value = |name: &str| {
            labels
                .get(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| SandboxError::Runtime(format!("managed sandbox {identifier} is missing label {name}")))
        };
        value(SANDBOX_MANAGED_LABEL)
            .and_then(|managed| exact_label(identifier, SANDBOX_MANAGED_LABEL, managed, "true"))
            .and_then(|()| value(SANDBOX_POLICY_LABEL))
            .and_then(|digest| exact_label(identifier, SANDBOX_POLICY_LABEL, digest, &self.digest))
            .and_then(|()| value(SANDBOX_PROVIDER_LABEL))
            .and_then(|provider| exact_label(identifier, SANDBOX_PROVIDER_LABEL, provider, "oci"))
            .and_then(|()| value(SANDBOX_VERSION_LABEL))
            .and_then(|version| exact_label(identifier, SANDBOX_VERSION_LABEL, version, &SANDBOX_RESOURCE_VERSION.to_string()))
            .and_then(|()| value(SANDBOX_RUN_LABEL).map(|run_id| run_id.to_string()))
            .and_then(|run_id| {
                value(SANDBOX_INPUT_LABEL)
                    .and_then(|value| {
                        value
                            .parse::<usize>()
                            .map_err(|why| SandboxError::Runtime(format!("managed sandbox {identifier} has invalid input index: {why}")))
                    })
                    .map(|input_index| (run_id, input_index))
            })
            .and_then(|(run_id, input_index)| {
                value(SANDBOX_CREATED_LABEL)
                    .and_then(|value| {
                        value
                            .parse::<u64>()
                            .map_err(|why| SandboxError::Runtime(format!("managed sandbox {identifier} has invalid creation timestamp: {why}")))
                    })
                    .map(|created_at| (run_id, input_index, created_at))
            })
            .and_then(|(run_id, input_index, created_at)| {
                value(SANDBOX_PHASE_LABEL)
                    .and_then(|phase| match phase.as_str() {
                        | "execution" => Ok(SandboxResourcePhase::Execution),
                        | "preparation" => Ok(SandboxResourcePhase::Preparation),
                        | phase => Err(SandboxError::Runtime(format!("managed sandbox {identifier} has invalid phase {phase}"))),
                    })
                    .map(|phase| (run_id, input_index, created_at, phase))
            })
            .and_then(|(run_id, input_index, created_at, phase)| {
                validate_identity(identifier, &run_id, input_index, phase).map(|()| (run_id, input_index, created_at, phase))
            })
            .and_then(|(run_id, input_index, created_at, phase)| {
                labels
                    .get(SANDBOX_STAGING_LABEL)
                    .map(PathBuf::from)
                    .map(|path| validate_artifact_staging(self, &run_id, path))
                    .transpose()
                    .map(|artifact_staging| ReconciliationResource {
                        artifact_staging,
                        created_at,
                        identifier: identifier.to_string(),
                        input_index,
                        phase,
                        run_id,
                    })
            })
    }
    fn reconcile_inspected(
        &self,
        leases: &BTreeMap<&str, SandboxLeaseState>,
        inspected: Vec<ReconciliationResource>,
    ) -> SandboxReconciliationOutcome {
        let cutoff = timestamp()
            .get()
            .saturating_sub(self.policy.lifecycle.stale_after_seconds.saturating_mul(1000));
        let mut errors = Vec::new();
        let mut evidence = Vec::new();
        let mut runs = BTreeMap::<String, RunCleanup>::new();
        inspected.into_iter().for_each(|resource| {
            let eligible = is_eligible(resource.created_at, cutoff, leases.get(resource.run_id.as_str()).copied());
            let run = runs.entry(resource.run_id.clone()).or_insert_with(|| RunCleanup {
                artifact_staging: BTreeSet::new(),
                ready: true,
            });
            run.ready &= eligible;
            if let Some(staging) = resource.artifact_staging.as_ref() {
                run.artifact_staging.insert(staging.clone());
            }
            if eligible {
                match self.cleanup(&resource.identifier, true) {
                    | Ok(_) => evidence.push(SandboxResourceEvidence {
                        error: None,
                        identifier: resource.identifier,
                        input_index: resource.input_index,
                        phase: resource.phase,
                        status: SandboxResourceDisposition::Reconciled,
                    }),
                    | Err(why) => {
                        let why = why.to_string();
                        run.ready = false;
                        errors.push(why.clone());
                        evidence.push(SandboxResourceEvidence {
                            error: Some(bounded_error(&why)),
                            identifier: resource.identifier,
                            input_index: resource.input_index,
                            phase: resource.phase,
                            status: SandboxResourceDisposition::Failed,
                        });
                    }
                }
            }
        });
        runs.into_iter().filter(|(_, run)| run.ready).for_each(|(run_id, run)| {
            let workspace = std::env::temp_dir().join(EnvironmentValue::PREFIX).join(&run_id);
            core::iter::once(workspace)
                .chain(run.artifact_staging)
                .filter_map(|path| remove_tree(&path).err())
                .for_each(|why| errors.push(why.to_string()));
        });
        evidence.sort_by(|left, right| (left.input_index, left.phase, &left.identifier).cmp(&(right.input_index, right.phase, &right.identifier)));
        SandboxReconciliationOutcome {
            error: (!errors.is_empty()).then(|| bounded_error(&errors.join("; additionally, "))),
            resources: evidence,
        }
    }
}
fn bounded_error(value: &str) -> String {
    match value.len() <= MAX_RECONCILIATION_ERROR_BYTES {
        | true => value.to_string(),
        | false => {
            const SUFFIX: &str = "...";
            let mut end = MAX_RECONCILIATION_ERROR_BYTES - SUFFIX.len();
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}{SUFFIX}", &value[..end])
        }
    }
}
fn exact_label(identifier: &str, name: &str, value: &str, expected: &str) -> Result<(), SandboxError> {
    match value == expected {
        | true => Ok(()),
        | false => Err(SandboxError::Runtime(format!(
            "managed sandbox {identifier} has unexpected label {name}={value}"
        ))),
    }
}
pub(super) fn is_eligible(created_at: u64, cutoff: u64, lease: Option<SandboxLeaseState>) -> bool {
    created_at <= cutoff && !matches!(lease, Some(SandboxLeaseState::Active | SandboxLeaseState::Indeterminate))
}
fn remove_tree(path: &Path) -> Result<(), SandboxError> {
    match remove_dir_all(path) {
        | Ok(()) => Ok(()),
        | Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
        | Err(why) => Err(SandboxError::Runtime(format!("remove stale sandbox staging {}: {why}", path.display()))),
    }
}
pub(super) fn resources(policy: &EffectiveSandboxPolicy, leases: &[SandboxLease]) -> SandboxReconciliationOutcome {
    let leases = leases
        .iter()
        .map(|lease| (lease.run_id.as_str(), lease.state))
        .collect::<BTreeMap<_, _>>();
    let inspected = policy.list().and_then(|identifiers| {
        identifiers
            .into_iter()
            .map(|identifier| policy.inspect(&identifier))
            .collect::<Result<Vec<_>, _>>()
    });
    match inspected {
        | Err(why) => SandboxReconciliationOutcome {
            error: Some(bounded_error(&why.to_string())),
            resources: Vec::new(),
        },
        | Ok(inspected) => policy.reconcile_inspected(&leases, inspected),
    }
}
fn validate_artifact_staging(policy: &EffectiveSandboxPolicy, run_id: &str, path: PathBuf) -> Result<PathBuf, SandboxError> {
    let normalized = path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir));
    let expected = format!(".xylem-{run_id}-artifacts");
    let granted = policy.policy.paths.artifact_roots.iter().any(|root| path.starts_with(root));
    match normalized && granted && path.file_name().is_some_and(|name| name == expected.as_str()) {
        | true => Ok(path),
        | false => Err(SandboxError::Runtime(format!(
            "managed sandbox artifact staging path {} is not an exact policy-owned path",
            path.display()
        ))),
    }
}
fn validate_identity(identifier: &str, run_id: &str, input_index: usize, phase: SandboxResourcePhase) -> Result<(), SandboxError> {
    let run_id_valid = !run_id.is_empty() && run_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    let suffix = match phase {
        | SandboxResourcePhase::Execution => "exec",
        | SandboxResourcePhase::Preparation => "prep",
    };
    let expected = format!("xylem-{run_id}-{input_index}-{suffix}");
    match run_id_valid && identifier == expected {
        | true => Ok(()),
        | false => Err(SandboxError::Runtime(format!(
            "managed sandbox {identifier} does not match its lifecycle labels"
        ))),
    }
}
