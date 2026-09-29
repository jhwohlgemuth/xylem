use super::*;
use acorn_schema::validation::Validate;
use alloc::sync::Arc;
use std::fs::{create_dir_all, read, read_to_string, remove_dir, remove_file};

fn metadata(run_id: &str) -> RunMetadata {
    RunMetadata {
        run_id: run_id.to_string(),
        configuration: ConfigurationIdentity {
            path: "config.json".to_string(),
            id: Some("workflow".to_string()),
            version: Some("1.0.0".to_string()),
        },
        dry_run: false,
        started_at_ms: 1,
    }
}
fn module() -> JournalModule {
    JournalModule::new(0, "module".to_string(), vec!["group".to_string(), "module".to_string()])
}
fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-journal-{label}-{}-{}.jsonl", std::process::id(), timestamp_ms()))
}

#[test]
fn concurrent_writes_remain_valid_and_sequenced() {
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
fn container_states_are_recovered() {
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
fn error_details_are_bounded_without_splitting_utf8() {
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
fn existing_destination_and_invalid_transition_are_rejected() {
    let path = path("exclusive");
    let journal = RunJournal::create(path.clone(), metadata("run-exclusive"), vec![JournalInput::new(0, None)], Vec::new()).expect("create journal");
    assert!(RunJournal::create(path.clone(), metadata("other"), Vec::new(), Vec::new()).is_err());
    assert!(journal.transition_input(0, ExecutionState::Succeeded, None).is_err());
    remove_file(path).expect("remove journal");
}
#[test]
fn journal_jsonl_omits_none_fields() {
    let path = path("omit-none");
    let journal = RunJournal::create(path.clone(), metadata("run-omit-none"), vec![JournalInput::new(0, None)], Vec::new()).expect("create journal");
    journal.transition_input(0, ExecutionState::Running, None).expect("start input");
    journal.transition_input(0, ExecutionState::Succeeded, None).expect("finish input");
    journal.finish(FinalStatus::Succeeded, None).expect("finish run");
    assert!(!read_to_string(&path).expect("read journal").contains("null"));
    remove_file(path).expect("remove journal");
}
#[test]
fn json_report_is_atomic_and_refuses_overwrite() {
    let directory = std::env::temp_dir().join(format!("xylem-journal-report-{}-{}.jsonl", std::process::id(), timestamp_ms()));
    create_dir_all(&directory).expect("create reporter test directory");
    let summary = RunSummary {
        schema_version: JOURNAL_SCHEMA_VERSION,
        containers: Vec::new(),
        run_id: "atomic-report".to_string(),
        configuration: ConfigurationIdentity {
            path: "config.json".to_string(),
            id: None,
            version: None,
        },
        dry_run: false,
        started_at_ms: 1,
        finished_at_ms: Some(2),
        duration_ms: Some(1),
        status: RunStatus::Succeeded,
        error: None,
        inputs: Vec::new(),
    };
    let path = summary.write_json_atomic(&directory).expect("write JSON report");
    assert_eq!(serde_json::from_slice::<RunSummary>(&read(&path).expect("read report")).unwrap(), summary);
    assert!(summary.write_json_atomic(&directory).is_err());
    remove_file(path).expect("remove report");
    remove_dir(directory).expect("remove reporter test directory");
}
#[test]
fn metadata_and_journal_inputs_are_validated_before_use() {
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
fn missing_finalization_is_interrupted_and_truncated_tail_is_ignored() {
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
fn round_trips_all_states_and_recovers_a_summary() {
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
fn run_status_display_matches_serialized_vocabulary() {
    assert_eq!(RunStatus::Succeeded.to_string(), "succeeded");
    assert_eq!(RunStatus::Failed.to_string(), "failed");
    assert_eq!(RunStatus::Interrupted.to_string(), "interrupted");
}
