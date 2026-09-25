use crate::config::*;
use crate::execution::{ExecutionContext, ExecutionLimits, InputBatch};
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use std::fs::{remove_file, write};
use std::sync::Mutex;

#[derive(Default)]
struct RecordingProgress {
    updates: Mutex<Vec<ProgressUpdate>>,
}

impl ProgressObserver for RecordingProgress {
    fn update(&self, update: ProgressUpdate) {
        self.updates.lock().expect("progress lock").push(update);
    }
}

#[test]
fn allowed_round_trips_in_configuration_options() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": { "allowed": ["example.com", "artifacts.example:8443"] }
    }))
    .expect("valid configuration");
    let hosts = config
        .options
        .as_ref()
        .and_then(|options| options.allowed.as_ref())
        .expect("allowed hosts");
    assert_eq!(hosts, &["example.com", "artifacts.example:8443"]);
    let serialized = serde_json::to_value(config).expect("serialize configuration");
    assert_eq!(
        serialized["options"]["allowed"],
        serde_json::json!(["example.com", "artifacts.example:8443"])
    );
}
#[test]
fn batch_rejects_zero_negative_non_numeric_and_overflow_values() {
    let overflow = (usize::MAX as u128 + 1).to_string();
    for value in ["0".to_string(), "-1".to_string(), "\"many\"".to_string(), overflow] {
        let json = format!(r#"{{"requirements":[],"modules":[],"options":{{"batch":{value}}}}}"#);
        let error = serde_json::from_str::<Config>(&json)
            .expect_err("invalid batch should be rejected")
            .to_string();
        assert!(!error.is_empty());
        if value == "0" {
            assert!(error.contains("input batch must be greater than zero"), "{error}");
        }
    }
    let yaml = "requirements: []\nmodules: []\noptions:\n  batch: 0\n";
    assert!(serde_yml::from_str::<Config>(yaml).is_err());
}
#[test]
fn batch_round_trips_through_json_and_yaml() {
    let json = serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": { "batch": 2 }
    });
    let config: Config = serde_json::from_value(json).expect("valid JSON batch");
    assert_eq!(config.options.as_ref().and_then(|options| options.batch).map(InputBatch::get), Some(2));
    let serialized = serde_json::to_value(&config).expect("serialize JSON configuration");
    assert_eq!(serialized["options"]["batch"], 2);
    let yaml = serde_yml::to_string(&config).expect("serialize YAML configuration");
    let round_trip: Config = serde_yml::from_str(&yaml).expect("deserialize YAML batch");
    assert_eq!(round_trip.options.and_then(|options| options.batch).map(InputBatch::get), Some(2));
}
#[test]
fn configuration_try_from_uses_mime_type_for_yml_files() {
    let path = std::env::temp_dir().join(format!("xylem-config-mime-{}.yml", std::process::id()));
    write(&path, "requirements: []\nmodules: []\n").expect("write YAML configuration");
    let config = Config::try_from(path.as_path()).expect("read YAML configuration by MIME type");
    assert!(config.modules.is_empty());
    remove_file(path).expect("remove YAML configuration");
}
#[test]
fn configuration_version_must_be_semantic() {
    for version in ["1", "1.2", "1.2.3"] {
        let config = serde_json::json!({
            "version": version,
            "requirements": [],
            "modules": []
        });
        assert!(serde_json::from_value::<Config>(config).is_ok(), "{version} should be valid");
    }

    for version in ["", "01.2.3", "1.02.3", "1.2.03", "v1.2.3", "1.2.3-alpha", "1.2.3.4"] {
        let config = serde_json::json!({
            "version": version,
            "requirements": [],
            "modules": []
        });
        assert!(serde_json::from_value::<Config>(config).is_err(), "{version} should be invalid");
    }
}
#[test]
fn preparation_and_execution_emit_progress_updates() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "probe",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": "file:../tests/e2e/modules/environment-probe",
            "template": { "command": "sh", "arguments": ["run.sh"] }
        }],
        "options": { "dry_run": true }
    }))
    .expect("valid configuration");
    let options = ConfigOptions::init().root(std::env::temp_dir()).dry_run(true).build();
    let execution = ExecutionContext::new(ExecutionLimits::try_from(1).expect("valid limit")).expect("execution context");
    let progress = RecordingProgress::default();
    let prepared = config
        .with_options(options.clone())
        .prepare_modules(&execution, Some(&progress))
        .expect("prepare modules");
    prepared.run_workflow_with_progress(Some("input-a".to_string()), &options, Some(&progress));
    let updates = progress.updates.lock().expect("progress lock").clone();
    assert_eq!(updates.len(), 4);
    assert_eq!(updates[0].phase, ProgressPhase::Preparation);
    assert_eq!(updates[0].state, ProgressState::Started);
    assert_eq!(updates[1].state, ProgressState::Finished);
    assert_eq!(updates[2].phase, ProgressPhase::Execution);
    assert_eq!(updates[2].input.as_deref(), Some("input-a"));
    assert_eq!(updates[3].state, ProgressState::Skipped);
}
#[test]
fn test_get_children() {
    assert_eq!(get_children("../../does/not/exist".to_string()), Vec::<String>::new());
    let children = get_children("../tests/fixtures/input".to_string())
        .iter()
        .map(|path| PathBuf::from(path).file_name().expect("input name").to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(children, ["one", "three", "two"]);
}
