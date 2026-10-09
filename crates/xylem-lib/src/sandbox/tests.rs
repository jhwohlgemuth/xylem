use super::*;
use crate::config::ConfigOptions;
use crate::script::Command;
use acorn_cmd::args;
use alloc::sync::Arc;
use std::ffi::OsString;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::process;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeFailure {
    Discover,
    Execute,
    InputFinish,
    Open,
    PrepareModule,
    RunFinish,
    Seal,
    StartInput,
    VerifyRequirements,
}
struct FakePreparation {
    failure: Option<FakeFailure>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    request: SandboxInput<Requested>,
    resources: SandboxResources,
}
struct FakePrepared {
    config: Config,
    failure: Option<FakeFailure>,
    input: Option<String>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    resources: SandboxResources,
}
#[derive(Clone)]
struct FakeProvider {
    acquisitions: Arc<Mutex<Vec<WorkloadAcquisition>>>,
    failure: Option<FakeFailure>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    resources: SandboxResources,
}
struct FakeSession {
    failure: Option<FakeFailure>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    resources: SandboxResources,
}
impl private::Sealed for FakePreparation {}
impl SandboxPreparation for FakePreparation {
    fn prepare_module(&mut self, _module_index: usize) -> Result<(), SandboxError> {
        self.operations.lock().expect("fake operation lock").push("prepare_module");
        match self.failure == Some(FakeFailure::PrepareModule) {
            | true => Err(SandboxError::Runtime("injected prepare module failure".to_string())),
            | false => Ok(()),
        }
    }
    fn seal(self: Box<Self>) -> Result<SandboxInput<Prepared>, SandboxError> {
        self.operations.lock().expect("fake operation lock").push("seal");
        match self.failure == Some(FakeFailure::Seal) {
            | true => Err(SandboxError::Runtime("injected seal failure".to_string())),
            | false => Ok(SandboxInput::from_backend(Arc::new(FakePrepared {
                config: self.request.config().clone(),
                failure: self.failure,
                input: self.request.input().map(str::to_string),
                operations: Arc::clone(&self.operations),
                resources: self.resources,
            }))),
        }
    }
    fn verify_requirements(&mut self) -> Result<(), SandboxError> {
        self.operations.lock().expect("fake operation lock").push("verify_requirements");
        match self.failure == Some(FakeFailure::VerifyRequirements) {
            | true => Err(SandboxError::Runtime("injected requirements failure".to_string())),
            | false => Ok(()),
        }
    }
}
impl PreparedSandboxBackend for FakePrepared {
    fn execute(&self, script: &Script, _module_index: usize, options: &ExecutionOptions) -> Result<ExecutionResult, ProcessError> {
        self.operations.lock().expect("fake operation lock").push("execute");
        match self.failure == Some(FakeFailure::Execute) {
            | true => Err(ProcessError::Spawn {
                script: script.to_string(),
                working_directory: PathBuf::from(&script.current_dir),
                source: std::io::Error::other("injected execute failure"),
            }),
            | false => script.execute_checked(options),
        }
    }
    fn finish(&self, _succeeded: bool) -> Result<(), SandboxError> {
        self.operations.lock().expect("fake operation lock").push("input_finish");
        match self.failure == Some(FakeFailure::InputFinish) {
            | true => Err(SandboxError::Runtime("injected input finish failure".to_string())),
            | false => Ok(()),
        }
    }
    fn prepared_config(&self) -> &Config {
        &self.config
    }
    fn prepared_input(&self) -> Option<&str> {
        self.input.as_deref()
    }
    fn resources(&self) -> &SandboxResources {
        &self.resources
    }
}
impl private::Sealed for FakeProvider {}
impl SandboxProvider for FakeProvider {
    fn discover(&self) -> Result<SandboxCapabilities, SandboxError> {
        self.operations.lock().expect("fake operation lock").push("discover");
        match self.failure == Some(FakeFailure::Discover) {
            | true => Err(SandboxError::Runtime("injected discover failure".to_string())),
            | false => Ok(SandboxCapabilities {
                isolation: IsolationClass::SharedKernel,
                network_deny: true,
                provider: SandboxProviderKind::Oci,
                read_only_root: true,
                resource_limits: true,
                rootless: true,
            }),
        }
    }
    fn open(&self, _run_id: &str, acquisition: WorkloadAcquisition) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        self.operations.lock().expect("fake operation lock").push("open");
        self.acquisitions.lock().expect("fake acquisition lock").push(acquisition);
        match self.failure == Some(FakeFailure::Open) {
            | true => Err(SandboxError::Runtime("injected open failure".to_string())),
            | false => Ok(Arc::new(FakeSession {
                failure: self.failure,
                operations: Arc::clone(&self.operations),
                resources: self.resources.clone(),
            })),
        }
    }
}
impl private::Sealed for FakeSession {}
impl SandboxSession for FakeSession {
    fn start_input(&self, request: SandboxInput<Requested>) -> Result<Box<dyn SandboxPreparation>, SandboxError> {
        self.operations.lock().expect("fake operation lock").push("start_input");
        match self.failure == Some(FakeFailure::StartInput) {
            | true => Err(SandboxError::Runtime("injected start input failure".to_string())),
            | false => Ok(Box::new(FakePreparation {
                failure: self.failure,
                operations: Arc::clone(&self.operations),
                request,
                resources: self.resources.clone(),
            })),
        }
    }
    fn finish(&self, _succeeded: bool) -> Result<(), SandboxError> {
        self.operations.lock().expect("fake operation lock").push("run_finish");
        match self.failure == Some(FakeFailure::RunFinish) {
            | true => Err(SandboxError::Runtime("injected finish failure".to_string())),
            | false => Ok(()),
        }
    }
    fn resources(&self) -> &SandboxResources {
        &self.resources
    }
}
fn invocation(policy: &EffectiveSandboxPolicy, script: &Script, container_name: &str) -> Result<Vec<OsString>, SandboxError> {
    script.validate().map_err(|why| SandboxError::Validation(why.to_string())).and_then(|()| {
        validate_granted_path(
            Path::new(&script.current_dir),
            &policy.policy.paths.module_roots,
            "module working directory",
        )
        .and_then(|module_source| {
            let input = script.envs.as_ref().and_then(|envs| envs.get("_XYLEM_INPUT"));
            input
                .map(|value| validate_granted_path(Path::new(value), &policy.policy.paths.input_roots, "input"))
                .transpose()
                .and_then(|input_source| {
                    input_source
                        .as_ref()
                        .map_or(Ok(()), |input_source| validate_mount_separation(&module_source, input_source))
                        .map(|()| (module_source, input.map(PathBuf::from), input_source))
                })
        })
        .map(|(module_source, input, input_source)| {
            let mut replacements = [input.as_deref(), input_source.as_deref()]
                .into_iter()
                .flatten()
                .map(|path| (path.display().to_string(), "/xylem/input/current".to_string()))
                .collect::<Vec<_>>();
            replacements.extend(
                [Path::new(&script.current_dir), &module_source]
                    .into_iter()
                    .map(|path| (path.display().to_string(), "/xylem/modules/current".to_string())),
            );
            replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| left.0.cmp(&right.0)));
            replacements.dedup();
            let command = rewrite_path(&script.command.name, &replacements);
            let arguments = script.arguments.iter().map(|argument| rewrite_path(argument, &replacements));
            let environment = script
                .envs
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|(name, _)| policy.policy.environment.allow.contains(name))
                .map(|(name, value)| format!("{name}={}", rewrite_path(&value, &replacements)))
                .collect::<Vec<_>>();
            let resources = &policy.policy.resources;
            let cpus = format!("{:.3}", f64::from(resources.cpu_millis) / 1000.0);
            let module_mount = format!("type=bind,source={},target=/xylem/modules/current,readonly", module_source.display());
            let input_mount = input_source
                .as_ref()
                .map(|source| format!("type=bind,source={},target=/xylem/input/current,readonly", source.display()));
            args![
                "run",
                "--init",
                ("--name", container_name),
                ("--label", "gov.ornl.xylem.sandbox=true"),
                ("--label", format!("gov.ornl.xylem.policy={}", policy.digest)),
                "--pull=never",
                "--network=none",
                "--read-only",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--ipc=private",
                ("--pids-limit", resources.pids.to_string()),
                ("--memory", resources.memory_bytes.to_string()),
                ("--memory-swap", resources.memory_bytes.to_string()),
                ("--cpus", cpus),
                "--user=65532:65532",
                ("--tmpfs", format!("/tmp:rw,noexec,nosuid,nodev,size={}", resources.scratch_bytes)),
                ("--mount", module_mount),
                ..input_mount.map(|mount| args!["--mount", mount]).unwrap_or_default(),
                ..environment.into_iter().flat_map(|value| args!["--env", value]),
                "--workdir=/xylem/modules/current",
                &policy.workload,
                command,
                ..arguments
            ]
        })
    })
}
fn policy(format: &str) -> String {
    match format {
        | "json" | "jsonc" => r#"{
  "environment": { "allow": ["PATH", "LANG"] },
  "lifecycle": { "retention": "delete", "stale_after_seconds": 3600 },
  "minimum_isolation": "shared_kernel",
  "paths": { "artifact_roots": ["artifacts"], "input_roots": ["inputs"], "module_roots": ["modules"] },
  "preparation_network": "deny",
  "process_guards": [],
  "provider": { "type": "oci", "runtime": "docker" },
  "resources": {
    "artifact_bytes": 1048576,
    "artifact_files": 100,
    "captured_output_bytes": 65536,
    "cpu_millis": 1000,
    "memory_bytes": 268435456,
    "pids": 64,
    "scratch_bytes": 1048576,
    "wall_time_seconds": 60
  },
  "version": 1,
  "workload": {
    "allowed": ["example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    "default": "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  }
}"#
        .to_string(),
        | "yaml" | "yml" => r#"environment:
  allow: [PATH, LANG]
lifecycle:
  retention: delete
  stale_after_seconds: 3600
minimum_isolation: shared_kernel
paths:
  artifact_roots: [artifacts]
  input_roots: [inputs]
  module_roots: [modules]
preparation_network: deny
process_guards: []
provider:
  runtime: docker
  type: oci
resources:
  artifact_bytes: 1048576
  artifact_files: 100
  captured_output_bytes: 65536
  cpu_millis: 1000
  memory_bytes: 268435456
  pids: 64
  scratch_bytes: 1048576
  wall_time_seconds: 60
version: 1
workload:
  allowed:
    - example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  default: example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
"#
        .to_string(),
        | _ => String::new(),
    }
}
fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-sandbox-policy-{label}-{}", process::id()))
}
fn workload_inspection() -> OciImageInspection {
    OciImageInspection {
        architecture: "amd64".to_string(),
        config: OciImageConfig {
            entrypoint: None,
            environment: Some(vec!["PATH=/usr/bin".to_string()]),
        },
        digest: Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
        operating_system: "linux".to_string(),
        repository_digests: Vec::new(),
    }
}

