use super::*;
use crate::acquisition::{AcquisitionCacheStatus, AcquisitionIdentity, AcquisitionLimits, AcquisitionPolicyEvidence};
use acorn_core::time::UnitOfTime;
use acorn_schema::validation::Validate;
use alloc::sync::Arc;
use std::fs::{create_dir_all, read, read_to_string, remove_dir, remove_file};

impl RunMetadata {
    fn sandbox(mut self) -> Self {
        self.sandbox.mode = "required".to_string();
        self.sandbox.lifecycle = Some(
            SandboxLifecycleEvidence::init()
                .retention(crate::sandbox::SandboxRetention::RetainOnFailure)
                .stale_after_seconds(60)
                .build(),
        );
        self
    }
}
fn memory(bytes: u64) -> Memory {
    Memory::kb(bytes as f64 / 1024.0)
}
fn memory_evidence() -> RunMemoryEvidence {
    RunMemoryEvidence {
        budget: memory(200),
        estimator_identity: "journal-test-v1".to_string(),
        inputs: vec![MemoryInputEvidence {
            estimated: Some(memory(120)),
            fallback: None,
            input: Some(memory(10)),
            input_index: 0,
            observed_peak: None,
            reservation: Some(memory(150)),
            safety_reserve: Some(memory(30)),
        }],
        mode: MemoryEvidenceMode::MemoryAware,
        safety_millionths: 1_250_000,
    }
}
fn acquisition_evidence() -> AcquisitionEvidence {
    AcquisitionEvidence {
        acquired_at_unix_seconds: 1,
        size: memory(64),
        cache: AcquisitionCacheStatus::Miss,
        entries: 1,
        identity: AcquisitionIdentity::Sha256 {
            digest: format!("sha256:{}", "a".repeat(64)),
        },
        limits: AcquisitionLimits::default(),
        module_path: vec!["group".to_string(), "module".to_string()],
        policy: AcquisitionPolicyEvidence {
            allow_insecure_http: false,
            git_ssh_enabled: false,
            offline: false,
            skip_checksum_verification: false,
        },
        source: "https://example.com/module".to_string(),
        tree_digest: None,
    }
}
fn metadata(run_id: &str) -> RunMetadata {
    RunMetadata {
        artifacts: None,
        configuration_acquisitions: Vec::new(),
        run_id: run_id.to_string(),
        configuration: ConfigurationIdentity {
            path: "config.json".to_string(),
            id: Some("workflow".to_string()),
            version: Some("1.0.0".to_string()),
        },
        dry_run: false,
        memory: None,
        sandbox: SandboxEvidence::off(),
        started_at: Milliseconds::new(1),
    }
}
fn module() -> JournalModule {
    JournalModule::new(0, "module".to_string(), vec!["group".to_string(), "module".to_string()])
}
fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-journal-{label}-{}-{}.jsonl", std::process::id(), timestamp()))
}

