use crate::acquisition::AcquisitionAction;
use crate::config::{Config, InputAttribute};
use crate::journal::{MemoryEvidenceMode, MemoryFallbackEvidence, RunMemoryEvidence};
use crate::plan::{ExecutionPlan, MemoryPlanMode};
use crate::script::ContainerStartup;
use crate::selection::InputSource;
use acorn_schema::hardware::Memory;
#[cfg(target_os = "linux")]
use std::ffi::OsString;
use std::fs::{create_dir_all, remove_dir_all, write};
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
fn test_cli_inputs_bypass_unused_selection_validation() {
    let mut config = config("/tmp/xylem-plan-output");
    config.input = Some(
        serde_json::from_value(serde_json::json!({
            "select": {
                "engine": "cel",
                "options": {"roots": []},
                "script": ""
            }
        }))
        .expect("selection fixture"),
    );
    let cli = ["explicit".to_string()];
    let resolved = config
        .resolve_inputs(Some(&cli), std::path::Path::new("missing/config.json"))
        .expect("CLI inputs bypass selection");
    assert_eq!(resolved.values, [Some("explicit".to_string())]);
    assert_eq!(resolved.source, InputSource::Cli);
}
#[test]
fn test_cli_inputs_override_configuration_and_duplicates_keep_first_occurrence() {
    let config = config("/tmp/xylem-plan-output");
    let cli = ["second".to_string(), "first".to_string(), "second".to_string()];
    assert_eq!(
        config
            .resolve_inputs(Some(&cli), std::path::Path::new("."))
            .expect("resolve CLI inputs")
            .values,
        [Some("second".to_string()), Some("first".to_string())]
    );
    assert_eq!(
        config
            .resolve_inputs(None, std::path::Path::new("."))
            .expect("resolve configured inputs")
            .values,
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
        config
            .resolve_inputs(None, std::path::Path::new("."))
            .expect("resolve directory inputs")
            .values,
        [Some(root.join("a").display().to_string()), Some(root.join("z").display().to_string())]
    );
    remove_dir_all(root).expect("remove input root");
}
#[test]
fn test_execution_plan_changes_when_selection_inventory_drifts() {
    let root = std::env::temp_dir().join(format!("xylem-plan-selection-drift-{}", std::process::id()));
    let inputs = root.join("inputs");
    create_dir_all(&inputs).expect("create selection root");
    let candidate = inputs.join("a.txt");
    write(&candidate, "a").expect("write candidate");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "input": {
            "select": {
                "engine": "cel",
                "options": {"roots": ["inputs"], "recursive": false},
                "script": "items.map(item, item.path)"
            }
        }
    }))
    .expect("selection configuration");
    let configuration_path = root.join("config.json");
    let first_inputs = config.resolve_inputs(None, &configuration_path).expect("resolve first inventory");
    let first = ExecutionPlan::build_resolved(&config, configuration_path.display().to_string(), &first_inputs, false).expect("build first plan");
    write(&candidate, "changed").expect("change candidate metadata");
    let second_inputs = config.resolve_inputs(None, &configuration_path).expect("resolve second inventory");
    let second = ExecutionPlan::build_resolved(&config, configuration_path.display().to_string(), &second_inputs, false).expect("build second plan");
    let digest = |source: &InputSource| match source {
        | InputSource::Selection { inventory_digest, .. } => inventory_digest.clone(),
        | _ => panic!("selection provenance"),
    };
    assert_ne!(digest(&first.inputs.source), digest(&second.inputs.source));
    assert_ne!(
        serde_json::to_vec(&first).expect("serialize first plan"),
        serde_json::to_vec(&second).expect("serialize second plan")
    );
    remove_dir_all(root).expect("remove selection root");
}
#[test]
fn test_execution_plan_contains_resolved_output_command_and_acquisition() {
    let output = std::env::temp_dir().join(format!("xylem-plan-output-{}", std::process::id()));
    let config = config(&output.display().to_string());
    let inputs = [Some("input-a".to_string())];
    let first = ExecutionPlan::build(&config, "config.yaml".to_string(), &inputs, false).expect("build plan");
    let second = ExecutionPlan::build(&config, "config.yaml".to_string(), &inputs, false).expect("rebuild plan");
    assert_eq!(first, second);
    assert_eq!(first.schema_version, 13);
    assert!(first.memory.is_none());
    assert_eq!(first.sandbox.mode, "off");
    assert_eq!(first.inputs.source, InputSource::List);
    let yaml = serde_norway::to_string(&config).expect("serialize YAML configuration");
    let yaml: Config = serde_norway::from_str(&yaml).expect("deserialize YAML configuration");
    let yaml_plan = ExecutionPlan::build(&yaml, "config.yaml".to_string(), &inputs, false).expect("build YAML plan");
    assert_eq!(first, yaml_plan);
    assert_eq!(first.configuration.output.as_deref(), Some(output.to_str().expect("UTF-8 output")));
    assert_eq!(first.modules[0].acquisition.action, AcquisitionAction::UseLocal);
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
fn test_execution_plan_falls_back_for_the_whole_run_when_one_input_is_out_of_range() {
    let root = std::env::temp_dir().join(format!("xylem-memory-range-plan-{}", std::process::id()));
    create_dir_all(&root).expect("create memory range plan directory");
    let covered = root.join("covered.bin");
    let oversized = root.join("oversized.bin");
    write(&covered, vec![0_u8; 2 * 1024]).expect("write covered memory input");
    write(&oversized, vec![0_u8; 4 * 1024]).expect("write out-of-range memory input");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": {
            "memory": {
                "budget": "8KB",
                "estimator": {
                    "identity": "memory-range-plan-v1",
                    "intercept": "1KB",
                    "maximum_input": "3KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 1_000_000
                }
            }
        }
    }))
    .expect("memory range plan configuration");
    let plan = ExecutionPlan::build(
        &config,
        "config.json".to_string(),
        &[Some(covered.display().to_string()), Some(oversized.display().to_string())],
        false,
    )
    .expect("build memory range plan");
    let memory = plan.memory.expect("planned memory range admission");
    assert_eq!(memory.mode, MemoryPlanMode::FixedFallback);
    assert_eq!(
        memory.inputs[0].reservation.as_ref().and_then(|reservation| reservation.checked_bytes()),
        Some(3 * 1024)
    );
    assert!(memory.inputs[1].fallback.is_some());
    assert_eq!(memory.inputs[1].input_bytes, Some(4 * 1024));
    assert_eq!(memory.reservations(), None);
    remove_dir_all(root).expect("remove memory range plan directory");
}
#[test]
fn test_execution_plan_rejects_a_reservation_larger_than_the_budget() {
    let root = std::env::temp_dir().join(format!("xylem-memory-budget-plan-{}", std::process::id()));
    create_dir_all(&root).expect("create memory budget plan directory");
    let input = root.join("input.bin");
    write(&input, vec![0_u8; 2 * 1024]).expect("write memory budget input");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": {
            "memory": {
                "budget": "2KB",
                "estimator": {
                    "identity": "memory-budget-plan-v1",
                    "intercept": "1KB",
                    "maximum_input": "3KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 1_000_000
                }
            }
        }
    }))
    .expect("memory budget plan configuration");
    let result = ExecutionPlan::build(&config, "config.json".to_string(), &[Some(input.display().to_string())], false);
    remove_dir_all(root).expect("remove memory budget plan directory");
    assert_eq!(
        result.expect_err("oversized reservation must fail").to_string(),
        "input 0 requires 3072 memory bytes, exceeding the 2048-byte budget"
    );
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
    assert_eq!(plan.schema_version, 13);
    assert_eq!(plan.sandbox.mode, "off");
    assert_eq!(startup("explicit"), Some(ContainerStartup::ExplicitCommand));
    assert_eq!(startup("external"), None);
    assert_eq!(startup("image-default"), Some(ContainerStartup::ImageDefault));
    assert_eq!(startup("keep-alive"), Some(ContainerStartup::KeepAlive));
}
#[test]
fn test_execution_plan_reports_gpu_monitoring_without_host_queries() {
    let mut config = config("/tmp/xylem-plan-output");
    config.options = Some(crate::config::ConfigOptions::init().gpu_monitor(true).build());
    let plan = ExecutionPlan::build(&config, "config.json".to_string(), &[Some("input-a".to_string())], false).expect("build GPU monitoring plan");
    let monitoring = plan.gpu_monitoring.expect("planned GPU monitoring");
    assert_eq!(monitoring.interval, *crate::gpu::GPU_SAMPLE_INTERVAL_MS);
    assert_eq!(monitoring.source, "nvidia-smi");
}
#[test]
fn test_execution_plan_reports_validated_memory_fallback() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [
            {
                "name": "First Module",
                "programmingLanguage": "generic",
                "template": { "arguments": [], "command": "true" },
                "type": "script",
                "uri": "file:."
            },
            {
                "name": "Second Module",
                "programmingLanguage": "generic",
                "template": { "arguments": [], "command": "true" },
                "type": "script",
                "uri": "file:."
            }
        ],
        "options": {
            "memory": {
                "budget": "64GB",
                "estimator": {
                    "identity": "custom-modules-v1",
                    "intercept": "16GB",
                    "maximum_input": "1TB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_100_000,
                    "slope_millionths": 0
                }
            }
        }
    }))
    .expect("memory plan configuration");
    let first = ExecutionPlan::build(&config, "config.json".to_string(), &[None], false).expect("build memory plan");
    let second = ExecutionPlan::build(&config, "config.json".to_string(), &[None], false).expect("rebuild memory plan");
    assert_eq!(first, second);
    let memory = first.memory.expect("planned memory intent");
    assert_eq!(memory.budget.checked_bytes(), Some(64 * 1024 * 1024 * 1024));
    assert_eq!(memory.estimator.identity, "custom-modules-v1");
    assert_eq!(memory.mode, MemoryPlanMode::FixedFallback);
    assert_eq!(memory.inputs.len(), 1);
    assert!(memory.inputs[0].fallback.is_some());
    assert_eq!(memory.inputs[0].input_bytes, None);
    assert_eq!(memory.inputs[0].reservation, None);
    let evidence = RunMemoryEvidence::try_from(&memory).expect("durable fallback evidence");
    assert_eq!(evidence.mode, MemoryEvidenceMode::FixedFallback);
    assert_eq!(evidence.inputs[0].fallback, Some(MemoryFallbackEvidence::NoInput));
    let serialized_evidence = serde_json::to_value(&evidence).expect("serialize durable fallback evidence");
    assert!(serialized_evidence["inputs"][0]["estimated"].is_null());
    assert!(serialized_evidence["inputs"][0]["observed_peak"].is_null());
    assert!(serialized_evidence["inputs"][0]["reservation"].is_null());
    assert!(serialized_evidence["inputs"][0]["safety_reserve"].is_null());
    let serialized = serde_json::to_value(second).expect("serialize memory plan");
    assert_eq!(serialized["memory"]["estimator"]["identity"], "custom-modules-v1");
    assert_eq!(serialized["memory"]["estimator"]["slope_millionths"], 0);
    assert_eq!(serialized["memory"]["inputs"][0]["fallback"]["kind"], "no_input");
    assert_eq!(serialized["memory"]["mode"], "fixed_fallback");
    assert_eq!(serialized["schema_version"], 13);
}
#[test]
fn test_execution_plan_uses_memory_aware_reservations() {
    let root = std::env::temp_dir().join(format!("xylem-memory-plan-{}", std::process::id()));
    create_dir_all(&root).expect("create memory plan directory");
    let input = root.join("input.bin");
    write(&input, vec![0_u8; 2 * 1024]).expect("write memory plan input");
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "modules": [],
        "options": {
            "memory": {
                "budget": "4KB",
                "estimator": {
                    "identity": "memory-plan-v1",
                    "intercept": "1KB",
                    "maximum_input": "3KB",
                    "minimum_input": "1KB",
                    "safety_millionths": 1_000_000,
                    "slope_millionths": 1_000_000
                }
            }
        }
    }))
    .expect("memory-aware plan configuration");
    let plan =
        ExecutionPlan::build(&config, "config.json".to_string(), &[Some(input.display().to_string())], false).expect("build memory-aware plan");
    let memory = plan.memory.expect("planned memory admission");
    assert_eq!(memory.mode, MemoryPlanMode::MemoryAware);
    assert_eq!(memory.inputs[0].input_bytes, Some(2 * 1024));
    assert_eq!(
        memory.inputs[0].reservation.as_ref().and_then(|reservation| reservation.checked_bytes()),
        Some(3 * 1024)
    );
    assert_eq!(memory.reservations().expect("active reservations")[0].checked_bytes(), Some(3 * 1024));
    let evidence = RunMemoryEvidence::try_from(&memory).expect("durable memory evidence");
    assert_eq!(evidence.mode, MemoryEvidenceMode::MemoryAware);
    assert_eq!(evidence.inputs[0].estimated.as_ref().and_then(Memory::checked_bytes), Some(3 * 1024));
    assert_eq!(evidence.inputs[0].safety_reserve.as_ref().and_then(Memory::checked_bytes), Some(0));
    assert_eq!(evidence.inputs[0].reservation.as_ref().and_then(Memory::checked_bytes), Some(3 * 1024));
    remove_dir_all(root).expect("remove memory plan directory");
}
#[cfg(target_os = "linux")]
#[test]
fn test_non_unicode_directory_input_is_rejected_with_context() {
    let root = std::env::temp_dir().join(format!("xylem-plan-non-unicode-{}", std::process::id()));
    create_dir_all(&root).expect("create input root");
    write(root.join(OsString::from_vec(vec![b'i', b'n', 0x80])), "input").expect("write non-Unicode input");
    let mut config = config("/tmp/xylem-plan-output");
    config.input = Some(InputAttribute::Path(root.display().to_string()));
    let error = config
        .resolve_inputs(None, std::path::Path::new("."))
        .expect_err("non-Unicode input must be rejected");
    assert!(error.to_string().contains("input directory entry is not valid UTF-8"));
    remove_dir_all(root).expect("remove input root");
}
