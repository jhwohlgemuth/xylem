#![expect(clippy::panic, reason = "tests intentionally panic on unexpected outcomes")]
#![expect(clippy::indexing_slicing, reason = "test indexing deliberately fails on malformed fixtures")]
use crate::acquisition::{AcquisitionCache, AcquisitionCacheStatus, AcquisitionLimits, AcquisitionPolicy};
use crate::config::*;
use crate::execution::{ExecutionContext, ExecutionLimits, InputBatch};
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use crate::workspace::RunWorkspace;
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use sha2::{Digest, Sha256};
use std::fs::{create_dir_all, read_dir, remove_dir_all, remove_file, write};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;

const FIXTURES: &str = "../../tests/fixtures";

fn acquisition_policy(hosts: Option<&[String]>, allow_insecure_http: bool, offline: bool) -> AcquisitionPolicy {
    AcquisitionPolicy::from_hosts(hosts, allow_insecure_http, None, offline, AcquisitionLimits::default()).expect("valid acquisition policy")
}
fn checksum(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
}
struct CancelledExecutor;
#[derive(Default)]
struct RecordingProgress {
    updates: Mutex<Vec<ProgressUpdate>>,
}
impl ModuleExecutor for CancelledExecutor {
    fn execute(&self, script: &Script, _input_index: usize, _module_index: usize) -> Result<(), ProcessError> {
        Err(ProcessError::Spawn {
            script: script.to_string(),
            working_directory: PathBuf::from(&script.current_dir),
            source: std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled by test"),
        })
    }
    fn is_cancellation_requested(&self) -> bool {
        true
    }
}
impl ProgressObserver for RecordingProgress {
    fn update(&self, update: ProgressUpdate) {
        self.updates.lock().expect("progress lock").push(update);
    }
}
#[allow(clippy::std_instead_of_core)]
fn bind_local_server() -> Option<TcpListener> {
    match TcpListener::bind("127.0.0.1:0") {
        | Ok(listener) => Some(listener),
        | Err(why) if why.kind() == std::io::ErrorKind::PermissionDenied => None,
        | Err(why) => panic!("bind local server: {why}"),
    }
}
fn configuration_files(root: &Path) -> Vec<PathBuf> {
    read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .map(|entry| entry.expect("configuration entry").path())
        .flat_map(|path| match path.is_dir() {
            | true => configuration_files(&path),
            | false => vec![path],
        })
        .filter(|path| {
            matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".xylem.json" | ".xylem.jsonc" | "config.json" | "config.yaml" | "config.yml")
            )
        })
        .collect()
}
fn memory_configuration(estimator: Value, modules: Vec<Value>, budget: &str) -> Config {
    serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": modules,
        "options": { "memory": { "budget": budget, "estimator": estimator } }
    }))
    .expect("memory configuration")
}
fn memory_estimator(identity: &str) -> Value {
    serde_json::json!({
        "identity": identity,
        "intercept": "16GB",
        "maximum_input": "1TB",
        "minimum_input": "1KB",
        "safety_millionths": 1_100_000,
        "slope_millionths": 0
    })
}
fn memory_module(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "programmingLanguage": "generic",
        "template": { "arguments": [], "command": "true" },
        "type": "script",
        "uri": "file:."
    })
}