#[test]
fn test_artifact_finalization_is_exactly_once_and_recovered() {
    let path = path("artifacts");
    let pending = ArtifactPublication::init()
        .bytes_per_input(memory(1024))
        .destination("/artifacts/run")
        .entries_per_input(10)
        .status(ArtifactPublicationStatus::Pending)
        .build();
    let journal = RunJournal::create(
        path.clone(),
        metadata("run-artifacts").with_artifacts(pending.clone()),
        vec![JournalInput::new(0, None)],
        Vec::new(),
    )
    .expect("create artifact journal");
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    journal.transition_input(0, ExecutionState::Failed, Some("failed")).expect("fail input");
    let publication = ArtifactPublication::init()
        .bytes_per_input(pending.bytes_per_input.clone())
        .destination(pending.destination.clone())
        .entries_per_input(pending.entries_per_input)
        .status(ArtifactPublicationStatus::Discarded)
        .build();
    journal.finalize_artifacts(publication.clone()).expect("finalize artifacts");
    assert!(journal.finalize_artifacts(publication.clone()).is_err());
    journal.finish(FinalStatus::Failed, Some("failed")).expect("finish run");
    assert_eq!(RunJournal::read(&path).expect("recover summary").artifacts, Some(publication));
    remove_file(path).expect("remove journal");
}
#[test]
fn test_concurrent_writes_remain_valid_and_sequenced() {
    let path = path("concurrent");
    let inputs = (0..8).map(|index| JournalInput::new(index, Some(index.to_string()))).collect();
    let journal = Arc::new(RunJournal::create(path.clone(), metadata("run-concurrent"), inputs, Vec::new()).expect("create journal"));
    std::thread::scope(|scope| {
        for index in 0..8 {
            let journal = Arc::clone(&journal);
            scope.spawn(move || {
                journal.transition_input(index, ExecutionState::Running, None).expect("start input");
                journal.transition_input(index, ExecutionState::Succeeded, None).expect("finish input");
            });
        }
    });
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover concurrent journal");
    assert!(summary.inputs.iter().all(|input| input.state == ExecutionState::Succeeded));
    remove_file(path).expect("remove journal");
}
#[test]
fn test_console_report_groups_modules_by_input() {
    let path = path("console-inputs");
    let inputs = vec![
        JournalInput::new(0, Some("alpha".to_string())),
        JournalInput::new(1, Some("bravo".to_string())),
    ];
    let journal = RunJournal::create(path.clone(), metadata("run-console"), inputs, vec![module()]).expect("create journal");
    journal.finish(FinalStatus::Failed, Some("preparation failed")).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover journal");
    let mut output = Vec::new();
    summary.render_console_to(&mut output, &path, false).expect("render console report");
    let output = String::from_utf8(output).expect("console report should be UTF-8");
    assert!(output.contains("│ Input"), "{output}");
    assert!(output.contains("│ alpha"), "{output}");
    assert!(output.contains("│ bravo"), "{output}");
    assert!(output.contains("group::module"), "{output}");
    let mut colored = Vec::new();
    summary
        .render_console_to(&mut colored, &path, true)
        .expect("render colored console report");
    assert!(String::from_utf8(colored).expect("colored report should be UTF-8").contains("\u{1b}["));
    remove_file(path).expect("remove journal");
}
#[test]
fn test_container_states_are_recovered() {
    let path = path("containers");
    let containers = vec![JournalContainer {
        index: 0,
        name: "xylem-worker-run".to_string(),
        owned: true,
        runtime: "docker".to_string(),
    }];
    let journal =
        RunJournal::create_with_containers(path.clone(), metadata("run-containers"), Vec::new(), Vec::new(), containers).expect("create journal");
    journal.transition_container(0, ContainerState::Ready, None).expect("ready container");
    journal.transition_container(0, ContainerState::Removed, None).expect("remove container");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover journal");
    assert_eq!(summary.containers[0].state, ContainerState::Removed);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_error_details_are_bounded_without_splitting_utf8() {
    let path = path("bounded");
    let journal = RunJournal::create(path.clone(), metadata("run-bounded"), vec![JournalInput::new(0, None)], Vec::new()).expect("create journal");
    let detail = "é".repeat(MAX_ERROR_BYTES);
    journal.transition_input(0, ExecutionState::Failed, Some(&detail)).expect("fail input");
    journal.finish(FinalStatus::Failed, Some(&detail)).expect("fail run");
    let summary = RunJournal::read(&path).expect("recover journal");
    assert!(summary.error.expect("run error").len() <= MAX_ERROR_BYTES);
    assert!(summary.inputs[0].error.as_ref().expect("input error").len() <= MAX_ERROR_BYTES);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_existing_destination_and_invalid_transition_are_rejected() {
    let path = path("exclusive");
    let journal = RunJournal::create(path.clone(), metadata("run-exclusive"), vec![JournalInput::new(0, None)], Vec::new()).expect("create journal");
    assert!(RunJournal::create(path.clone(), metadata("other"), Vec::new(), Vec::new()).is_err());
    assert!(journal.transition_input(0, ExecutionState::Succeeded, None).is_err());
    remove_file(path).expect("remove journal");
}
#[test]
fn test_gpu_monitoring_aggregates_samples_by_uuid() {
    let path = path("gpu-aggregate");
    let journal = RunJournal::create(path.clone(), metadata("run-gpu"), Vec::new(), Vec::new()).expect("create journal");
    let device = GpuDevice {
        index: 0,
        name: "NVIDIA Test".to_string(),
        memory_total_mib: Some(8192),
        uuid: "GPU-a".to_string(),
    };
    journal.start_gpu_monitoring(Milliseconds::new(1000)).expect("start monitoring");
    journal.record_gpu_inventory(vec![device]).expect("record inventory");
    journal
        .record_gpu_sample(vec![GpuReading {
            memory_used_mib: Some(1024),
            utilization_percent: Some(25),
            uuid: "GPU-a".to_string(),
        }])
        .expect("record first sample");
    journal
        .record_gpu_sample(vec![GpuReading {
            memory_used_mib: Some(3072),
            utilization_percent: Some(75),
            uuid: "GPU-a".to_string(),
        }])
        .expect("record second sample");
    journal.finish_gpu_monitoring(None).expect("finish monitoring");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover journal");
    let monitoring = summary.gpu_monitoring.expect("GPU summary");
    assert_eq!(monitoring.status, GpuMonitoringStatus::Complete);
    assert_eq!(monitoring.devices[0].samples, 2);
    assert_eq!(
        monitoring.devices[0].utilization_percent,
        Some(GpuMetricSummary {
            average: 50,
            maximum: 75,
            samples: 2,
        })
    );
    assert_eq!(
        monitoring.devices[0].memory_used_mib,
        Some(GpuMetricSummary {
            average: 2048,
            maximum: 3072,
            samples: 2,
        })
    );
    remove_file(path).expect("remove journal");
}
#[test]
fn test_gpu_monitoring_recovers_unavailable_and_interrupted_states() {
    let unavailable_path = path("gpu-unavailable");
    let unavailable =
        RunJournal::create(unavailable_path.clone(), metadata("run-gpu-unavailable"), Vec::new(), Vec::new()).expect("create unavailable journal");
    unavailable
        .start_gpu_monitoring(Milliseconds::new(1000))
        .expect("start unavailable monitoring");
    unavailable
        .finish_gpu_monitoring(Some("nvidia-smi missing"))
        .expect("finish unavailable monitoring");
    unavailable
        .finish(FinalStatus::Failed, Some("GPU unavailable"))
        .expect("finish unavailable run");
    let summary = RunJournal::read(&unavailable_path).expect("recover unavailable journal");
    assert_eq!(
        summary.gpu_monitoring.as_ref().map(|monitoring| monitoring.status),
        Some(GpuMonitoringStatus::Unavailable)
    );
    remove_file(unavailable_path).expect("remove unavailable journal");

    let interrupted_path = path("gpu-interrupted");
    let interrupted =
        RunJournal::create(interrupted_path.clone(), metadata("run-gpu-interrupted"), Vec::new(), Vec::new()).expect("create interrupted journal");
    interrupted
        .start_gpu_monitoring(Milliseconds::new(1000))
        .expect("start interrupted monitoring");
    drop(interrupted);
    let summary = RunJournal::read(&interrupted_path).expect("recover interrupted journal");
    assert_eq!(
        summary.gpu_monitoring.as_ref().map(|monitoring| monitoring.status),
        Some(GpuMonitoringStatus::Interrupted)
    );
    remove_file(interrupted_path).expect("remove interrupted journal");
}
#[test]
fn test_gpu_monitoring_rejects_invalid_event_order_and_identity() {
    let path = path("gpu-invalid");
    let journal = RunJournal::create(path.clone(), metadata("run-gpu-invalid"), Vec::new(), Vec::new()).expect("create journal");
    assert!(journal.record_gpu_sample(Vec::new()).is_err());
    journal.start_gpu_monitoring(Milliseconds::new(1000)).expect("start monitoring");
    assert!(journal.record_gpu_sample(Vec::new()).is_err());
    journal
        .record_gpu_inventory(vec![GpuDevice {
            index: 0,
            name: "NVIDIA Test".to_string(),
            memory_total_mib: Some(8192),
            uuid: "GPU-a".to_string(),
        }])
        .expect("record inventory");
    assert!(journal
        .record_gpu_sample(vec![GpuReading {
            memory_used_mib: Some(1),
            utilization_percent: Some(1),
            uuid: "GPU-b".to_string(),
        }])
        .is_err());
    assert!(journal.finish(FinalStatus::Succeeded, None).is_err());
    drop(journal);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_interrupted_sandbox_summary_preserves_pending_lifecycle() {
    let path = path("sandbox-interrupted");
    let journal = RunJournal::create(path.clone(), metadata("run-sandbox-interrupted").sandbox(), Vec::new(), Vec::new())
        .expect("create interrupted sandbox journal");
    drop(journal);
    let summary = RunJournal::read(&path).expect("recover interrupted sandbox summary");
    assert_eq!(summary.status, RunStatus::Interrupted);
    assert!(!summary.sandbox.lifecycle.expect("sandbox lifecycle evidence").finalized);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_journal_jsonl_omits_none_fields() {
    let path = path("omit-none");
    let journal = RunJournal::create(path.clone(), metadata("run-omit-none"), vec![JournalInput::new(0, None)], Vec::new()).expect("create journal");
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    journal.transition_input(0, ExecutionState::Succeeded, None).expect("finish input");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    assert!(!read_to_string(&path).expect("read journal").contains("null"));
    remove_file(path).expect("remove journal");
}
#[test]
fn test_json_report_is_atomic_and_refuses_overwrite() {
    let directory = std::env::temp_dir().join(format!("xylem-journal-report-{}-{}.jsonl", std::process::id(), timestamp()));
    create_dir_all(&directory).expect("create reporter test directory");
    let summary = RunSummary {
        artifacts: None,
        configuration_acquisitions: Vec::new(),
        schema_version: JOURNAL_SCHEMA_VERSION,
        containers: Vec::new(),
        run_id: "atomic-report".to_string(),
        configuration: ConfigurationIdentity {
            path: "config.json".to_string(),
            id: None,
            version: None,
        },
        dry_run: false,
        sandbox: SandboxEvidence::off(),
        started_at: Milliseconds::new(1),
        finished_at: Some(Milliseconds::new(2)),
        duration: Some(Milliseconds::new(1)),
        status: RunStatus::Succeeded,
        error: None,
        gpu_monitoring: None,
        inputs: Vec::new(),
        memory: None,
    };
    let path = summary.write_json_atomic(&directory).expect("write JSON report");
    let bytes = read(&path).expect("read report");
    assert_eq!(serde_json::from_slice::<RunSummary>(&bytes).unwrap(), summary);
    let serialized = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
    assert!(serialized["memory"].is_null());
    assert!(serialized.get("started_at").is_some());
    assert!(serialized.get("started_at_ms").is_none());
    assert!(summary.write_json_atomic(&directory).is_err());
    remove_file(path).expect("remove report");
    remove_dir(directory).expect("remove reporter test directory");
}
#[test]
fn test_memory_evidence_recovers_optional_peak_observations() {
    let path = path("memory-observed");
    let metadata = metadata("run-memory-observed").with_memory(memory_evidence());
    let journal =
        RunJournal::create(path.clone(), metadata, vec![JournalInput::new(0, Some("input".to_string()))], Vec::new()).expect("create memory journal");
    assert!(journal.record_memory_peak(0, memory(175)).is_err());
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    journal.transition_input(0, ExecutionState::Succeeded, None).expect("finish input");
    assert!(journal.record_memory_peak(0, memory(0)).is_err());
    journal.record_memory_peak(0, memory(175)).expect("record underpredicted peak");
    assert!(journal.record_memory_peak(0, memory(176)).is_err());
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover memory journal");
    let serialized = serde_json::to_value(&summary).expect("serialize memory summary");
    assert!(serialized["memory"]["budget"].is_string());
    assert!(serialized["memory"]["inputs"][0]["estimated"].is_string());
    assert!(serialized["memory"]["inputs"][0].get("estimated_bytes").is_none());
    let mut output = Vec::new();
    summary.render_console_to(&mut output, &path, false).expect("render memory evidence");
    let output = String::from_utf8(output).expect("memory output should be UTF-8");
    assert!(output.contains("Memory: memory_aware via journal-test-v1, budget 200 bytes"));
    assert!(output.contains("estimate 120 + safety 30 = reservation 150 bytes, observed 175 bytes"));
    let memory = summary.memory.expect("memory evidence");
    assert_eq!(memory.budget.checked_bytes(), Some(200));
    assert_eq!(memory.inputs[0].estimated.as_ref().and_then(Memory::checked_bytes), Some(120));
    assert_eq!(memory.inputs[0].safety_reserve.as_ref().and_then(Memory::checked_bytes), Some(30));
    assert_eq!(memory.inputs[0].reservation.as_ref().and_then(Memory::checked_bytes), Some(150));
    assert_eq!(memory.inputs[0].observed_peak.as_ref().and_then(Memory::checked_bytes), Some(175));
    remove_file(path).expect("remove memory journal");
}
#[test]
fn test_memory_evidence_validates_mode_and_unconfigured_observations() {
    let invalid_fallback = MemoryFallbackEvidence::InputOutOfRange {
        maximum_input: memory(1),
        minimum_input: memory(2),
    };
    assert!(invalid_fallback.validate().is_err());
    let mut invalid_input = memory_evidence().inputs.remove(0);
    invalid_input.safety_reserve = Some(memory(29));
    assert!(invalid_input.validate().is_err());
    let invalid_path = path("memory-invalid");
    let mut invalid = memory_evidence();
    invalid.mode = MemoryEvidenceMode::FixedFallback;
    assert!(RunJournal::create(
        invalid_path.clone(),
        metadata("run-memory-invalid").with_memory(invalid),
        vec![JournalInput::new(0, None)],
        Vec::new()
    )
    .is_err());
    assert!(!invalid_path.exists());
    let path = path("memory-unconfigured");
    let journal = RunJournal::create(
        path.clone(),
        metadata("run-memory-unconfigured"),
        vec![JournalInput::new(0, None)],
        Vec::new(),
    )
    .expect("create unconfigured journal");
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    assert!(journal.record_memory_peak(0, memory(1)).is_err());
    remove_file(path).expect("remove unconfigured journal");
}
#[test]
fn test_metadata_and_journal_inputs_are_validated_before_use() {
    let path = path("validation");
    assert!(metadata("").validate().is_err());
    assert!(RunJournal::create(path.clone(), metadata(""), Vec::new(), Vec::new()).is_err());
    assert!(!path.exists());

    let first = JournalInput::new(0, None);
    let mut duplicate = JournalInput::new(1, None);
    duplicate.input_id.clone_from(&first.input_id);
    assert!(RunJournal::create(path.clone(), metadata("run-validation"), vec![first, duplicate], Vec::new()).is_err());
    assert!(!path.exists());
}
#[test]
fn test_missing_finalization_is_interrupted_and_truncated_tail_is_ignored() {
    let path = path("truncated");
    let journal =
        RunJournal::create(path.clone(), metadata("run-truncated"), vec![JournalInput::new(0, None)], vec![module()]).expect("create journal");
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    drop(journal);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open journal")
        .write_all(br#"{"schema_version":1,"sequence"#)
        .expect("append truncated record");

    let summary = RunJournal::read(&path).expect("recover interrupted journal");
    assert_eq!(summary.status, RunStatus::Interrupted);
    assert_eq!(summary.inputs[0].state, ExecutionState::Running);
    assert_eq!(summary.inputs[0].modules[0].state, ExecutionState::NotStarted);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_round_trips_all_states_and_recovers_a_summary() {
    let path = path("round-trip");
    let inputs = vec![JournalInput::new(0, Some("one".to_string())), JournalInput::new(1, None)];
    let journal = RunJournal::create(path.clone(), metadata("run-one"), inputs, vec![module()]).expect("create journal");
    journal.transition_input(0, ExecutionState::Pending, None).expect("pending input");
    journal.transition_input(0, ExecutionState::Running, None).expect("run input");
    journal.transition_module(0, 0, ExecutionState::Running, None).expect("run module");
    journal.transition_module(0, 0, ExecutionState::Succeeded, None).expect("finish module");
    journal.transition_input(0, ExecutionState::Succeeded, None).expect("finish input");
    journal.transition_input(1, ExecutionState::Skipped, None).expect("skip input");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");

    let summary = RunSummary::try_from(path.as_path()).expect("recover journal");
    assert_eq!(summary.status, RunStatus::Succeeded);
    assert_eq!(summary.inputs[0].state, ExecutionState::Succeeded);
    assert_eq!(summary.inputs[0].modules[0].state, ExecutionState::Succeeded);
    assert_eq!(summary.inputs[0].modules[0].module_path, vec!["group", "module"]);
    assert_eq!(summary.inputs[1].state, ExecutionState::Skipped);
    remove_file(path).expect("remove journal");
}
#[test]
fn test_module_acquisition_is_exactly_once_before_execution_and_recovered() {
    let path = path("module-acquisition");
    let journal = RunJournal::create(
        path.clone(),
        metadata("run-module-acquisition"),
        vec![JournalInput::new(0, None)],
        vec![module()],
    )
    .expect("create journal");
    let evidence = acquisition_evidence();
    journal.record_module_acquisition(0, 0, evidence.clone()).expect("record acquisition");
    assert!(journal.record_module_acquisition(0, 0, evidence.clone()).is_err());
    journal.transition_input(0, ExecutionState::Running, None).expect("run input");
    journal.transition_module(0, 0, ExecutionState::Running, None).expect("run module");
    journal.transition_module(0, 0, ExecutionState::Succeeded, None).expect("finish module");
    journal.transition_input(0, ExecutionState::Succeeded, None).expect("finish input");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    let summary = RunJournal::read(&path).expect("recover acquisition evidence");
    assert_eq!(summary.inputs[0].modules[0].acquisition.as_ref(), Some(&evidence));
    remove_file(path).expect("remove journal");
}
#[test]
fn test_run_status_display_matches_serialized_vocabulary() {
    assert_eq!(RunStatus::Succeeded.to_string(), "succeeded");
    assert_eq!(RunStatus::Failed.to_string(), "failed");
    assert_eq!(RunStatus::Interrupted.to_string(), "interrupted");
}
#[test]
fn test_sandbox_lifecycle_finalization_is_exactly_once_and_recovered() {
    let path = path("sandbox-lifecycle");
    let journal =
        RunJournal::create(path.clone(), metadata("run-sandbox").sandbox(), Vec::new(), Vec::new()).expect("create sandbox lifecycle journal");
    assert!(journal.finish(FinalStatus::Failed, Some("failed")).is_err());
    let resources = vec![SandboxResourceEvidence {
        error: None,
        identifier: "xylem-run-sandbox-0-exec".to_string(),
        input_index: 0,
        phase: crate::sandbox::SandboxResourcePhase::Execution,
        status: SandboxResourceDisposition::Retained,
    }];
    let reconciled = vec![SandboxResourceEvidence {
        error: None,
        identifier: "xylem-old-0-prep".to_string(),
        input_index: 0,
        phase: crate::sandbox::SandboxResourcePhase::Preparation,
        status: SandboxResourceDisposition::Reconciled,
    }];
    journal
        .finalize_sandbox(resources.clone(), reconciled.clone(), None)
        .expect("finalize sandbox lifecycle");
    assert!(journal.finalize_sandbox(resources.clone(), reconciled.clone(), None).is_err());
    journal.finish(FinalStatus::Failed, Some("failed")).expect("finish sandbox run");
    let summary = RunJournal::read(&path).expect("recover sandbox lifecycle summary");
    let lifecycle = summary.sandbox.lifecycle.expect("sandbox lifecycle evidence");
    assert!(lifecycle.finalized);
    assert_eq!(lifecycle.resources, resources);
    assert_eq!(lifecycle.reconciled, reconciled);
    assert_eq!(lifecycle.retention, crate::sandbox::SandboxRetention::RetainOnFailure);
    remove_file(path).expect("remove journal");
}
