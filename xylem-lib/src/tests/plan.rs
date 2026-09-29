use crate::config::{Config, InputAttribute};
use crate::plan::ExecutionPlan;
use crate::script::ContainerStartup;
use std::fs::{create_dir_all, remove_dir_all, write};

#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;

fn config(output: &str) -> Config {
    serde_json::from_value(serde_json::json!({
        "input": ["configured", "configured"],
        "output": output,
        "requirements": [],
        "modules": [{
            "completion": {
                "marker": "{{ OUTPUT_DIRECTORY }}/.xylem/{{ PARENT_DIRECTORY }}.json",
                "outputs": ["{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/*.txt"]
            },
            "name": "probe",
            "programmingLanguage": "generic",
            "template": { "arguments": ["{{ INPUT }}", "{{ OUTPUT_DIRECTORY }}"], "command": "probe" },
            "type": "script",
            "uri": "file:./probe"
        }]
    }))
    .expect("plan fixture")
}

#[test]
fn test_cli_inputs_override_configuration_and_duplicates_keep_first_occurrence() {
    let config = config("/tmp/xylem-plan-output");
    let cli = ["second".to_string(), "first".to_string(), "second".to_string()];
    assert_eq!(
        config.resolve_inputs(Some(&cli)).expect("resolve CLI inputs"),
        [Some("second".to_string()), Some("first".to_string())]
    );
    assert_eq!(
        config.resolve_inputs(None).expect("resolve configured inputs"),
        [Some("configured".to_string())]
    );
}
#[test]
fn test_directory_inputs_are_sorted_deterministically() {
    let root = std::env::temp_dir().join(format!("xylem-plan-inputs-{}", std::process::id()));
    create_dir_all(&root).expect("create input root");
    write(root.join("z"), "z").expect("write z input");
    write(root.join("a"), "a").expect("write a input");
    let mut config = config("/tmp/xylem-plan-output");
    config.input = Some(InputAttribute::Path(root.display().to_string()));
    assert_eq!(
        config.resolve_inputs(None).expect("resolve directory inputs"),
        [Some(root.join("a").display().to_string()), Some(root.join("z").display().to_string())]
    );
    remove_dir_all(root).expect("remove input root");
}
#[test]
fn test_execution_plan_contains_resolved_output_command_and_acquisition() {
    let output = std::env::temp_dir().join(format!("xylem-plan-output-{}", std::process::id()));
    let config = config(&output.display().to_string());
    let inputs = [Some("input-a".to_string())];
    let first = ExecutionPlan::build(&config, "config.yaml".to_string(), &inputs, false).expect("build plan");
    let second = ExecutionPlan::build(&config, "config.yaml".to_string(), &inputs, false).expect("rebuild plan");
    assert_eq!(first, second);
    let yaml = serde_norway::to_string(&config).expect("serialize YAML configuration");
    let yaml: Config = serde_norway::from_str(&yaml).expect("deserialize YAML configuration");
    let yaml_plan = ExecutionPlan::build(&yaml, "config.yaml".to_string(), &inputs, false).expect("build YAML plan");
    assert_eq!(first, yaml_plan);
    assert_eq!(first.configuration.output.as_deref(), Some(output.to_str().expect("UTF-8 output")));
    assert_eq!(first.modules[0].acquisition.action, "use_local");
    assert_eq!(first.modules[0].command.program.as_deref(), Some("probe"));
    assert_eq!(first.modules[0].command.arguments[0], "input-a");
    assert_eq!(
        first.modules[0].variables.get("_XYLEM_OUTPUT_DIRECTORY"),
        Some(&output.display().to_string())
    );
    assert_eq!(
        first.modules[0].completion.as_ref().expect("completion").outputs,
        [output.join("input-a").join("*.txt").display().to_string()]
    );
    assert!(!output.exists());
}
#[test]
fn test_execution_plan_reports_container_startup_strategies() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [
            {
                "name": "explicit",
                "type": "script",
                "uri": "file:.",
                "template": {
                    "arguments": [],
                    "command": "echo",
                    "container": {
                        "command": ["serve"],
                        "create": true,
                        "image": "alpine:3.22",
                        "name": "explicit"
                    }
                }
            },
            {
                "name": "external",
                "type": "script",
                "uri": "file:.",
                "template": {"arguments": [], "command": "echo", "container": {"name": "external"}}
            },
            {
                "name": "image-default",
                "type": "script",
                "uri": "file:.",
                "template": {
                    "arguments": [],
                    "command": "echo",
                    "container": {"create": true, "image": "alpine:3.22", "name": "image-default"}
                }
            },
            {
                "name": "keep-alive",
                "type": "script",
                "uri": "file:.",
                "template": {
                    "arguments": [],
                    "command": "echo",
                    "container": {"create": true, "image": "alpine:3.22", "keep_alive": true, "name": "keep-alive"}
                }
            }
        ]
    }))
    .expect("container startup plan fixture");
    let plan = ExecutionPlan::build(&config, "config.json".to_string(), &[None], false).expect("build container startup plan");
    let startup = |name: &str| {
        plan.containers
            .iter()
            .find(|container| container.name == name)
            .and_then(|container| container.startup)
    };
    assert_eq!(plan.schema_version, 4);
    assert_eq!(startup("explicit"), Some(ContainerStartup::ExplicitCommand));
    assert_eq!(startup("external"), None);
    assert_eq!(startup("image-default"), Some(ContainerStartup::ImageDefault));
    assert_eq!(startup("keep-alive"), Some(ContainerStartup::KeepAlive));
}
#[cfg(target_os = "linux")]
#[test]
fn test_non_unicode_directory_input_is_rejected_with_context() {
    let root = std::env::temp_dir().join(format!("xylem-plan-non-unicode-{}", std::process::id()));
    create_dir_all(&root).expect("create input root");
    write(root.join(OsString::from_vec(vec![b'i', b'n', 0x80])), "input").expect("write non-Unicode input");
    let mut config = config("/tmp/xylem-plan-output");
    config.input = Some(InputAttribute::Path(root.display().to_string()));
    let error = config.resolve_inputs(None).expect_err("non-Unicode input must be rejected");
    assert!(error.to_string().contains("input directory entry is not valid UTF-8"));
    remove_dir_all(root).expect("remove input root");
}