#[test]
fn fake_provider_injects_each_lifecycle_failure() {
    [
        FakeFailure::Discover,
        FakeFailure::Execute,
        FakeFailure::InputFinish,
        FakeFailure::Open,
        FakeFailure::PrepareModule,
        FakeFailure::RunFinish,
        FakeFailure::Seal,
        FakeFailure::StartInput,
        FakeFailure::VerifyRequirements,
    ]
    .into_iter()
    .for_each(|failure| {
        let acquisitions = Arc::new(Mutex::new(Vec::new()));
        let operations = Arc::new(Mutex::new(Vec::new()));
        let provider = FakeProvider {
            acquisitions: Arc::clone(&acquisitions),
            failure: Some(failure),
            operations: Arc::clone(&operations),
            resources: SandboxResources {
                artifact_bytes: 1,
                artifact_files: 1,
                captured_output_bytes: 1,
                cpu_millis: 1,
                memory_bytes: 1,
                pids: 1,
                scratch_bytes: 1,
                wall_time_seconds: 1,
            },
        };
        let discovered = provider.discover();
        if failure == FakeFailure::Discover {
            assert!(discovered.is_err());
        } else {
            assert!(discovered.is_ok());
            let session = provider.open("run", WorkloadAcquisition::LocalOnly);
            if failure == FakeFailure::Open {
                assert!(session.is_err());
            } else {
                let session = session.expect("open fake session");
                let config =
                    serde_json::from_str::<Config>(r#"{"requirements":[],"modules":[{"name":"fake","type":"script","uri":"file:/tmp/fake"}]}"#)
                        .expect("parse fake sandbox config");
                let request = SandboxInput::new(config, 0, [0], root("fake-staging"), None).expect("build fake sandbox request");
                let script = Script::init().command(Command::init().name("true".to_string()).build()).build();
                let input_result = request.prepare(session.as_ref(), |_, _| ()).map(|prepared| {
                    assert_eq!(
                        prepared.execute(&script, 0, &ExecutionOptions::new()).is_err(),
                        failure == FakeFailure::Execute
                    );
                    assert_eq!(prepared.observed_memory_peak().expect("read optional peak"), None);
                    assert_eq!(prepared.finish(false).is_err(), failure == FakeFailure::InputFinish);
                });
                assert_eq!(
                    input_result.is_err(),
                    matches!(
                        failure,
                        FakeFailure::PrepareModule | FakeFailure::Seal | FakeFailure::StartInput | FakeFailure::VerifyRequirements
                    )
                );
                assert_eq!(session.finish(false).is_err(), failure == FakeFailure::RunFinish);
            }
        }
        let recorded = operations.lock().expect("fake operation lock").clone();
        assert_eq!(recorded.first(), Some(&"discover"));
        assert!(recorded.contains(&match failure {
            | FakeFailure::Discover => "discover",
            | FakeFailure::Execute => "execute",
            | FakeFailure::InputFinish => "input_finish",
            | FakeFailure::Open => "open",
            | FakeFailure::PrepareModule => "prepare_module",
            | FakeFailure::RunFinish => "run_finish",
            | FakeFailure::Seal => "seal",
            | FakeFailure::StartInput => "start_input",
            | FakeFailure::VerifyRequirements => "verify_requirements",
        }));
        let acquisitions = acquisitions.lock().expect("fake acquisition lock").clone();
        assert_eq!(
            acquisitions,
            (failure != FakeFailure::Discover)
                .then_some(WorkloadAcquisition::LocalOnly)
                .into_iter()
                .collect::<Vec<_>>()
        );
    });
}
#[test]
fn isolation_classes_are_ordered_by_strength() {
    assert!(IsolationClass::Process < IsolationClass::SharedKernel);
    assert!(IsolationClass::SharedKernel < IsolationClass::SeparateKernel);
}
#[test]
fn invocation_is_hardened_and_preserves_argument_boundaries() {
    let directory = root("invocation");
    let modules = directory.join("modules");
    let inputs = directory.join("inputs");
    create_dir_all(&modules).expect("create module fixture directory");
    create_dir_all(&inputs).expect("create input fixture directory");
    let input = inputs.join("sample data.txt");
    write(&input, "sample").expect("write input fixture");
    let policy_path = directory.join("policy.json");
    write(
        &policy_path,
        policy("json").replace("[\"PATH\", \"LANG\"]", "[\"PATH\", \"LANG\", \"_XYLEM_INPUT\"]"),
    )
    .expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&policy_path).expect("parse policy fixture");
    let script = Script::init()
        .command(Command::init().name("printf".to_string()).build())
        .arguments(["%s", input.to_str().expect("UTF-8 fixture path"), "; touch /host"])
        .current_dir(modules.to_str().expect("UTF-8 fixture path").to_string())
        .envs(std::collections::HashMap::from([(
            "_XYLEM_INPUT".to_string(),
            input.to_str().expect("UTF-8 fixture path").to_string(),
        )]))
        .build();
    let arguments = invocation(&policy, &script, "xylem-run-0-0")
        .expect("build OCI invocation")
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.contains(&"--network=none".to_string()));
    assert!(!arguments.contains(&"--rm".to_string()));
    assert!(arguments.contains(&"--read-only".to_string()));
    assert!(arguments.contains(&"--cap-drop=ALL".to_string()));
    assert!(arguments.contains(&"--security-opt=no-new-privileges".to_string()));
    assert!(arguments.contains(&"/xylem/input/current".to_string()), "{arguments:?}");
    assert!(arguments.contains(&"; touch /host".to_string()));
    assert_eq!(arguments.last().map(String::as_str), Some("; touch /host"));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[cfg(unix)]
