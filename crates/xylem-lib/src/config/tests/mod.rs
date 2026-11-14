#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
use crate::acquisition::{AcquisitionCache, AcquisitionCacheStatus, AcquisitionLimits, AcquisitionPolicy};
use crate::config::*;
use crate::execution::{ExecutionContext, ExecutionLimits, InputBatch};
use crate::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use crate::run::{ContextError, RunCancellation, RunContext};
use crate::workspace::RunWorkspace;
use acorn_core::time::{Seconds, UnitOfTime};
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::fs::symlink_metadata;
use std::fs::{create_dir_all, read_dir, read_to_string, remove_dir_all, remove_file, write};
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::sync::Mutex;
use std::thread;

const FIXTURES: &str = "../../tests/fixtures";
struct CancelledExecutor;
#[derive(Default)]
struct RecordingProgress {
    updates: Mutex<Vec<ProgressUpdate>>,
}
impl ModuleExecutor for CancelledExecutor {
    fn execute(&self, script: &Script, _input_index: usize, _module_index: usize, _timeout: Option<Seconds>) -> Result<(), ProcessError> {
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
fn acquisition_policy(hosts: Option<&[String]>, allow_insecure_http: bool, offline: bool) -> AcquisitionPolicy {
    AcquisitionPolicy::from_hosts(hosts, allow_insecure_http, None, offline, AcquisitionLimits::default()).expect("valid acquisition policy")
}
#[allow(clippy::std_instead_of_core)]
fn bind_local_server() -> Option<TcpListener> {
    match TcpListener::bind("127.0.0.1:0") {
        | Ok(listener) => Some(listener),
        | Err(why) if why.kind() == std::io::ErrorKind::PermissionDenied => None,
        | Err(why) => panic!("bind local server: {why}"),
    }
}
fn checksum(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
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
fn test_context_variables_apply_precedence_and_derived_values() {
    let root = std::env::temp_dir().join(format!("xylem-context-precedence-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root, "run").expect("create run workspace");
    let context = RunContext::init()
        .artifact_output(root.join("cli-output"))
        .expect("UTF-8 artifact output")
        .batch(InputBatch::try_from(2).expect("positive batch"))
        .cancellation(RunCancellation::default())
        .failure_policy(FailurePolicy::Continue)
        .limits(ExecutionLimits::try_from(4).expect("positive thread limit"))
        .retain_workspace(true)
        .run_id("run-id".to_string())
        .expect("safe run id")
        .configuration_directory(root.clone())
        .expect("UTF-8 configuration directory")
        .invocation_directory(root.clone())
        .expect("UTF-8 invocation directory")
        .workspace(workspace)
        .expect("UTF-8 workspace")
        .build();
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "Probe Module",
            "programmingLanguage": "generic",
            "template": { "arguments": [], "command": "true" },
            "type": "script",
            "uri": "file:.",
            "variables": {
                "COMPOSED": "{{ BASE }}:{{ LAYER }}:{{ RUN_ID }}",
                "LAYER": "module",
                "OUTPUT_DIRECTORY": "module-output"
            }
        }],
        "output": "configuration-output",
        "requirements": [],
        "variables": { "BASE": "workflow", "LAYER": "workflow" }
    }))
    .expect("context fixture");
    let workflow = context.workflow_context(0, Some("/data/repeat.tif".to_string())).expect("input context");
    let variables = workflow.variables(&config, config.modules.first()).expect("resolved variables");
    assert_eq!(variables["BASE"], "workflow");
    assert_eq!(variables["COMPOSED"], "workflow:module:run-id");
    assert_eq!(variables["INPUT"], "/data/repeat.tif");
    assert_eq!(variables["INPUT_BASENAME"], "repeat.tif");
    assert_eq!(variables["LAYER"], "module");
    assert_eq!(variables["MODULE_LIST"], "probe_module");
    assert_eq!(variables["OUTPUT_DIRECTORY"], root.join("cli-output").display().to_string());
    assert_eq!(variables["RUN_ID"], "run-id");
    assert_eq!(variables["WORK_DIRECTORY"], workflow.workspace().root().display().to_string());
    let repeated = context
        .workflow_context(1, Some("/data/other.tif".to_string()))
        .expect("second input context");
    let [environment, repeated_environment] = thread::scope(|scope| {
        let first = scope.spawn(|| workflow.command_environment(&config, config.modules.first().expect("module")));
        let second = scope.spawn(|| repeated.command_environment(&config, config.modules.first().expect("module")));
        [
            first.join().expect("first context thread").expect("first command environment"),
            second.join().expect("second context thread").expect("second command environment"),
        ]
    });
    assert_eq!(environment["_XYLEM_COMPOSED"], "workflow:module:run-id");
    assert_eq!(environment["_XYLEM_INPUT"], "/data/repeat.tif");
    assert_eq!(environment["_XYLEM_INPUT_BASENAME"], "repeat.tif");
    assert_eq!(repeated_environment["_XYLEM_INPUT"], "/data/other.tif");
    assert_eq!(repeated_environment["_XYLEM_INPUT_BASENAME"], "other.tif");
    assert!(!environment.contains_key("COMPOSED"));
    assert_eq!(context.batch().map(InputBatch::get), Some(2));
    assert_eq!(context.failure_policy(), FailurePolicy::Continue);
    assert!(context.retain_workspace());
    let journal = root.join("journal.jsonl");
    context.bind_journal_path(journal.clone()).expect("bind journal path");
    assert_eq!(context.journal_path(), Some(journal.as_path()));
    assert!(matches!(
        context.bind_journal_path(root.join("second.jsonl")),
        Err(ContextError::JournalAlreadyBound)
    ));
    drop((repeated, workflow));
    let retained = context.retain();
    remove_dir_all(retained).expect("remove retained context workspace");
}
#[cfg(unix)]
#[test]
fn test_context_variables_reject_non_unicode_paths() {
    let root = std::env::temp_dir().join(format!("xylem-context-unicode-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root, "run").expect("create run workspace");
    let invalid = PathBuf::from(std::ffi::OsString::from_vec(vec![b'o', b'u', b't', 0x80]));
    let error = RunContext::init()
        .limits(ExecutionLimits::default())
        .run_id("run-id".to_string())
        .expect("safe run id")
        .configuration_directory(root.clone())
        .expect("UTF-8 configuration directory")
        .invocation_directory(root.clone())
        .expect("UTF-8 invocation directory")
        .workspace(workspace)
        .expect("UTF-8 workspace")
        .artifact_output(invalid)
        .expect_err("non-Unicode output must fail");
    assert!(matches!(
        error,
        ContextError::NonUnicodePath {
            field: "artifact output",
            ..
        }
    ));
    let _ = remove_dir_all(root);
}
#[test]
fn test_context_variables_reject_reserved_names() {
    for value in [
        serde_json::json!({ "modules": [], "requirements": [], "variables": { "INPUT_BASENAME": "forged" } }),
        serde_json::json!({
            "modules": [{ "name": "probe", "type": "script", "uri": "file:.", "variables": { "INPUT_BASENAME": "forged" } }],
            "requirements": []
        }),
    ] {
        let config: Config = serde_json::from_value(value).expect("reserved basename fixture");
        let error = config.validate().expect_err("input basename must be reserved");
        assert!(error.to_string().contains("variables.INPUT_BASENAME"));
    }
    let workflow: Config = serde_json::from_value(serde_json::json!({
        "modules": [],
        "requirements": [],
        "variables": { "RUN_ID": "forged" }
    }))
    .expect("reserved workflow fixture");
    let error = workflow.validate().expect_err("workflow reserved name must fail");
    assert!(error.to_string().contains("variables.RUN_ID"));
    let module: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "probe",
            "type": "script",
            "uri": "file:.",
            "variables": { "INPUT": "forged" }
        }],
        "requirements": []
    }))
    .expect("reserved module fixture");
    let error = module.validate().expect_err("module reserved name must fail");
    assert!(error.to_string().contains("modules[0].variables.INPUT"));
}
#[test]
fn test_contexts_isolate_repeated_and_zero_inputs() {
    let root = std::env::temp_dir().join(format!("xylem-context-inputs-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root, "run").expect("create run workspace");
    let context = RunContext::init()
        .limits(ExecutionLimits::default())
        .run_id("run-id".to_string())
        .expect("safe run id")
        .configuration_directory(root.clone())
        .expect("UTF-8 configuration directory")
        .invocation_directory(root.clone())
        .expect("UTF-8 invocation directory")
        .workspace(workspace)
        .expect("UTF-8 workspace")
        .build();
    let config: Config = serde_json::from_value(serde_json::json!({ "modules": [], "requirements": [], "variables": { "VALUE": "shared" } }))
        .expect("zero-input fixture");
    let first = context.workflow_context(0, Some("same".to_string())).expect("first input context");
    let second = context.workflow_context(1, Some("same".to_string())).expect("second input context");
    let zero = context.workflow_context(2, None).expect("zero-input context");
    assert_ne!(first.workspace().root(), second.workspace().root());
    assert_ne!(first.workspace().environments(), second.workspace().environments());
    assert_eq!(first.variables(&config, None).expect("first variables")["INPUT"], "same");
    let zero_variables = zero.variables(&config, None).expect("zero-input variables");
    assert!(!zero_variables.contains_key("INPUT"));
    assert!(!zero_variables.contains_key("INPUT_BASENAME"));
    assert_eq!(zero_variables["MODULE_LIST"], "");
    assert_eq!(zero_variables["RUN_ID"], "run-id");
    drop((first, second, zero));
    context.close().expect("close run context");
    let _ = remove_dir_all(root);
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
fn test_failure_policy_and_module_timeout_round_trip_across_formats() {
    let json = r#"{
        "requirements": [],
        "modules": [{"name": "probe", "type": "script", "uri": "file:.", "timeout_seconds": 30}],
        "options": {"failure_policy": "continue"}
    }"#;
    let jsonc = r#"{
        // Continue independent inputs after a failure.
        "requirements": [],
        "modules": [{"name": "probe", "type": "script", "uri": "file:.", "timeout_seconds": 30}],
        "options": {"failure_policy": "continue"},
    }"#;
    let yaml = "requirements: []\nmodules:\n  - name: probe\n    type: script\n    uri: 'file:.'\n    timeout_seconds: 30\noptions:\n  failure_policy: continue\n";
    let yml_path = std::env::temp_dir().join(format!("xylem-policy-timeout-{}.yml", std::process::id()));
    write(&yml_path, yaml).expect("write YML policy");
    let yml = Config::try_from(yml_path.clone()).expect("parse YML policy");
    remove_file(yml_path).expect("remove YML policy");
    let configurations = [
        parse_json_configuration(json).expect("parse JSON policy"),
        parse_jsonc_configuration(jsonc).expect("parse JSONC policy"),
        parse_yaml_configuration(yaml).expect("parse YAML policy"),
        yml,
    ];
    configurations.iter().for_each(|config| {
        assert_eq!(
            config.options.as_ref().map(|options| options.failure_policy),
            Some(FailurePolicy::Continue)
        );
        assert_eq!(config.modules[0].timeout, Some(Seconds::new(30)));
        config.validate().expect("validate policy and timeout");
    });
    let json = serde_json::to_string(&configurations[0]).expect("serialize JSON policy");
    let json = parse_json_configuration(&json).expect("round trip JSON policy");
    let yaml = serde_norway::to_string(&configurations[0]).expect("serialize YAML policy");
    let yaml = parse_yaml_configuration(&yaml).expect("round trip YAML policy");
    assert_eq!(json.options.map(|options| options.failure_policy), Some(FailurePolicy::Continue));
    assert_eq!(yaml.modules[0].timeout, Some(Seconds::new(30)));
}
#[test]
fn test_failure_policy_defaults_and_unknown_values() {
    assert_eq!(FailurePolicy::Continue.to_string(), "continue");
    assert_eq!(FailurePolicy::FailFast.to_string(), "fail_fast");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{"name": "probe", "type": "binary", "uri": "file:./probe"}],
        "options": {}
    }))
    .expect("default failure policy");
    assert_eq!(config.options.map(|options| options.failure_policy), Some(FailurePolicy::FailFast));
    assert_eq!(ConfigOptions::init().build().failure_policy, FailurePolicy::FailFast);
    let explicit: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": {"failure_policy": "fail_fast"}
    }))
    .expect("explicit fail-fast policy");
    assert_eq!(
        serde_json::to_value(explicit).expect("serialize fail-fast policy")["options"]["failure_policy"],
        "fail_fast"
    );
    assert!(parse_json_configuration(r#"{"requirements":[],"modules":[],"options":{"failure_policy":"fail-fast"}}"#).is_err());
    assert!(parse_json_configuration(r#"{"requirements":[],"modules":[],"options":{"failure_policy":"unknown"}}"#).is_err());
    assert!(parse_jsonc_configuration(r#"{"requirements":[],"modules":[],"options":{"failure_policy":"unknown"}}"#).is_err());
    assert!(parse_yaml_configuration("requirements: []\nmodules: []\noptions:\n  failure_policy: unknown\n").is_err());
}
#[test]
fn test_input_basename_uses_final_path_component() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [],
        "requirements": [],
        "variables": { "LABEL": "scene:{{ INPUT_BASENAME }}" }
    }))
    .expect("input basename fixture");
    for (input, expected) in [
        ("/data/basin_range", "basin_range"),
        ("/data/basin_range/", "basin_range"),
        ("/data/basin_range/elevation.tif", "elevation.tif"),
        ("relative/name with spaces.tif", "name with spaces.tif"),
        ("single", "single"),
        ("/", "/"),
        (".", "."),
        ("..", ".."),
        ("", ""),
    ] {
        let variables = config.module_environment(None, Some(input));
        assert_eq!(variables["_XYLEM_INPUT_BASENAME"], expected, "basename for {input:?}");
        assert_eq!(variables["_XYLEM_LABEL"], format!("scene:{expected}"));
    }
    let variables = config.module_environment(None, None);
    assert!(!variables.contains_key("_XYLEM_INPUT_BASENAME"));
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
fn test_local_oci_archive_materialization_rewrites_sandbox_source() {
    let root = std::env::temp_dir().join(format!("xylem-oci-sandbox-handoff-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("create OCI handoff workspace");
    let input = workspace.input(0).expect("create OCI handoff input workspace");
    let archive = std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle.tar"))
        .expect("canonical OCI archive fixture");
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let source = format!("oci-layout:{}@{digest}", archive.display());
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "archive",
            "type": "script",
            "uri": source,
            "template": { "command": "sh", "arguments": ["run.sh"] }
        }],
        "requirements": [],
        "sandbox": true
    }))
    .expect("local OCI archive configuration");
    let options = ConfigOptions::init()
        .build()
        .with_acquisition_cache(Some(AcquisitionCache::new(root.join("cache"))))
        .with_offline(Some(true));
    let prepared = config
        .with_options(options)
        .materialize_modules_selected_in_workspace(&BTreeSet::from([0]), None, &input)
        .expect("materialize local OCI archive for sandbox");
    let path = prepared
        .options
        .as_ref()
        .and_then(|options| options.workspace_modules.get(&0))
        .expect("materialized OCI workspace path");
    let uri = match &prepared.modules[0].module_type {
        | ModuleType::Script { uri, .. } => <&str>::from(uri),
        | _ => panic!("OCI archive fixture should remain a script module"),
    };
    assert_eq!(uri, format!("file:{}", path.display()));
    let evidence = prepared.modules[0].acquisition.as_ref().expect("local OCI acquisition evidence");
    assert_eq!(evidence.source, source);
    assert!(evidence.policy.offline);
    workspace.close().expect("close OCI handoff workspace");
    remove_dir_all(root).expect("remove OCI handoff fixture");
}
#[test]
fn test_local_oci_archive_paths_follow_declaring_configuration() {
    let root = std::env::temp_dir().join(format!("xylem-oci-archive-config-{}", std::process::id()));
    let child = root.join("child");
    create_dir_all(&child).expect("create child configuration directory");
    let root = std::fs::canonicalize(root).expect("canonical fixture root");
    let child = root.join("child");
    write(root.join("direct.tar"), []).expect("write direct archive placeholder");
    write(child.join("nested.tar"), []).expect("write nested archive placeholder");
    let digest = format!("sha256:{}", "a".repeat(64));
    let direct: Config = serde_json::from_value(serde_json::json!({
        "modules": [{"name": "direct", "type": "script", "uri": format!("oci-layout:./direct.tar@{digest}")}],
        "requirements": []
    }))
    .expect("direct configuration");
    let direct = direct.resolve(&root.join("config.json")).expect("resolve direct archive");
    let direct_plan = crate::plan::ExecutionPlan::build(&direct, "config.json".to_string(), &[None], false).expect("plan direct archive");
    assert_eq!(
        direct_plan.modules[0].acquisition.source.to_string(),
        format!("oci-layout:{}@{digest}", root.join("direct.tar").display())
    );
    write(
        child.join("config.json"),
        format!(r#"{{"modules":[{{"name":"nested","type":"script","uri":"oci-layout:./nested.tar@{digest}"}}],"requirements":[]}}"#),
    )
    .expect("write nested configuration");
    let nested: Config = serde_json::from_value(serde_json::json!({
        "modules": [{"name": "child", "type": "configuration", "uri": "file:./child/config.json"}],
        "requirements": []
    }))
    .expect("root configuration");
    let policy = acquisition_policy(None, false, false);
    let cache = AcquisitionCache::new(root.join("cache"));
    let nested = nested
        .resolve_with_policy(&root.join("config.json"), &policy, &cache, false)
        .expect("resolve nested archive");
    let nested_plan = crate::plan::ExecutionPlan::build(&nested, "config.json".to_string(), &[None], false).expect("plan nested archive");
    assert_eq!(
        nested_plan.modules[0].acquisition.source.to_string(),
        format!("oci-layout:{}@{digest}", child.join("nested.tar").display())
    );
    let binary: Config = serde_json::from_value(serde_json::json!({
        "modules": [{"name": "unsupported", "type": "binary", "uri": format!("oci-layout:./direct.tar@{digest}")}],
        "requirements": []
    }))
    .expect("binary archive configuration");
    assert!(binary.validate().is_err());
    remove_dir_all(root).expect("remove archive configuration fixture");
}
#[test]
fn test_local_oci_archive_preparation_executes_from_input_local_tree() {
    let root = std::env::temp_dir().join(format!("xylem-oci-host-preparation-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("create OCI host workspace");
    let input = workspace.input(0).expect("create OCI host input workspace");
    let archive = std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle.tar"))
        .expect("canonical OCI archive fixture");
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let source = format!("oci-layout:{}@{digest}", archive.display());
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "archive",
            "type": "script",
            "uri": source,
            "template": { "command": "sh", "arguments": ["-c", "test -f run.sh"] }
        }],
        "requirements": []
    }))
    .expect("local OCI host configuration");
    let options = ConfigOptions::init()
        .build()
        .with_acquisition_cache(Some(AcquisitionCache::new(root.join("cache"))))
        .with_offline(Some(true));
    let prepared = config
        .with_options(options)
        .prepare_modules_selected_in_workspace(&BTreeSet::from([0]), None, &input)
        .expect("prepare local OCI archive for host execution");
    let options = prepared.options.clone().expect("prepared host options");
    let module = options.workspace_modules.get(&0).expect("input-local OCI module");
    assert!(module.starts_with(input.modules()));
    assert!(module.join("run.sh").is_file());
    assert_eq!(prepared.modules[0].acquisition.as_ref().expect("OCI acquisition evidence").source, source);
    let outcome = prepared.run_workflow(None, &options);
    assert!(outcome.state.is_succeeded());
    workspace.close().expect("close OCI host workspace");
    remove_dir_all(root).expect("remove OCI host fixture");
}
#[test]
fn test_local_source_interpolation_retains_bindings_through_nested_workflow() {
    let root = std::env::temp_dir().join(format!("xylem-nested-source-interpolation-{}", std::process::id()));
    create_dir_all(&root).expect("create fixture");
    let parent = root.join("parent.json");
    let child = root.join("child.json");
    write(
        &parent,
        r#"{"requirements":[],"variables":{"SOURCE":"./parent"},"modules":[{"name":"child","type":"configuration","uri":"file:./child.json"}]}"#,
    )
    .expect("write parent");
    write(
        &child,
        r#"{"requirements":[],"variables":{"SOURCE":"./child"},"modules":[{"name":"leaf","type":"script","uri":"file:{{SOURCE}}"}]}"#,
    )
    .expect("write child");
    let bindings = VariableOverrides::parse(&["SOURCE=./invoked".to_string()]).expect("binding");
    let policy = acquisition_policy(None, false, true);
    let cache = AcquisitionCache::new(root.join("cache"));
    let resolved = Config::read(parent.clone())
        .expect("read parent")
        .resolve_with_policy_and_bindings(&parent, &policy, &cache, true, &bindings)
        .expect("resolve nested leaf");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert!(<&str>::from(uri).ends_with("/invoked"), "{uri}"),
        | other => panic!("expected script: {other:?}"),
    }
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_local_source_interpolation_uses_module_scope_and_invocation_precedence() {
    let root = std::env::temp_dir().join(format!("xylem-local-source-interpolation-{}", std::process::id()));
    create_dir_all(&root).expect("create fixture");
    let config_path = root.join("config.json");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "modules": [{
            "name": "probe", "type": "script", "uri": "file:{{SOURCE}}",
            "variables": {"SOURCE": "./module-default"},
            "template": {"command": "true", "arguments": []}
        }], "variables": {"SOURCE": "./workflow-default"}
    }))
    .expect("source fixture");
    let local = config.clone().resolve(&config_path).expect("resolve module value");
    match &local.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:./module-default"),
        | other => panic!("expected script: {other:?}"),
    }
    let bindings = VariableOverrides::parse(&["SOURCE=./invoked".to_string()]).expect("binding");
    let policy = acquisition_policy(None, false, true);
    let cache = AcquisitionCache::new(root.join("cache"));
    let resolved = config
        .resolve_with_policy_and_bindings(&config_path, &policy, &cache, true, &bindings)
        .expect("resolve invocation source");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:./invoked"),
        | other => panic!("expected script: {other:?}"),
    }
    let invalid: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "modules": [{"name": "broken", "type": "script", "uri": "file:{{MISSING}}"}]
    }))
    .expect("invalid source fixture");
    let error = invalid.resolve(&config_path).expect_err("missing source binding");
    assert!(error.to_string().contains("modules.broken.uri"));
    let selected: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "variables": {"SOURCE": "file:./selected"},
        "modules": [{"name": "selected", "type": "script", "uri": "{{SOURCE}}"}]
    }))
    .expect("selected source fixture");
    assert!(selected.validate().is_ok());
    let resolved = selected.clone().resolve(&config_path).expect("resolve entire URI");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:./selected"),
        | other => panic!("expected script: {other:?}"),
    }
    let remote = VariableOverrides::parse(&["SOURCE=https://example.invalid/repo".to_string()]).expect("remote binding");
    let resolved = selected
        .resolve_with_policy_and_bindings(&config_path, &policy, &cache, true, &remote)
        .expect("resolve remote invocation source");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "https://example.invalid/repo"),
        | other => panic!("expected script: {other:?}"),
    }
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_local_source_interpolation_uses_output_directory_default() {
    let root = std::env::temp_dir().join(format!("xylem-source-output-default-{}", std::process::id()));
    create_dir_all(&root).expect("create fixture");
    let path = root.join("workflow.json");
    let output = root.join("artifacts");
    let config: Config = serde_json::from_value(serde_json::json!({
        "output": output.display().to_string(),
        "requirements": [],
        "modules": [{"name": "probe", "type": "script", "uri": "file:{{OUTPUT_DIRECTORY}}"}]
    }))
    .expect("output-derived source fixture");
    let resolved = config.clone().resolve(&path).expect("resolve output-derived local source");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), format!("file:{}", output.display())),
        | other => panic!("expected script: {other:?}"),
    }
    assert_eq!(
        resolved.modules[0].variables.as_ref().expect("effective variables")["OUTPUT_DIRECTORY"],
        output.display().to_string()
    );
    let mut explicit = config;
    explicit.modules[0].variables = serde_json::json!({"OUTPUT_DIRECTORY": "./explicit"}).as_object().cloned();
    let resolved = explicit.resolve(&path).expect("resolve explicit module output");
    match &resolved.modules[0].module_type {
        | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:./explicit"),
        | other => panic!("expected script: {other:?}"),
    }
    remove_dir_all(root).expect("remove fixture");
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
fn test_module_timeout_validation_is_type_qualified() {
    let zero: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{"name": "probe", "type": "script", "uri": "file:.", "timeout_seconds": 0}]
    }))
    .expect("zero timeout configuration");
    let report = zero.validate().expect_err("zero timeout should fail");
    assert!(report
        .iter()
        .any(|issue| issue.path == "modules[0].timeout_seconds" && issue.error.code == "positive"));
    let wrapper: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{"name": "wrapper", "type": "configuration", "uri": "file:./child.json", "timeout_seconds": 30}]
    }))
    .expect("wrapper timeout configuration");
    let report = wrapper.validate().expect_err("configuration timeout should fail");
    assert!(report
        .iter()
        .any(|issue| issue.path == "modules[0].timeout_seconds" && issue.error.code == "unsupported"));
    assert!(serde_json::from_value::<Config>(serde_json::json!({
        "requirements": [],
        "modules": [{"name": "probe", "type": "script", "uri": "file:.", "timeout_seconds": "later"}]
    }))
    .is_err());
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
fn test_output_interpolation_uses_workflow_values() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "modules": [], "output": "{{ROOT}}/artifacts",
        "variables": {"ROOT": "{{BASE}}", "BASE": "outputs"}
    }))
    .expect("output fixture");
    assert_eq!(config.expanded_output().expect("expand output").as_deref(), Some("outputs/artifacts"));
    let bindings = VariableOverrides::parse(&["BASE=override".to_string()]).expect("binding");
    assert_eq!(
        bindings.apply_root(config).expanded_output().expect("expand override").as_deref(),
        Some("override/artifacts")
    );
    let empty: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "modules": [], "output": "{{EMPTY}}", "variables": {"EMPTY": ""}
    }))
    .expect("empty output fixture");
    assert!(empty.expanded_output().expect_err("reject empty output").to_string().contains("output"));
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
fn test_preparation_keeps_supported_managers_input_local() {
    let root = std::env::temp_dir().join(format!("xylem-input-local-managers-{}", std::process::id()));
    let _ = remove_dir_all(&root);
    let source = root.join("source");
    create_dir_all(&source).expect("create manager module source");
    write(source.join("environment.yml"), "name: isolated\ndependencies: []\n").expect("write conda manifest");
    let modules = ["conda", "mamba", "pixi"]
        .into_iter()
        .enumerate()
        .map(|(index, manager)| {
            serde_json::json!({
                "name": format!("manager-{index}"),
                "programmingLanguage": "python",
                "template": {
                    "arguments": [],
                    "command": "python",
                    "environment": { "manager": manager, "name": "isolated" }
                },
                "type": "script",
                "uri": format!("file:{}", source.display())
            })
        })
        .collect::<Vec<_>>();
    let config: Config = serde_json::from_value(serde_json::json!({ "modules": modules, "requirements": [] })).expect("manager configuration");
    let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("create run workspace");
    let first = workspace.input(0).expect("create first input workspace");
    let second = workspace.input(1).expect("create second input workspace");
    for input in [&first, &second] {
        create_dir_all(input.environments().join("module-000000")).expect("create conda prefix");
        create_dir_all(input.environments().join("module-000001")).expect("create mamba prefix");
    }
    let selected = BTreeSet::from([0, 1, 2]);
    let first_prepared = config
        .clone()
        .prepare_modules_selected_in_workspace(&selected, None, &first)
        .expect("prepare first repeated input");
    let second_prepared = config
        .prepare_modules_selected_in_workspace(&selected, None, &second)
        .expect("prepare second repeated input");
    for (prepared, input) in [(&first_prepared, &first), (&second_prepared, &second)] {
        let options = prepared.options.as_ref().expect("input-local runtime options");
        assert_eq!(options.environment_root.as_deref(), Some(input.environments()));
        for index in 0..3 {
            let path = options.workspace_modules.get(&index).expect("prepared module path");
            assert!(path.starts_with(input.modules()), "{}", path.display());
        }
    }
    assert_ne!(
        first_prepared.options.as_ref().expect("first options").workspace_modules,
        second_prepared.options.as_ref().expect("second options").workspace_modules
    );
    workspace.close().expect("close manager workspace");
    let _ = remove_dir_all(root);
}
#[test]
fn test_preparation_omits_installed_pixi_envs_on_host_and_sandbox() {
    let root = std::env::temp_dir().join(format!("xylem-pixi-materialization-{}", std::process::id()));
    let source = root.join("source");
    create_dir_all(source.join(".pixi/envs/default/lib")).expect("installed environment");
    create_dir_all(source.join("data")).expect("data directory");
    let files = [
        (".gitignore", "data/\n.pixi/\n"),
        (".pixi/config.toml", "run-post-link-scripts = 'insecure'\n"),
        ("data/input.txt", "input data\n"),
        ("pixi.lock", "version: 6\n"),
        ("pixi.toml", "[workspace]\nname = 'probe'\n"),
        ("pyproject.toml", "[project]\nname = 'probe'\n"),
        ("run.py", "print('probe')\n"),
    ];
    for (path, contents) in files {
        write(source.join(path), contents).expect("source file");
    }
    write(source.join(".pixi/envs/default/lib/installed"), "installed package").expect("installed package");
    #[cfg(unix)]
    symlink("missing", source.join(".pixi/envs/default/lib/broken")).expect("broken installed link");
    let config: Config = serde_json::from_value(serde_json::json!({
        "modules": [{
            "name": "probe",
            "programmingLanguage": "generic",
            "template": {"arguments": ["run.py"], "command": "python3"},
            "type": "script",
            "uri": format!("file:{}", source.display())
        }],
        "requirements": []
    }))
    .expect("generic module configuration");
    let workspace = RunWorkspace::create(&root.join("workspaces"), "run").expect("run workspace");
    let host = workspace.input(0).expect("host input");
    let sandbox = workspace.input(1).expect("sandbox input");
    let selected = BTreeSet::from([0]);
    let host_prepared = config
        .clone()
        .prepare_modules_selected_in_workspace(&selected, None, &host)
        .expect("host preparation");
    let sandbox_prepared = config
        .materialize_modules_selected_in_workspace(&selected, None, &sandbox)
        .expect("sandbox materialization");
    for (prepared, input) in [(&host_prepared, &host), (&sandbox_prepared, &sandbox)] {
        let path = &prepared.options.as_ref().expect("prepared options").workspace_modules[&0];
        assert!(path.starts_with(input.modules()));
        assert!(!path.join(".pixi/envs").exists());
        for (relative, contents) in files {
            assert_eq!(read_to_string(path.join(relative)).expect("copied source file"), contents);
        }
        write(path.join("data/input.txt"), "workspace-only edit").expect("modify isolated data");
    }
    assert_eq!(read_to_string(source.join("data/input.txt")).expect("original data"), "input data\n");
    assert!(source.join(".pixi/envs/default/lib/installed").is_file());
    #[cfg(unix)]
    assert!(symlink_metadata(source.join(".pixi/envs/default/lib/broken"))
        .expect("original link")
        .file_type()
        .is_symlink());
    workspace.close().expect("close workspace");
    remove_dir_all(root).expect("cleanup fixture");
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
fn test_selection_roots_expand_across_configuration_formats() {
    let root = std::env::temp_dir().join(format!("xylem-root-formats-{}", std::process::id()));
    create_dir_all(root.join("data")).expect("create input directory");
    write(root.join("data/scene.tif"), "scene").expect("write input");
    let value = serde_json::json!({
        "input": { "select": { "engine": "cel", "options": { "roots": ["{{ DATA_ROOT }}"] }, "script": "items.map(item, item.path)" } },
        "modules": [], "requirements": [], "variables": { "DATA_ROOT": "data" }
    });
    for extension in ["json", "jsonc", "yaml", "yml"] {
        let path = root.join(format!("config.{extension}"));
        let contents = match extension {
            | "json" => serde_json::to_string(&value).expect("serialize JSON"),
            | "jsonc" => format!(
                "// selection root interpolation\n{}",
                serde_json::to_string(&value).expect("serialize JSONC")
            ),
            | _ => serde_norway::to_string(&value).expect("serialize YAML"),
        };
        write(&path, contents).expect("write config");
        let config = Config::try_from(path.as_path()).expect("read config");
        let inputs = config.resolve_inputs(None, &path).expect("expand and discover inputs");
        assert_eq!(
            inputs.values,
            [Some(
                canonicalize(root.join("data/scene.tif")).expect("canonical input").display().to_string()
            )],
            "{extension}"
        );
    }
    remove_dir_all(root).expect("remove fixture");
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
fn test_source_interpolation_accepts_pinned_git_and_oci_wrapper_forms() {
    let root = std::env::temp_dir().join(format!("xylem-source-remote-forms-{}", std::process::id()));
    let cache = AcquisitionCache::new(root.join("cache"));
    let git: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "variables": {"HOST": "example.org"},
        "modules": [{
            "name": "git", "type": "configuration",
            "uri": {"scheme": "https", "uri": "https://{{HOST}}/workflow.git", "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
        }]
    }))
    .expect("parse Git wrapper");
    let policy = acquisition_policy(Some(&["example.org".to_string()]), false, false);
    let error = git
        .resolve_with_policy(Path::new("config.json"), &policy, &cache, true)
        .expect_err("uncached Git source remains unavailable in dry run");
    assert!(matches!(error, ConfigurationModuleError::RemoteDryRun { .. }), "{error}");
    assert!(error.to_string().contains("https://example.org/workflow.git"));
    let archive = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle.tar");
    let oci: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "variables": {
            "SOURCE": format!("oci-layout:{}@sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805", archive.display())
        },
        "modules": [{"name": "oci", "type": "configuration", "uri": "{{SOURCE}}"}]
    }))
    .expect("parse OCI wrapper");
    let error = oci
        .resolve_with_policy(Path::new("config.json"), &policy, &cache, true)
        .expect_err("uncached OCI source remains unavailable in dry run");
    assert!(matches!(error, ConfigurationModuleError::RemoteDryRun { .. }), "{error}");
    assert!(error.to_string().contains("oci-layout:"));
}
#[test]
fn test_source_interpolation_denies_effective_host_and_offline_request() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let root = std::env::temp_dir().join(format!("xylem-source-denied-{}", std::process::id()));
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [], "variables": {"SOURCE": format!("http://{address}/child.json")},
            "modules": [{
                "checksum": format!("sha256:{}", "a".repeat(64)),
                "name": "child", "type": "configuration", "uri": "{{SOURCE}}"
            }]
        }))
        .expect("parse configuration");
        let cache = AcquisitionCache::new(root.join("cache"));
        let denied = acquisition_policy(Some(&["allowed.example".to_string()]), true, false);
        let error = config
            .clone()
            .resolve_with_policy(Path::new("config.json"), &denied, &cache, false)
            .expect_err("final host must be denied");
        assert!(matches!(error, ConfigurationModuleError::Remote { .. }), "{error}");
        let offline = acquisition_policy(Some(&[address.to_string()]), true, true);
        let error = config
            .resolve_with_policy(Path::new("config.json"), &offline, &cache, false)
            .expect_err("offline source must not be fetched");
        assert!(matches!(error, ConfigurationModuleError::Remote { .. }), "{error}");
        listener.set_nonblocking(true).expect("nonblocking server");
        assert_eq!(listener.accept().expect_err("no request expected").kind(), std::io::ErrorKind::WouldBlock);
    }
}
#[test]
fn test_source_interpolation_detects_effective_nested_cycle() {
    let root = std::env::temp_dir().join(format!("xylem-source-cycle-{}", std::process::id()));
    create_dir_all(&root).expect("create fixture");
    let parent = root.join("config.json");
    let child = root.join("child.json");
    write(
        &child,
        r#"{"requirements":[],"modules":[{"name":"again","type":"configuration","uri":"{{CHILD}}"}]}"#,
    )
    .expect("write child");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [], "variables": {"CHILD": "file:./child.json"},
        "modules": [{"name": "outer", "type": "configuration", "uri": "{{CHILD}}"}]
    }))
    .expect("parse configuration");
    let error = config.resolve(&parent).expect_err("effective child source repeats");
    assert!(matches!(error, ConfigurationModuleError::Cycle { .. }), "{error}");
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_source_interpolation_rejects_unknown_before_http_request() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let root = std::env::temp_dir().join(format!("xylem-source-unknown-{}", std::process::id()));
        let config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [], "modules": [{
                "checksum": format!("sha256:{}", "a".repeat(64)),
                "name": "child", "type": "configuration",
                "uri": format!("http://{address}/{{{{MISSING}}}}.json")
            }]
        }))
        .expect("parse configuration");
        assert!(config.validate().is_ok(), "defer URI validation until expansion");
        let policy = acquisition_policy(Some(&[address.to_string()]), true, false);
        let cache = AcquisitionCache::new(root.join("cache"));
        let error = config
            .resolve_with_policy(Path::new("config.json"), &policy, &cache, false)
            .expect_err("unknown binding must fail before acquisition");
        assert!(matches!(error, ConfigurationModuleError::Interpolation(_)), "{error}");
        assert!(error.to_string().contains("modules.child.uri"));
        listener.set_nonblocking(true).expect("nonblocking server");
        assert_eq!(listener.accept().expect_err("no request expected").kind(), std::io::ErrorKind::WouldBlock);
    }
}
#[test]
fn test_source_interpolation_resolves_http_wrapper_and_remote_leaf_with_invocation_precedence() {
    if let Some(listener) = bind_local_server() {
        let address = listener.local_addr().expect("server address");
        let root = std::env::temp_dir().join(format!("xylem-source-http-{}", std::process::id()));
        create_dir_all(&root).expect("create fixture");
        let child = serde_json::json!({
            "requirements": [], "variables": {"HOST": "child.example"},
            "modules": [{
                "name": "leaf", "type": "script",
                "uri": {"scheme": "https", "uri": "https://{{HOST}}/module", "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                "template": {"command": "true", "arguments": []}
            }]
        })
        .to_string();
        let root_config: Config = serde_json::from_value(serde_json::json!({
            "requirements": [],
            "variables": {"SOURCE": format!("http://{address}/child.json"), "HOST": "parent.example"},
            "modules": [{
                "checksum": checksum(&child), "name": "outer", "type": "configuration",
                "uri": "{{SOURCE}}", "variables": {"HOST": "wrapper.example"}
            }]
        }))
        .expect("parse configuration");
        assert!(root_config.validate().is_ok(), "authored source may contain bindings");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept child request");
            let mut request = [0_u8; 1024];
            let count = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET /child.json "));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{child}",
                child.len()
            );
            stream.write_all(response.as_bytes()).expect("write child response");
        });
        let policy = acquisition_policy(Some(&[address.to_string(), "final.example".to_string()]), true, false);
        let cache = AcquisitionCache::new(root.join("cache"));
        let bindings = VariableOverrides::parse(&["HOST=final.example".to_string()]).expect("invocation binding");
        let resolved = root_config
            .resolve_with_policy_and_bindings(Path::new("config.json"), &policy, &cache, false, &bindings)
            .expect("resolve effective sources");
        server.join().expect("server completed");
        assert_eq!(resolved.modules[0].module_path, vec!["outer", "leaf"]);
        match &resolved.modules[0].module_type {
            | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "https://final.example/module"),
            | other => panic!("expected remote script: {other:?}"),
        }
        remove_dir_all(root).expect("remove fixture");
    }
}
#[test]
fn test_structural_interpolation_across_configuration_formats() {
    let root = std::env::temp_dir().join(format!("xylem-structural-formats-{}", std::process::id()));
    create_dir_all(&root).expect("create fixture");
    for extension in ["json", "jsonc", "yaml", "yml"] {
        let path = root.join(format!("workflow.{extension}"));
        let content = match extension {
            | "json" | "jsonc" => r#"{"artifacts":"{{ROOT}}/artifacts","input":["{{ITEM}}"],"requirements":[],"variables":{"ROOT":"out","ITEM":"sample","SOURCE":"."},"modules":[{"name":"probe","type":"script","uri":{"scheme":"file","uri":"file:{{SOURCE}}"}}]}"#,
            | _ => "artifacts: '{{ROOT}}/artifacts'\ninput: ['{{ITEM}}']\nrequirements: []\nvariables: {ROOT: out, ITEM: sample, SOURCE: .}\nmodules:\n  - name: probe\n    type: script\n    uri: {scheme: file, uri: 'file:{{SOURCE}}'}\n",
        };
        write(&path, content).expect("write fixture");
        let config = Config::read(path.clone()).expect("read fixture");
        assert!(
            config.validate().is_ok(),
            "validate templated local source in {extension}: {:?}",
            config.validate()
        );
        assert_eq!(config.expanded_output().expect("expand output").as_deref(), Some("out/artifacts"));
        let resolved = config.resolve(&path).expect("resolve source");
        match &resolved.modules[0].module_type {
            | ModuleType::Script { uri, .. } => assert_eq!(<&str>::from(uri), "file:."),
            | other => panic!("expected script: {other:?}"),
        }
    }
    remove_dir_all(root).expect("remove fixture");
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
fn test_variable_override_validation_and_precedence() {
    let bindings = ["DATASET=first", "EMPTY=", "LABEL=one,two=three", "DATASET=last"].map(str::to_string);
    let overrides = VariableOverrides::parse(&bindings).expect("valid bindings");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "probe", "type": "script", "programmingLanguage": "generic",
            "uri": "file:.", "template": {"command": "true", "arguments": []},
            "variables": {"DATASET": "module", "OTHER": 4}
        }],
        "variables": {"DATASET": "root", "UNCHANGED": true}
    }))
    .expect("fixture");
    let root = overrides.apply_root(config.clone());
    assert_eq!(root.variables.as_ref().expect("root values")["DATASET"], "last");
    assert_eq!(root.modules[0].variables.as_ref().expect("module values")["DATASET"], "module");
    let resolved = overrides.apply_resolved(root);
    assert_eq!(resolved.modules[0].variables.as_ref().expect("module values")["DATASET"], "last");
    assert_eq!(resolved.modules[0].variables.as_ref().expect("module values")["OTHER"], 4);
    assert_eq!(resolved.module_environment(resolved.modules.first(), None)["_XYLEM_DATASET"], "last");
    assert_eq!(resolved.variables.as_ref().expect("root values")["EMPTY"], "");
    assert_eq!(resolved.variables.as_ref().expect("root values")["LABEL"], "one,two=three");
    assert_eq!(resolved.variables.as_ref().expect("root values")["UNCHANGED"], true);
    assert_eq!(VariableOverrides::default().apply_resolved(config.clone()).variables, config.variables);
    for name in ["INPUT", "INPUT_BASENAME", "MODULE_LIST", "OUTPUT_DIRECTORY", "RUN_ID", "WORK_DIRECTORY"] {
        assert!(matches!(
            VariableOverrides::parse(&[format!("{name}=x")]),
            Err(VariableOverrideError::ReservedName(_))
        ));
    }
    for value in ["NO_EQUALS", "=value", "lower=value", "A-B=value", "1A=value"] {
        assert!(VariableOverrides::parse(&[value.to_string()]).is_err(), "{value}");
    }
}
#[test]
fn test_workflow_arguments_bind_in_order_with_named_precedence() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "arguments": ["DATASET", "MODE"], "requirements": [], "modules": [],
        "variables": {"DATASET": "default", "MODE": "full"}
    }))
    .expect("argument fixture");
    let named = VariableOverrides::parse(&["MODE=fast".to_string()]).expect("named binding");
    let bindings = config
        .bind_arguments(&["basin".to_string(), "slow".to_string()], &named)
        .expect("bind arguments");
    let effective = bindings.apply_root(config.clone());
    assert_eq!(effective.variables.as_ref().expect("values")["DATASET"], "basin");
    assert_eq!(effective.variables.as_ref().expect("values")["MODE"], "fast");
    let omitted = config
        .bind_arguments(&["".to_string()], &VariableOverrides::default())
        .expect("empty binding")
        .apply_root(config.clone());
    assert_eq!(omitted.variables.as_ref().expect("values")["DATASET"], "");
    assert_eq!(omitted.variables.as_ref().expect("values")["MODE"], "full");
    assert!(matches!(
        config.bind_arguments(&["one".to_string(), "two".to_string(), "three".to_string()], &named),
        Err(WorkflowArgumentsError::TooManyValues { .. })
    ));
    assert!(config.validate().is_ok());
}
#[test]
fn test_workflow_arguments_declarations_validate_across_formats() {
    let root = std::env::temp_dir().join(format!("xylem-workflow-arguments-{}", std::process::id()));
    create_dir_all(&root).expect("create argument fixture directory");
    for extension in ["json", "jsonc", "yaml", "yml"] {
        let path = root.join(format!("workflow.{extension}"));
        let content = match extension {
            | "json" | "jsonc" => r#"{"arguments":["DATASET","MODE"],"requirements":[],"modules":[]}"#,
            | _ => "arguments: [DATASET, MODE]\nrequirements: []\nmodules: []\n",
        };
        write(&path, content).expect("write workflow argument fixture");
        let config = Config::read(path).expect("read argument fixture");
        assert_eq!(config.arguments, Some(vec!["DATASET".to_string(), "MODE".to_string()]));
        assert!(config.validate().is_ok());
        assert_eq!(
            serde_json::from_str::<Config>(&serde_json::to_string(&config).expect("serialize config"))
                .expect("round trip")
                .arguments,
            config.arguments
        );
    }
    for declarations in [
        serde_json::json!(["DATASET", "DATASET"]),
        serde_json::json!(["lower"]),
        serde_json::json!(["INPUT"]),
        serde_json::json!(["OUTPUT_DIRECTORY"]),
    ] {
        let config: Config = serde_json::from_value(serde_json::json!({"arguments": declarations, "requirements": [], "modules": []}))
            .expect("deserialize declaration");
        assert!(config.validate().is_err());
        assert!(config.bind_arguments(&[], &VariableOverrides::default()).is_err());
    }
    let absent: Config = serde_json::from_value(serde_json::json!({"requirements": [], "modules": []})).expect("absent declaration");
    let empty: Config = serde_json::from_value(serde_json::json!({"arguments": [], "requirements": [], "modules": []})).expect("empty declaration");
    assert!(absent.validate().is_ok());
    assert!(empty.validate().is_ok());
    assert!(matches!(
        absent.bind_arguments(&["extra".to_string()], &VariableOverrides::default()),
        Err(WorkflowArgumentsError::TooManyValues { .. })
    ));
    remove_dir_all(root).expect("remove argument fixtures");
}
#[test]
fn test_workflow_arguments_use_only_invoked_root_declarations() {
    let root = std::env::temp_dir().join(format!("xylem-workflow-argument-root-{}", std::process::id()));
    create_dir_all(&root).expect("create nested argument fixture");
    let parent = root.join("parent.json");
    let child = root.join("child.json");
    write(
        &parent,
        r#"{"arguments":["ROOT"],"requirements":[],"modules":[{"name":"child","type":"configuration","uri":"file:./child.json"}]}"#,
    )
    .expect("write parent");
    write(&child, r#"{"arguments":["CHILD"],"requirements":[],"modules":[]}"#).expect("write child");
    let resolved = Config::read(parent.clone())
        .expect("read parent")
        .resolve(&parent)
        .expect("resolve child");
    assert_eq!(resolved.arguments, Some(vec!["ROOT".to_string()]));
    let bindings = resolved
        .bind_arguments(&["parent".to_string()], &VariableOverrides::default())
        .expect("bind root");
    assert_eq!(bindings.apply_root(resolved).variables.as_ref().expect("variables")["ROOT"], "parent");
    let direct = Config::read(child).expect("read child");
    assert_eq!(direct.arguments, Some(vec!["CHILD".to_string()]));
    remove_dir_all(root).expect("remove nested argument fixture");
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
