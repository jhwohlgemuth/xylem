use crate::config::*;
use crate::execution::{ExecutionContext, ExecutionLimits, InputBatch};
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use acorn_schema::validation::Validate;
use std::fs::{create_dir_all, remove_dir_all, remove_file, write};
use std::sync::Mutex;

const FIXTURES: &str = "../tests/fixtures";
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
fn test_allowed_round_trips_in_configuration_options() {
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
fn test_batch_rejects_zero_negative_non_numeric_and_overflow_values() {
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
    assert!(parse_yaml_configuration(yaml).is_err());
}
#[test]
fn test_batch_round_trips_through_json_and_yaml() {
    let json = serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": { "batch": 2 }
    });
    let config: Config = serde_json::from_value(json).expect("valid JSON batch");
    assert_eq!(config.options.as_ref().and_then(|options| options.batch).map(InputBatch::get), Some(2));
    let serialized = serde_json::to_value(&config).expect("serialize JSON configuration");
    assert_eq!(serialized["options"]["batch"], 2);
    let yaml = serde_norway::to_string(&config).expect("serialize YAML configuration");
    let round_trip = parse_yaml_configuration(&yaml).expect("deserialize YAML batch");
    assert_eq!(round_trip.options.and_then(|options| options.batch).map(InputBatch::get), Some(2));
}
#[test]
fn test_completion_declarations_round_trip_through_json_and_yaml() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "output": "/tmp/artifacts",
        "requirements": [],
        "modules": [{
            "completion": {
                "marker": "{{ OUTPUT_DIRECTORY }}/.xylem/probe.json",
                "outputs": ["{{ OUTPUT_DIRECTORY }}/*.txt"]
            },
            "name": "probe",
            "programmingLanguage": "generic",
            "template": { "arguments": [], "command": "true" },
            "type": "script",
            "uri": "file:./probe"
        }]
    }))
    .expect("completion configuration");
    let json = serde_json::to_value(&config).expect("serialize JSON");
    let yaml = serde_norway::to_string(&config).expect("serialize YAML");
    let yaml: Config = parse_yaml_configuration(&yaml).expect("deserialize YAML");
    assert_eq!(json, serde_json::to_value(yaml).expect("normalize YAML"));
}
#[test]
fn test_completion_planning_reports_invalid_glob_unresolved_variable_and_outside_root() {
    for (output, expected) in [
        ("{{ OUTPUT_DIRECTORY }}/[", "invalid glob"),
        ("{{ OUTPUT_DIRECTORY }}/{{ MISSING }}/*.txt", "unresolved variable"),
        ("/tmp/outside/*.txt", "outside every declared output root"),
    ] {
        let config: Config = serde_json::from_value(serde_json::json!({
            "output": "/tmp/xylem-validation-root",
            "requirements": [],
            "modules": [{
                "completion": {
                    "marker": "{{ OUTPUT_DIRECTORY }}/.xylem/probe.json",
                    "outputs": [output]
                },
                "name": "probe",
                "programmingLanguage": "generic",
                "template": { "arguments": [], "command": "true" },
                "type": "script",
                "uri": "file:./probe"
            }]
        }))
        .expect("completion configuration");
        let error = config
            .validate_for_inputs(&[Some("input".to_string())])
            .expect_err("completion declaration should fail planning");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.to_string().contains("inputs[0].modules[0].completion"), "{error}");
    }
}
#[test]
fn test_configuration_errors_preserve_format_path_and_source() {
    let missing = std::env::temp_dir().join(format!("xylem-missing-config-{}.json", std::process::id()));
    let missing_error = Config::read_json(missing.clone()).expect_err("missing configuration should fail");
    match &missing_error {
        | ConfigReadError::Read { format, path, .. } => {
            assert_eq!(*format, ConfigFormat::Json);
            assert_eq!(path, &missing);
        }
        | error => panic!("expected read error, found {error}"),
    }
    assert!(core::error::Error::source(&missing_error).is_some());
    assert!(missing_error.to_string().contains("read application/json configuration"));

    let empty = std::env::temp_dir().join(format!("xylem-empty-config-{}.yaml", std::process::id()));
    write(&empty, " \n").expect("write empty YAML configuration");
    let empty_error = Config::read_yaml(empty.clone()).expect_err("empty configuration should fail");
    match &empty_error {
        | ConfigReadError::Invalid { format, path, .. } => {
            assert_eq!(*format, ConfigFormat::Yaml);
            assert_eq!(path, &empty);
        }
        | error => panic!("expected invalid error, found {error}"),
    }
    assert!(core::error::Error::source(&empty_error).is_some());
    assert!(empty_error.to_string().contains("parse application/yaml configuration"));
    remove_file(empty).expect("remove empty YAML configuration");
}
#[test]
fn test_configuration_try_from_uses_mime_type_for_yml_files() {
    let path = std::env::temp_dir().join(format!("xylem-config-mime-{}.yml", std::process::id()));
    write(&path, "requirements: []\nmodules: []\n").expect("write YAML configuration");
    let config = Config::try_from(path.as_path()).expect("read YAML configuration by MIME type");
    assert!(config.modules.is_empty());
    remove_file(path).expect("remove YAML configuration");
}
#[test]
fn test_configuration_modules_flatten_local_workflows() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-modules-{}", std::process::id()));
    let child_directory = root.join("child");
    create_dir_all(&child_directory).expect("create child configuration directory");
    let parent = root.join("config.json");
    let child = child_directory.join("config.json");
    write(
        &parent,
        r#"{
            "requirements": ["sh"],
            "variables": { "PARENT": "parent" },
            "modules": [{
                "name": "Group",
                "type": "configuration",
                "uri": "file:./child/config.json",
                "variables": { "WRAPPER": "wrapper" }
            }]
        }"#,
    )
    .expect("write parent configuration");
    write(
        &child,
        r#"{
            "requirements": ["python", "sh"],
            "variables": { "CHILD": "child" },
            "modules": [{
                "name": "Leaf",
                "type": "script",
                "programmingLanguage": "generic",
                "uri": "file:./module",
                "variables": { "LEAF": "leaf" },
                "template": { "command": "true", "arguments": [] }
            }]
        }"#,
    )
    .expect("write child configuration");
    let config = Config::read(parent.clone())
        .expect("read parent configuration")
        .resolve(&parent)
        .expect("resolve nested configuration");
    assert_eq!(config.requirements, vec!["sh", "python"]);
    assert_eq!(config.modules.len(), 1);
    assert_eq!(config.modules[0].name, "Leaf");
    assert_eq!(config.modules[0].module_path, vec!["Group", "Leaf"]);
    match &config.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert!(<&str>::from(uri).ends_with("/child/module"), "{uri}"),
        | _ => panic!("nested leaf should remain a script module"),
    }
    let variables = config.modules[0].variables.as_ref().expect("merged variables");
    assert_eq!(variables["PARENT"], "parent");
    assert_eq!(variables["WRAPPER"], "wrapper");
    assert_eq!(variables["CHILD"], "child");
    assert_eq!(variables["LEAF"], "leaf");
    remove_dir_all(root).expect("remove configuration test directory");
}
#[test]
fn test_configuration_modules_reject_cycles() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-cycle-{}", std::process::id()));
    create_dir_all(&root).expect("create configuration cycle directory");
    let first = root.join("first.json");
    let second = root.join("second.json");
    write(
        &first,
        r#"{"requirements":[],"modules":[{"name":"Second","type":"configuration","uri":"file:./second.json"}]}"#,
    )
    .expect("write first configuration");
    write(
        &second,
        r#"{"requirements":[],"modules":[{"name":"First","type":"configuration","uri":"file:./first.json"}]}"#,
    )
    .expect("write second configuration");
    let error = Config::read(first.clone())
        .expect("read first configuration")
        .resolve(&first)
        .expect_err("cycle should fail");
    assert!(matches!(error, ConfigurationModuleError::Cycle { .. }));
    assert!(error.to_string().contains("Second::First"));
    remove_dir_all(root).expect("remove configuration cycle directory");
}
#[test]
fn test_configuration_modules_reject_excessive_depth() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-depth-{}", std::process::id()));
    create_dir_all(&root).expect("create configuration depth directory");
    (0..18usize).for_each(|index| {
        let path = root.join(format!("{index}.json"));
        let content = match index {
            | 17 => r#"{"requirements":[],"modules":[{"name":"Leaf","type":"script","uri":"file:./module","template":{"command":"true","arguments":[]}}]}"#
                .to_string(),
            | _ => format!(
                r#"{{"requirements":[],"modules":[{{"name":"N{index}","type":"configuration","uri":"file:./{}.json"}}]}}"#,
                index + 1
            ),
        };
        write(path, content).expect("write nested configuration");
    });
    let first = root.join("0.json");
    let error = Config::read(first.clone())
        .expect("read first configuration")
        .resolve(&first)
        .expect_err("depth should fail");
    assert!(matches!(error, ConfigurationModuleError::Depth { .. }));
    remove_dir_all(root).expect("remove configuration depth directory");
}
#[test]
fn test_configuration_version_must_be_semantic() {
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
fn test_every_example_configuration_passes_semantic_validation() {
    let examples = std::fs::read_dir("../examples").expect("read examples directory");
    let mut configurations = examples
        .map(|entry| entry.expect("example entry").path().join("config.json"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    configurations.extend(
        std::fs::read_dir("../tests")
            .expect("read tests directory")
            .map(|entry| entry.expect("test entry").path().join("config.json"))
            .filter(|path| path.is_file()),
    );
    configurations.sort();
    assert!(!configurations.is_empty());
    configurations.into_iter().for_each(|path| {
        let config = Config::try_from(path.as_path()).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        config.validate().unwrap_or_else(|error| panic!("validate {}: {error}", path.display()));
    });
}
#[test]
fn test_json_and_yaml_fixtures_deserialize_equally() {
    let json = Config::read_json(PathBuf::from(format!("{FIXTURES}/config-equivalent.json"))).expect("read equivalent JSON fixture");
    let yaml = Config::read_yaml(PathBuf::from(format!("{FIXTURES}/config-equivalent.yaml"))).expect("read equivalent YAML fixture");
    let json_value = serde_json::to_value(json).expect("normalize JSON fixture");
    let yaml_value = serde_json::to_value(yaml).expect("normalize YAML fixture");
    assert_eq!(json_value, yaml_value);
    assert_eq!(yaml_value["variables"]["ALIASED"], "shared");
    assert_eq!(yaml_value["variables"]["BOOLEAN"], true);
    assert!(yaml_value["variables"]["NULL"].is_null());
    assert_eq!(yaml_value["variables"]["NUMBER"], 42);
    assert_eq!(yaml_value["variables"]["QUOTED"], "true");
}
#[test]
fn test_json_and_yaml_reject_unknown_fields() {
    for (json, yaml) in [
        (
            r#"{"requirements":[],"modules":[],"unexpected":true}"#,
            "requirements: []\nmodules: []\nunexpected: true\n",
        ),
        (
            r#"{"requirements":[],"modules":[],"options":{"unexpected":true}}"#,
            "requirements: []\nmodules: []\noptions:\n  unexpected: true\n",
        ),
        (
            r#"{"requirements":[],"modules":[{"name":"probe","type":"script","uri":"file:./probe","unexpected":true}]}"#,
            "requirements: []\nmodules:\n  - name: probe\n    type: script\n    uri: file:./probe\n    unexpected: true\n",
        ),
        (
            r#"{"requirements":[],"modules":[{"name":"probe","type":"script","uri":"file:./probe","template":{"command":"sh","arguments":[],"unexpected":true}}]}"#,
            "requirements: []\nmodules:\n  - name: probe\n    type: script\n    uri: file:./probe\n    template:\n      command: sh\n      arguments: []\n      unexpected: true\n",
        ),
    ] {
        assert!(parse_json_configuration(json).is_err(), "JSON should reject {json}");
        assert!(parse_yaml_configuration(yaml).is_err(), "YAML should reject {yaml}");
    }
}
#[test]
fn test_preparation_and_execution_emit_progress_updates() {
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
fn test_semantic_validation_collects_path_qualified_errors() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [""],
        "resources": ["gpu"],
        "modules": [
            {
                "name": "duplicate",
                "programmingLanguage": "generic",
                "type": "script",
                "uri": "invalid",
                "template": { "arguments": [], "command": "" }
            },
            {
                "completion": { "marker": "*.json", "outputs": [] },
                "name": "duplicate",
                "programmingLanguage": "generic",
                "type": "script",
                "uri": "file:./probe"
            }
        ]
    }))
    .expect("configuration should deserialize before semantic validation");
    let error = config.validate().expect_err("configuration should be invalid");
    let paths = error.iter().map(|issue| issue.path.as_str()).collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "resources",
            "requirements[0]",
            "modules[0].uri",
            "modules[0].template.command",
            "modules[1].name",
            "modules[1].completion.marker",
            "modules[1].completion.outputs",
            "modules[1].completion",
        ]
    );
    assert!(error
        .to_string()
        .contains("modules[0].template.command: template command must not be empty"));
}
#[test]
fn test_type_specific_module_fields_are_rejected_in_json_and_yaml() {
    for (json, yaml) in [
        (
            r#"{"requirements":[],"modules":[{"name":"probe","type":"script","uri":"file:./probe","checksum":"sha256:00"}]}"#,
            "requirements: []\nmodules:\n  - name: probe\n    type: script\n    uri: file:./probe\n    checksum: sha256:00\n",
        ),
        (
            r#"{"requirements":[],"modules":[{"name":"probe","type":"binary","uri":"file:./probe","programmingLanguage":"generic"}]}"#,
            "requirements: []\nmodules:\n  - name: probe\n    type: binary\n    uri: file:./probe\n    programmingLanguage: generic\n",
        ),
    ] {
        assert!(parse_json_configuration(json).is_err());
        assert!(parse_yaml_configuration(yaml).is_err());
    }
}
#[test]
fn test_yaml_rejects_ambiguous_and_resource_exhausting_input() {
    for yaml in [
        "requirements: []\nrequirements: []\nmodules: []\n",
        "requirements: []\nmodules: []\n---\nrequirements: []\nmodules: []\n",
        "requirements: []\nmodules:\n  - <<: invalid\n    name: probe\n    type: script\n    uri: file:./probe\n",
    ] {
        assert!(parse_yaml_configuration(yaml).is_err(), "YAML should reject {yaml}");
    }
    let nested = format!(
        "requirements: []\nmodules: []\nvariables:\n  DEEP: {}value{}\n",
        "[".repeat(129),
        "]".repeat(129)
    );
    assert!(parse_yaml_configuration(&nested).is_err());
    let repeated = "requirements: []\nmodules: []\nvariables:\n  A: &A [x, x, x, x, x, x, x, x, x, x]\n  B: &B [*A, *A, *A, *A, *A, *A, *A, *A, *A, *A]\n  C: &C [*B, *B, *B, *B, *B, *B, *B, *B, *B, *B]\n  D: &D [*C, *C, *C, *C, *C, *C, *C, *C, *C, *C]\n  E: [*D, *D, *D, *D, *D, *D, *D, *D, *D, *D]\n";
    let error = parse_yaml_configuration(repeated).expect_err("excessive alias expansion should fail");
    assert!(error.to_string().contains("repetition limit exceeded"), "{error}");
}
#[test]
fn test_yaml_supports_merge_sequence_precedence() {
    let yaml = "requirements: []\nmodules:\n  - <<:\n      - &first {name: First, type: script, uri: 'file:./first'}\n      - &second {name: Second, type: script, uri: 'file:./second'}\n";
    let config = parse_yaml_configuration(yaml).expect("merge sequence should deserialize");
    assert_eq!(config.modules[0].name, "First");
    match &config.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:./first"),
        | module_type => panic!("expected script module, found {module_type:?}"),
    }
}