#[test]
fn test_all_module_locations_round_trip_scalar_and_structured_forms() {
    let document = serde_json::json!({
        "requirements": [],
        "modules": [
            { "name": "binary", "type": "binary", "uri": "https://example.com/tool" },
            {
                "name": "configuration",
                "type": "configuration",
                "uri": { "scheme": "https", "uri": "https://example.com/workflow.git", "revision": "v1.2.3" }
            },
            { "name": "script", "type": "script", "uri": "file:./script" }
        ]
    });
    let config: Config = serde_json::from_value(document).expect("valid scalar and structured locations");
    let serialized = serde_json::to_value(config).expect("serialize module locations");
    assert_eq!(serialized["modules"][0]["uri"], "https://example.com/tool");
    assert_eq!(serialized["modules"][1]["uri"]["revision"], "v1.2.3");
    assert_eq!(serialized["modules"][2]["uri"], "file:./script");
    serde_json::from_value::<Config>(serialized).expect("round trip module locations");
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
fn test_acquisition_limits_must_be_positive() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [],
        "options": { "acquisition_limits": { "git_entries": 0 } },
        "requirements": []
    }))
    .expect("configuration should deserialize before semantic validation");
    let report = config.validate().expect_err("zero acquisition limit should fail");
    assert!(report.iter().any(|issue| issue.path == "options.acquisition_limits"));
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
fn test_configuration_modules_child_policy_cannot_expand_remote_leaf_access() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-policy-{}", std::process::id()));
    create_dir_all(&root).expect("create configuration directory");
    let parent = root.join("config.json");
    let child = root.join("child.json");
    write(
        &parent,
        r#"{
            "requirements": [],
            "modules": [{ "name": "Group", "type": "configuration", "uri": "file:./child.json" }]
        }"#,
    )
    .expect("write parent configuration");
    write(
        &child,
        r#"{
            "requirements": [],
            "options": { "allowed": ["child.example"] },
            "modules": [{
                "name": "Leaf",
                "type": "script",
                "uri": {
                    "scheme": "https",
                    "uri": "https://parent.example/module",
                    "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                }
            }]
        }"#,
    )
    .expect("write child configuration");
    let hosts = vec!["parent.example".to_string()];
    let policy = acquisition_policy(Some(&hosts), false, false);
    let cache = AcquisitionCache::new(root.join("cache"));
    let error = Config::read(parent.clone())
        .expect("read parent configuration")
        .resolve_with_policy(&parent, &policy, &cache, false)
        .expect_err("child policy must intersect root policy");
    assert!(error.is_availability());
    assert!(error.to_string().contains("Group::Leaf"));
    remove_dir_all(root).expect("remove configuration test directory");
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
fn test_configuration_modules_preserve_nested_context_with_policy() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-context-{}", std::process::id()));
    let child_directory = root.join("child");
    let grandchild_directory = child_directory.join("grandchild");
    create_dir_all(&grandchild_directory).expect("create nested configuration directories");
    let parent = root.join("config.json");
    let child = child_directory.join("config.json");
    let grandchild = grandchild_directory.join("config.json");
    write(
        &parent,
        r#"{
            "input": ["root-input"],
            "output": "root-output",
            "requirements": ["sh"],
            "variables": { "PARENT": "parent" },
            "modules": [{
                "name": "Outer",
                "type": "configuration",
                "uri": "file:./child/config.json",
                "variables": { "WRAPPER": "outer" }
            }]
        }"#,
    )
    .expect("write parent configuration");
    write(
        &child,
        r#"{
            "input": ["ignored-child-input"],
            "output": "child-output",
            "requirements": ["python", "sh"],
            "variables": { "CHILD": "child" },
            "modules": [{
                "name": "Inner",
                "type": "configuration",
                "uri": "file:./grandchild/config.json",
                "variables": { "WRAPPER": "inner" }
            }]
        }"#,
    )
    .expect("write child configuration");
    write(
        &grandchild,
        r#"{
            "requirements": ["ruby"],
            "variables": { "GRANDCHILD": "grandchild" },
            "modules": [{
                "name": "Leaf",
                "type": "script",
                "uri": "file:./module",
                "variables": { "LEAF": "leaf" },
                "template": { "command": "true", "arguments": [] }
            }]
        }"#,
    )
    .expect("write grandchild configuration");
    let policy = acquisition_policy(None, false, false);
    let cache = AcquisitionCache::new(root.join("cache"));
    let config = Config::read(parent.clone())
        .expect("read parent configuration")
        .resolve_with_policy(&parent, &policy, &cache, false)
        .expect("resolve nested configuration");
    assert_eq!(config.requirements, vec!["sh", "python", "ruby"]);
    assert_eq!(config.modules[0].module_path, vec!["Outer", "Inner", "Leaf"]);
    assert_eq!(config.clone().get_module_list(), "outer::inner::leaf");
    assert!(matches!(config.input, Some(InputAttribute::List(ref values)) if values == &["root-input"]));
    let variables = config.modules[0].variables.as_ref().expect("merged variables");
    assert_eq!(variables["PARENT"], "parent");
    assert_eq!(variables["WRAPPER"], "inner");
    assert_eq!(variables["CHILD"], "child");
    assert_eq!(variables["GRANDCHILD"], "grandchild");
    assert_eq!(variables["LEAF"], "leaf");
    assert_eq!(variables["OUTPUT_DIRECTORY"], "child-output");
    match &config.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert!(<&str>::from(uri).ends_with("/child/grandchild/module"), "{uri}"),
        | _ => panic!("nested leaf should remain a script module"),
    }
    let mut overridden = Config::read(parent.clone()).expect("read parent configuration for output override");
    overridden
        .variables
        .get_or_insert_default()
        .insert("OUTPUT_DIRECTORY".to_string(), Value::String("artifact-output".to_string()));
    let overridden = overridden
        .resolve_with_policy(&parent, &policy, &cache, false)
        .expect("resolve nested configuration with output override");
    assert_eq!(
        overridden.modules[0].variables.as_ref().expect("merged override variables")["OUTPUT_DIRECTORY"],
        "artifact-output"
    );
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
fn test_configuration_modules_reject_http_source_cycles_before_a_second_request() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let uri = format!("http://{address}/config.json");
        let nested_checksum = format!("sha256:{}", "a".repeat(64));
        let document = serde_json::json!({
            "requirements": [],
            "modules": [{ "checksum": nested_checksum, "name": "Again", "type": "configuration", "uri": uri }]
        })
        .to_string();
        let document_checksum = checksum(&document);
        listener.set_nonblocking(false).expect("use blocking listener");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept configuration request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read configuration request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{document}",
                document.len()
            );
            stream.write_all(response.as_bytes()).expect("write configuration response");
        });
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [],
            "modules": [{
                "checksum": document_checksum,
                "name": "Outer",
                "type": "configuration",
                "uri": format!("http://{address}/config.json")
            }]
        }))
        .expect("valid root configuration");
        let hosts = vec![address.to_string()];
        let policy = acquisition_policy(Some(&hosts), true, false);
        let cache = AcquisitionCache::new(std::env::temp_dir().join(format!("xylem-cycle-cache-{address}")));
        let error = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
            .expect_err("HTTP source cycle should fail");
        server.join().expect("configuration server");
        assert!(matches!(error, ConfigurationModuleError::Cycle { .. }));
        assert!(error.to_string().contains("Outer::Again"));
    }
}
#[test]
fn test_configuration_modules_reject_offline_remote_sources() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "checksum": format!("sha256:{}", "a".repeat(64)),
            "name": "Remote",
            "type": "configuration",
            "uri": "https://example.com/config.json"
        }],
        "options": { "offline": true }
    }))
    .expect("valid root configuration");
    let hosts = vec!["example.com".to_string()];
    let policy = acquisition_policy(Some(&hosts), false, true);
    let cache = AcquisitionCache::new(std::env::temp_dir().join("xylem-offline-configuration-cache"));
    let error = config
        .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
        .expect_err("offline remote source should fail");
    assert!(error.is_availability());
    assert!(error.to_string().contains("offline acquisition cache miss"));
}
#[test]
fn test_configuration_modules_reject_revisions_for_http_documents() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "Remote",
            "type": "configuration",
            "uri": {
                "scheme": "https",
                "uri": "https://example.com/config.json",
                "revision": "main"
            }
        }]
    }))
    .expect("valid structured location");
    let hosts = vec!["example.com".to_string()];
    let error = config
        .resolve_with_policy(
            Path::new("config.json"),
            &acquisition_policy(Some(&hosts), false, false),
            &AcquisitionCache::new(std::env::temp_dir().join("xylem-revision-configuration-cache")),
            false,
        )
        .expect_err("direct document revision should fail");
    assert!(matches!(error, ConfigurationModuleError::Revision { .. }));
}
#[test]
fn test_configuration_modules_reject_wrapper_fields_with_policy() {
    let root = std::env::temp_dir().join(format!("xylem-configuration-wrapper-{}", std::process::id()));
    create_dir_all(&root).expect("create configuration directory");
    let parent = root.join("config.json");
    let child = root.join("child.json");
    write(&child, r#"{"requirements":[],"modules":[]}"#).expect("write child configuration");
    write(
        &parent,
        r#"{
            "requirements": [],
            "modules": [{
                "name": "Group",
                "type": "configuration",
                "uri": "file:./child.json",
                "template": { "command": "true", "arguments": [] }
            }]
        }"#,
    )
    .expect("write parent configuration");
    let error = Config::read(parent.clone())
        .expect("read parent configuration")
        .resolve_with_policy(
            &parent,
            &acquisition_policy(None, false, false),
            &AcquisitionCache::new(root.join("cache")),
            false,
        )
        .expect_err("wrapper template should fail");
    assert!(matches!(error, ConfigurationModuleError::Wrapper { .. }));
    remove_dir_all(root).expect("remove configuration test directory");
}
#[test]
fn test_configuration_modules_remote_dry_run_is_request_free() {
    if let Some(listener) = bind_local_server() {
        listener.set_nonblocking(true).expect("make listener nonblocking");
        let address = listener.local_addr().expect("server address");
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [],
            "modules": [{
                "checksum": format!("sha256:{}", "a".repeat(64)),
                "name": "Remote",
                "type": "configuration",
                "uri": format!("http://{address}/config.json")
            }]
        }))
        .expect("valid root configuration");
        let hosts = vec![address.to_string()];
        let policy = acquisition_policy(Some(&hosts), true, false);
        let cache = AcquisitionCache::new(std::env::temp_dir().join(format!("xylem-dry-run-cache-{address}")));
        let error = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, true)
            .expect_err("remote dry run should fail");
        assert!(matches!(error, ConfigurationModuleError::RemoteDryRun { .. }));
        assert!(listener.accept().is_err(), "dry run must not connect to the remote source");
    }
}
#[test]
fn test_configuration_modules_report_malformed_http_documents_with_context() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let body = "{".to_string();
        let body_checksum = checksum(&body);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept configuration request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read configuration request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write configuration response");
        });
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [],
            "modules": [{
                "checksum": body_checksum,
                "name": "Remote",
                "type": "configuration",
                "uri": format!("http://{address}/config.json")
            }]
        }))
        .expect("valid root configuration");
        let hosts = vec![address.to_string()];
        let policy = acquisition_policy(Some(&hosts), true, false);
        let cache = AcquisitionCache::new(std::env::temp_dir().join(format!("xylem-malformed-cache-{address}")));
        let error = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
            .expect_err("malformed remote document should fail");
        server.join().expect("configuration server");
        assert!(matches!(error, ConfigurationModuleError::Read { .. }));
        assert!(error.to_string().contains("Remote"));
    }
}
#[test]
fn test_configuration_modules_resolve_nested_http_documents() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let root = std::env::temp_dir().join(format!("xylem-configuration-http-{}", std::process::id()));
        create_dir_all(&root).expect("create HTTP configuration test directory");
        let child_uri = format!("http://{address}/child.json");
        let child_document = serde_json::json!({
            "requirements": ["child-tool"],
            "modules": [{
                "checksum": format!("sha256:{}", "a".repeat(64)),
                "name": "Leaf",
                "type": "binary",
                "uri": format!("http://{address}/module-source"),
                "template": { "command": "true", "arguments": [] }
            }]
        })
        .to_string();
        let child_checksum = checksum(&child_document);
        let parent_document = serde_json::json!({
            "requirements": ["parent-tool"],
            "modules": [{
                "checksum": child_checksum,
                "name": "Inner",
                "type": "configuration",
                "uri": child_uri
            }]
        })
        .to_string();
        let parent_checksum = checksum(&parent_document);
        let server = thread::spawn(move || {
            for body in [parent_document, child_document] {
                let (mut stream, _) = listener.accept().expect("accept configuration request");
                let mut request = [0_u8; 1024];
                let count = stream.read(&mut request).expect("read configuration request");
                assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET "));
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).expect("write configuration response");
            }
        });
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": ["root-tool"],
            "modules": [{
                "checksum": parent_checksum,
                "name": "Outer",
                "type": "configuration",
                "uri": format!("http://{address}/parent.json")
            }]
        }))
        .expect("valid root configuration");
        let hosts = vec![address.to_string()];
        let policy = acquisition_policy(Some(&hosts), true, false);
        let cache = AcquisitionCache::new(root.join("cache"));
        let resolved = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
            .expect("resolve nested HTTP configurations");
        server.join().expect("configuration server");
        assert_eq!(resolved.requirements, vec!["root-tool", "parent-tool", "child-tool"]);
        assert_eq!(resolved.modules[0].module_path, vec!["Outer", "Inner", "Leaf"]);
        remove_dir_all(root).expect("remove HTTP configuration test directory");
    }
}
#[test]
fn test_checksum_escape_hatch_allows_an_unpinned_remote_binary() {
    let module = serde_json::json!({
        "modules": [{
            "name": "Remote",
            "type": "binary",
            "uri": "https://example.org/tool"
        }],
        "requirements": []
    });
    let denied: Config = serde_json::from_value(module.clone()).expect("parse remote binary configuration");
    assert!(denied.validate().is_err());
    let mut allowed: Config = serde_json::from_value(module).expect("parse checksum escape hatch configuration");
    allowed.options = Some(ConfigOptions::init().skip_verify_checksum(true).build());
    assert!(allowed.validate().is_ok());
}
#[test]
fn test_remote_binary_is_verified_and_copied_into_the_input_workspace() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let body = "#!/bin/sh\nexit 0\n";
        let expected = body.as_bytes().to_vec();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept binary request");
            let mut request = [0_u8; 1024];
            let count = stream.read(&mut request).expect("read binary request");
            assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET "));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write binary response");
        });
        let config: Config = serde_json::from_value(serde_json::json!({
            "modules": [{
                "checksum": checksum(body),
                "name": "Remote",
                "type": "binary",
                "uri": format!("http://{address}/tool")
            }],
            "requirements": []
        }))
        .expect("valid remote binary configuration");
        let hosts = vec![address.to_string()];
        let policy = acquisition_policy(Some(&hosts), true, false);
        let root = std::env::temp_dir().join(format!("xylem-binary-workspace-{address}"));
        let cache = AcquisitionCache::new(root.join("cache"));
        let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("create run workspace");
        let input = workspace.input(0).expect("create input workspace");
        let prepared = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
            .expect("resolve remote binary")
            .prepare_modules_selected_in_workspace(&BTreeSet::from([0]), None, &input)
            .expect("prepare remote binary");
        server.join().expect("binary server");
        let module = &prepared.modules[0];
        let path = prepared
            .options
            .as_ref()
            .and_then(|options| options.workspace_modules.get(&0))
            .expect("materialized binary path");
        assert!(path.starts_with(input.modules()));
        assert_eq!(std::fs::read(path).expect("read materialized binary"), expected);
        assert_eq!(
            module.acquisition.as_ref().expect("remote acquisition evidence").cache,
            AcquisitionCacheStatus::Miss
        );
        workspace.close().expect("close run workspace");
        remove_dir_all(root).expect("remove remote binary test directory");
    }
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
    let mut configurations = configuration_files(Path::new("../../examples"));
    configurations.extend(configuration_files(Path::new("../../tests")));
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
fn test_json_and_yaml_selection_fixtures_deserialize_equally() {
    let json = Config::read_json(PathBuf::from(format!("{FIXTURES}/config_with_selection_input.json"))).expect("read selection JSON fixture");
    let yaml = Config::read_yaml(PathBuf::from(format!("{FIXTURES}/config_with_selection_input.yaml"))).expect("read selection YAML fixture");
    let json_value = serde_json::to_value(json).expect("normalize selection JSON fixture");
    let yaml_value = serde_json::to_value(yaml).expect("normalize selection YAML fixture");
    assert_eq!(json_value, yaml_value);
    assert_eq!(json_value["id"], "selection-fixture");
    assert_eq!(json_value["input"]["select"]["engine"], "cel");
    assert_eq!(json_value["input"]["select"]["options"]["roots"].as_array().map(Vec::len), Some(2));
    assert_eq!(json_value["variables"]["selection"]["extensions"][1], ".tif");
    assert_eq!(json_value["variables"]["selection"]["minimum_size"], 1024);
}
#[test]
fn test_jsonc_accepts_comments_and_trailing_commas_but_rejects_loose_json() {
    let root = std::env::temp_dir().join(format!("xylem-jsonc-config-{}", std::process::id()));
    create_dir_all(&root).expect("create JSONC test root");
    let valid = root.join("workflow.jsonc");
    write(
        &valid,
        r#"{
            // strict JSONC supports comments
            "requirements": [],
            "modules": [],
        }"#,
    )
    .expect("write valid JSONC");
    let config = Config::try_from(valid.as_path()).expect("valid JSONC should load");
    assert!(config.modules.is_empty());
    for (name, content) in [
        ("loose-name.jsonc", "{ requirements: [], \"modules\": [] }"),
        ("single-quote.jsonc", "{ 'requirements': [], \"modules\": [] }"),
        ("missing-comma.jsonc", "{ \"requirements\": [] \"modules\": [] }"),
    ] {
        let path = root.join(name);
        write(&path, content).expect("write invalid JSONC");
        assert!(Config::try_from(path.as_path()).is_err(), "{name} should fail");
    }
    remove_dir_all(root).expect("remove JSONC test root");
}
#[test]
fn test_managed_container_keep_alive_round_trips_and_validates() {
    let document = serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "managed",
            "type": "script",
            "uri": "file:.",
            "template": {
                "command": "echo",
                "arguments": [],
                "container": {
                    "name": "worker",
                    "create": true,
                    "image": "alpine:3.22",
                    "keep_alive": true,
                    "runtime": "docker"
                }
            }
        }]
    });
    let config: Config = serde_json::from_value(document.clone()).expect("keep-alive JSON configuration");
    assert!(config.validate().is_ok());
    let serialized = serde_json::to_value(&config).expect("serialize keep-alive JSON configuration");
    assert_eq!(serialized["modules"][0]["template"]["container"]["keep_alive"], true);
    let yaml = serde_norway::to_string(&config).expect("serialize keep-alive YAML configuration");
    let yaml = parse_yaml_configuration(&yaml).expect("deserialize keep-alive YAML configuration");
    let yaml = serde_json::to_value(yaml).expect("serialize round-tripped YAML configuration");
    assert_eq!(yaml["modules"][0]["template"]["container"]["keep_alive"], true);
    let mut omitted = document;
    omitted["modules"][0]["template"]["container"]
        .as_object_mut()
        .expect("container object")
        .remove("keep_alive");
    let omitted: Config = serde_json::from_value(omitted).expect("configuration without keep_alive");
    let omitted = serde_json::to_value(omitted).expect("serialize default keep_alive");
    assert_eq!(omitted["modules"][0]["template"]["container"]["keep_alive"], false);
    let invalid = [
        (
            serde_json::json!({"name": "worker", "keep_alive": true, "runtime": "docker"}),
            "set create to true to use managed container fields",
        ),
        (
            serde_json::json!({"name": "worker", "create": true, "image": "alpine:3.22", "keep_alive": true, "runtime": "apptainer"}),
            "managed containers require docker or podman",
        ),
        (
            serde_json::json!({"name": "worker", "create": true, "image": "alpine:3.22", "keep_alive": true, "runtime": "unknown"}),
            "container runtime is not supported",
        ),
        (
            serde_json::json!({
                "name": "worker",
                "command": ["serve"],
                "create": true,
                "image": "alpine:3.22",
                "keep_alive": true,
                "runtime": "docker"
            }),
            "keep_alive must not be combined with command",
        ),
    ];
    invalid.into_iter().for_each(|(container, message)| {
        let mut document = serde_json::json!({
            "requirements": [],
            "modules": [{
                "name": "invalid",
                "type": "script",
                "uri": "file:.",
                "template": {"arguments": [], "command": "echo"}
            }]
        });
        document["modules"][0]["template"]["container"] = container;
        let config: Config = serde_json::from_value(document).expect("deserialize invalid semantic configuration");
        let error = config.validate().expect_err("keep-alive declaration must be rejected").to_string();
        assert!(error.contains(message), "{error}");
    });
}
#[test]
fn test_managed_container_validation_requires_owned_fields() {
    let managed: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "managed",
            "type": "script",
            "uri": "file:.",
            "template": {
                "command": "echo",
                "arguments": [],
                "container": {
                    "name": "worker",
                    "create": true,
                    "image": "alpine:3.22",
                    "runtime": "docker",
                    "mounts": [{"source": ".", "target": "/workspace"}]
                }
            }
        }]
    }))
    .expect("managed container configuration");
    assert!(managed.validate().is_ok());
    let external_with_image: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "external",
            "type": "script",
            "uri": "file:.",
            "template": {
                "command": "echo",
                "arguments": [],
                "container": {"name": "worker", "image": "alpine:3.22", "runtime": "docker"}
            }
        }]
    }))
    .expect("external container configuration");
    assert!(external_with_image.validate().is_err());
}
#[test]
fn test_memory_configuration_accepts_arbitrary_module_sets() {
    let config = memory_configuration(
        memory_estimator("custom-x-y-z-v1"),
        vec![memory_module("Module X"), memory_module("Module Y"), memory_module("Module Z")],
        "64GB",
    );
    assert!(config.validate().is_ok());

    let empty = memory_configuration(memory_estimator("zero-module-v1"), Vec::new(), "64GB");
    assert!(empty.validate().is_ok());
}
#[test]
fn test_memory_configuration_rejects_incomplete_or_invalid_budgets() {
    let estimator = memory_estimator("custom-modules-v1");
    let modules = vec![memory_module("Anything")];
    let zero = memory_configuration(estimator.clone(), modules.clone(), "0GB");
    let direct = zero
        .options
        .as_ref()
        .and_then(|options| options.memory.as_ref())
        .expect("memory options")
        .validate()
        .expect_err("memory options must validate independently");
    assert!(direct.iter().any(|issue| issue.path == "budget"));
    let zero = zero.validate().expect_err("zero memory budget must fail");
    assert!(zero.iter().any(|issue| issue.path == "options.memory.budget"));
    let mut nonfinite = memory_configuration(estimator.clone(), modules.clone(), "64GB");
    nonfinite
        .options
        .as_mut()
        .and_then(|options| options.memory.as_mut())
        .expect("memory options")
        .budget = Memory::gb(f64::INFINITY);
    let nonfinite = nonfinite.validate().expect_err("nonfinite memory budget must fail");
    assert!(nonfinite.iter().any(|issue| issue.path == "options.memory.budget"));
    for memory in [serde_json::json!({"budget": "64GB"}), serde_json::json!({"estimator": estimator.clone()})] {
        let document = serde_json::json!({"requirements": [], "modules": modules.clone(), "options": {"memory": memory}});
        assert!(serde_json::from_value::<Config>(document).is_err());
    }
    for budget in [serde_json::json!(-1), serde_json::json!("invalid")] {
        let document = serde_json::json!({
            "requirements": [],
            "modules": [],
            "options": {"memory": {"budget": budget, "estimator": estimator.clone()}}
        });
        assert!(serde_json::from_value::<Config>(document).is_err());
    }
}
#[test]
fn test_memory_configuration_rejects_invalid_estimators() {
    let invalid_identity = memory_configuration(memory_estimator("  "), vec![memory_module("Anything")], "64GB")
        .validate()
        .expect_err("empty estimator identity must fail");
    assert!(invalid_identity.iter().any(|issue| issue.path == "options.memory.estimator.identity"));

    let mut reversed = memory_estimator("reversed-range-v1");
    reversed["maximum_input"] = serde_json::json!("1KB");
    reversed["minimum_input"] = serde_json::json!("1TB");
    let reversed = memory_configuration(reversed, vec![memory_module("Anything")], "64GB")
        .validate()
        .expect_err("reversed estimator range must fail");
    assert!(reversed.iter().any(|issue| issue.path == "options.memory.estimator.maximum_input"));

    let mut unsafe_multiplier = memory_estimator("unsafe-multiplier-v1");
    unsafe_multiplier["safety_millionths"] = serde_json::json!(999_999);
    let unsafe_multiplier = memory_configuration(unsafe_multiplier, vec![memory_module("Anything")], "64GB")
        .validate()
        .expect_err("unsafe multiplier must fail");
    assert!(unsafe_multiplier
        .iter()
        .any(|issue| issue.path == "options.memory.estimator.safety_millionths"));

    let mut zero = memory_estimator("zero-prediction-v1");
    zero["intercept"] = serde_json::json!("0GB");
    let zero = memory_configuration(zero, vec![memory_module("Anything")], "64GB")
        .validate()
        .expect_err("zero prediction must fail");
    assert!(zero.iter().any(|issue| issue.path == "options.memory.estimator"));

    let mut nonfinite = memory_configuration(memory_estimator("nonfinite-v1"), vec![memory_module("Anything")], "64GB");
    nonfinite
        .options
        .as_mut()
        .and_then(|options| options.memory.as_mut())
        .expect("memory options")
        .estimator
        .intercept = Memory::gb(f64::INFINITY);
    let nonfinite = nonfinite.validate().expect_err("nonfinite estimator memory must fail");
    assert!(nonfinite.iter().any(|issue| issue.path == "options.memory.estimator.intercept"));
    let mut overflowing = memory_estimator("overflowing-v1");
    overflowing["safety_millionths"] = serde_json::json!(u64::MAX);
    overflowing["slope_millionths"] = serde_json::json!(u64::MAX);
    let overflowing = memory_configuration(overflowing, vec![memory_module("Anything")], "64GB")
        .validate()
        .expect_err("estimator overflow at the calibrated maximum must fail");
    assert!(overflowing.iter().any(|issue| issue.path == "options.memory.estimator.maximum_input"));
}
#[test]
fn test_memory_configuration_round_trips_through_json_and_yaml() {
    let config = memory_configuration(
        memory_estimator("custom-round-trip-v1"),
        vec![memory_module("First"), memory_module("Second")],
        "64GB",
    );
    assert!(config.validate().is_ok());
    let serialized = serde_json::to_value(&config).expect("serialize memory configuration");
    assert_eq!(serialized["options"]["memory"]["budget"], "64GB");
    assert_eq!(serialized["options"]["memory"]["estimator"]["identity"], "custom-round-trip-v1");
    assert_eq!(serialized["options"]["memory"]["estimator"]["slope_millionths"], 0);
    let yaml = serde_norway::to_string(&config).expect("serialize YAML memory configuration");
    let round_trip = parse_yaml_configuration(&yaml).expect("deserialize YAML memory configuration");
    assert_eq!(
        round_trip
            .options
            .and_then(|options| options.memory)
            .and_then(|memory| memory.budget.checked_bytes()),
        Some(64 * 1024 * 1024 * 1024)
    );
}
#[test]
fn test_non_local_environment_managers_are_rejected() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "poetry",
            "programmingLanguage": "python",
            "template": {
                "arguments": [],
                "command": "python",
                "environment": { "manager": "poetry", "name": "default" }
            },
            "type": "script",
            "uri": "file:./module"
        }],
        "requirements": []
    }))
    .expect("configuration should deserialize before semantic validation");
    let error = config.validate().expect_err("shared Poetry environment should be rejected");
    assert!(error.iter().any(|issue| issue.path == "modules[0].template.environment.manager"));
}
#[test]
fn test_preparation_and_execution_emit_progress_updates() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "probe",
            "type": "script",
            "programmingLanguage": "generic",
            "uri": "file:../../tests/e2e/modules/environment-probe",
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
    assert_eq!(updates[0].module_path, vec!["probe"]);
    assert_eq!(updates[1].state, ProgressState::Finished);
    assert_eq!(updates[2].phase, ProgressPhase::Execution);
    assert_eq!(updates[2].input.as_deref(), Some("input-a"));
    assert_eq!(updates[3].state, ProgressState::Skipped);
}
#[test]
fn test_recorded_workflow_preserves_cancellation_state() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "probe",
            "type": "script",
            "uri": "file:.",
            "template": { "command": "true", "arguments": [] }
        }]
    }))
    .expect("cancellation fixture should parse");
    let options = ConfigOptions::init().root(std::env::temp_dir()).build();
    let outcome = config.run_workflow_recorded(None, &options, None, Some(&CancelledExecutor), None, 0);
    assert!(outcome.state.is_cancelled());
    assert!(outcome.modules[0].state.is_cancelled());
}
#[test]
fn test_sandbox_is_an_opt_in_boolean() {
    let omitted: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": []
    }))
    .expect("configuration without sandbox");
    assert!(!omitted.sandbox);
    let disabled: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "sandbox": false
    }))
    .expect("configuration with disabled sandbox");
    assert!(!disabled.sandbox);
    let enabled: Config = parse_yaml_configuration("requirements: []\nmodules: []\nsandbox: true\n").expect("configuration with enabled sandbox");
    assert!(enabled.sandbox);
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