#[test]
fn invocation_rejects_symlinks_inside_mounted_trees() {
    use std::os::unix::fs::symlink;

    let directory = root("symlink");
    let modules = directory.join("modules");
    create_dir_all(&modules).expect("create module fixture directory");
    symlink("/etc/passwd", modules.join("escape")).expect("create module symlink fixture");
    let policy_path = directory.join("policy.json");
    write(&policy_path, policy("json")).expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&policy_path).expect("parse policy fixture");
    let script = Script::init()
        .command(Command::init().name("true".to_string()).build())
        .current_dir(modules.to_str().expect("UTF-8 fixture path").to_string())
        .build();
    let error = invocation(&policy, &script, "xylem-run-0-0").expect_err("symlinked mount must fail");
    assert!(error.to_string().contains("is a symlink"));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_formats_produce_the_same_canonical_digest() {
    let directory = root("formats");
    create_dir_all(&directory).expect("create policy fixture directory");
    let digests = ["json", "jsonc", "yaml", "yml"]
        .into_iter()
        .map(|extension| {
            let path = directory.join(format!("policy.{extension}"));
            write(&path, policy(extension)).expect("write policy fixture");
            EffectiveSandboxPolicy::read(&path).expect("parse policy fixture").digest
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(digests.len(), 1);
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_rejects_legacy_flat_shape() {
    let directory = root("legacy");
    create_dir_all(&directory).expect("create policy fixture directory");
    let path = directory.join("legacy.json");
    write(
        &path,
        r#"{
  "allowed_images": ["example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
  "backend": "docker",
  "default_image": "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "version": 1
}"#,
    )
    .expect("write legacy policy fixture");
    assert!(EffectiveSandboxPolicy::read(&path).is_err());
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_rejects_process_guards_until_supported() {
    let directory = root("guard");
    create_dir_all(&directory).expect("create policy fixture directory");
    let path = directory.join("guard.json");
    ["bubblewrap", "nono", "nsjail"].into_iter().for_each(|guard| {
        write(
            &path,
            policy("json").replace("\"process_guards\": []", &format!("\"process_guards\": [\"{guard}\"]")),
        )
        .expect("write guarded policy fixture");
        let error = EffectiveSandboxPolicy::read(&path).expect_err("unimplemented process guard must fail");
        assert!(error.to_string().contains("reserved for a later provider integration phase"));
    });
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn policy_rejects_unknown_fields_and_mutable_workloads() {
    let directory = root("invalid");
    create_dir_all(&directory).expect("create policy fixture directory");
    let unknown = directory.join("unknown.json");
    write(
        &unknown,
        policy("json").replacen("\"version\": 1", "\"unknown\": true, \"version\": 1", 1),
    )
    .expect("write unknown-field fixture");
    assert!(EffectiveSandboxPolicy::read(&unknown).is_err());
    let mutable = directory.join("mutable.json");
    write(
        &mutable,
        policy("json").replace(
            "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "example.invalid/xylem:latest",
        ),
    )
    .expect("write mutable-workload fixture");
    assert!(EffectiveSandboxPolicy::read(&mutable).is_err());
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn sandbox_feature_matrix_accepts_requirements_binary_and_python_modules() {
    let config = serde_json::from_str::<Config>(
        r#"{
  "requirements": ["python", "tar"],
  "modules": [
    {"name":"binary","type":"binary","uri":"https://example.invalid/module","checksum":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
    {"name":"generic","type":"script","uri":"file:/tmp/generic"},
    {"name":"python","type":"script","language":"python","uri":"https://example.invalid/python"}
  ],
  "sandbox": true
}"#,
    )
    .expect("parse supported sandbox configuration");
    assert!(config.validate_sandbox().is_ok());
}
#[test]
fn sandbox_feature_matrix_rejects_ssh_and_nested_configurations() {
    let config = serde_json::from_str::<Config>(
        r#"{
  "requirements": [],
  "modules": [
    {"name":"nested","type":"configuration","uri":"file:/tmp/nested.json"},
    {"name":"ssh","type":"script","uri":"ssh://git@example.invalid/module"}
  ],
  "sandbox": true
}"#,
    )
    .expect("parse unsupported sandbox configuration");
    let report = config.validate_sandbox().expect_err("unsupported sandbox features must fail").to_string();
    assert!(report.contains("nested configuration"));
    assert!(report.contains("local or HTTP(S)"));
}
#[test]
fn sandbox_input_request_requires_sorted_unique_module_indices() {
    let config = serde_json::from_str::<Config>(
        r#"{"requirements":[],"modules":[{"name":"one","type":"script","uri":"file:/tmp/one"},{"name":"two","type":"script","uri":"file:/tmp/two"}]}"#,
    )
    .expect("parse sandbox request config");
    assert!(SandboxInput::new(config.clone(), 0, [0, 1], root("request"), None).is_ok());
    assert!(SandboxInput::new(config.clone(), 0, [1, 0], root("request"), None).is_err());
    assert!(SandboxInput::new(config, 0, [0, 0], root("request"), None).is_err());
}
#[test]
fn sandbox_session_open_respects_optional_policy() {
    assert!(<dyn SandboxSession>::open("run", None, WorkloadAcquisition::LocalOnly)
        .expect("open host mode")
        .is_none());
}
#[test]
fn validate_workflow_accepts_managed_run_root() {
    let directory = root("managed-run-root");
    let modules = directory.join("modules");
    create_dir_all(&modules).expect("create module root");
    let policy_path = directory.join("policy.json");
    write(&policy_path, policy("json")).expect("write sandbox policy");
    let policy = EffectiveSandboxPolicy::read(&policy_path).expect("read sandbox policy");
    let config = serde_json::from_value::<Config>(serde_json::json!({
        "requirements": [],
        "modules": [{
            "name": "managed-root",
            "type": "script",
            "uri": format!("file:{}", modules.display()),
            "template": { "command": "true", "arguments": [] }
        }],
        "sandbox": true
    }))
    .expect("parse sandbox configuration");
    let options = ConfigOptions::init().root(directory.clone()).build();
    let config = config.with_options(options);
    assert!(config.validate().is_err());
    assert!(policy.validate_workflow(&config, &[]).is_ok());
    remove_dir_all(directory).expect("remove sandbox fixture");
}
#[test]
fn workload_acquisition_is_local_first_and_fail_closed() {
    let workload = "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let local = workload_inspection();
    assert_eq!(
        acquire_workload(workload, WorkloadAcquisition::PullIfMissing, Ok(Some(local.clone())), || {
            panic!("a local workload must not be pulled")
        })
        .expect("reuse local workload"),
        local
    );
    assert!(acquire_workload(workload, WorkloadAcquisition::LocalOnly, Ok(None), || {
        panic!("offline mode must not pull")
    })
    .is_err());
    let pulled = workload_inspection();
    assert_eq!(
        acquire_workload(workload, WorkloadAcquisition::PullIfMissing, Ok(None), || Ok(Some(pulled.clone()))).expect("use pulled workload"),
        pulled
    );
    assert!(acquire_workload(workload, WorkloadAcquisition::PullIfMissing, Ok(None), || Ok(None)).is_err());
}
#[test]
fn workload_bootstrap_probe_is_digest_pinned_and_hardened() {
    let directory = root("workload-invocations");
    create_dir_all(&directory).expect("create policy fixture directory");
    let path = directory.join("policy.json");
    write(&path, policy("json")).expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&path).expect("parse policy fixture");
    let workload = "example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let probe = policy
        .probe_invocation("run")
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(probe.contains(&"--pull=never".to_string()));
    assert!(probe.contains(&"--network=none".to_string()));
    assert!(probe.contains(&"--read-only".to_string()));
    assert!(probe.contains(&"--user=65532:65532".to_string()));
    assert!(probe.contains(&workload.to_string()));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
#[test]
fn workload_metadata_must_match_the_provider_contract() {
    let directory = root("workload-metadata");
    create_dir_all(&directory).expect("create policy fixture directory");
    let path = directory.join("policy.json");
    write(&path, policy("json")).expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&path).expect("parse policy fixture");
    assert!(policy.verify_workload(&workload_inspection()).is_ok());
    let invalid = [
        OciImageInspection {
            digest: Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
            ..workload_inspection()
        },
        OciImageInspection {
            architecture: "arm64".to_string(),
            ..workload_inspection()
        },
        OciImageInspection {
            operating_system: "windows".to_string(),
            ..workload_inspection()
        },
        OciImageInspection {
            config: OciImageConfig {
                entrypoint: Some(vec!["/bin/sh".to_string()]),
                ..workload_inspection().config
            },
            ..workload_inspection()
        },
        OciImageInspection {
            config: OciImageConfig {
                environment: Some(vec!["SECRET=value".to_string()]),
                ..workload_inspection().config
            },
            ..workload_inspection()
        },
    ];
    invalid
        .into_iter()
        .for_each(|inspection| assert!(policy.verify_workload(&inspection).is_err()));
    remove_dir_all(directory).expect("remove policy fixture directory");
}
