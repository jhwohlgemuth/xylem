use crate::config::*;
use crate::execution::{ExecutionContext, ExecutionLimits};
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use std::sync::Mutex;

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
fn test_get_children() {
    assert_eq!(get_children("../../does/not/exist".to_string()), Vec::<String>::new());
    match PathBuf::from("/root/dev/command").try_exists() {
        | Ok(true) => {
            let expected = vec![
                "/root/dev/command/xylem-lib/src".to_string(),
                "/root/dev/command/xylem-lib/Cargo.toml".to_string(),
            ];
            assert_eq!(get_children("../xylem-lib".to_string()), expected);
        }
        | Ok(false) => (),
        | Err(_) => (),
    };
}

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
    let prepared = config.with_options(options.clone()).prepare_modules(&execution, Some(&progress));
    prepared.run_workflow_with_progress(Some("input-a".to_string()), &options, Some(&progress));
    let updates = progress.updates.lock().expect("progress lock").clone();
    assert_eq!(updates.len(), 4);
    assert_eq!(updates[0].phase, ProgressPhase::Preparation);
    assert_eq!(updates[0].state, ProgressState::Started);
    assert_eq!(updates[1].state, ProgressState::Finished);
    assert_eq!(updates[2].phase, ProgressPhase::Execution);
    assert_eq!(updates[2].input.as_deref(), Some("input-a"));
    assert_eq!(updates[3].state, ProgressState::Finished);
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
