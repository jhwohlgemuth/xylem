use super::*;
use crate::script::Command;
use alloc::sync::Arc;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::process;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeFailure {
    Discover,
    Execute,
    Finish,
    Open,
}
#[derive(Clone)]
struct FakeProvider {
    failure: Option<FakeFailure>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    resources: SandboxResources,
}
struct FakeSession {
    failure: Option<FakeFailure>,
    operations: Arc<Mutex<Vec<&'static str>>>,
    resources: SandboxResources,
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
    fn open(&self, _run_id: &str) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        self.operations.lock().expect("fake operation lock").push("open");
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
    fn execute(
        &self,
        script: &Script,
        _input_index: usize,
        _module_index: usize,
        _options: &ExecutionOptions,
    ) -> Result<ExecutionResult, ProcessError> {
        self.operations.lock().expect("fake operation lock").push("execute");
        let reason = match self.failure == Some(FakeFailure::Execute) {
            | true => "injected execute failure",
            | false => "fake provider does not spawn processes",
        };
        Err(ProcessError::Spawn {
            script: script.to_string(),
            working_directory: PathBuf::from(&script.current_dir),
            source: std::io::Error::other(reason),
        })
    }
    fn finish(&self, _succeeded: bool) -> Result<(), SandboxError> {
        self.operations.lock().expect("fake operation lock").push("finish");
        match self.failure == Some(FakeFailure::Finish) {
            | true => Err(SandboxError::Runtime("injected finish failure".to_string())),
            | false => Ok(()),
        }
    }
    fn resources(&self) -> &SandboxResources {
        &self.resources
    }
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

#[test]
fn fake_provider_injects_each_lifecycle_failure() {
    [FakeFailure::Discover, FakeFailure::Open, FakeFailure::Execute, FakeFailure::Finish]
        .into_iter()
        .for_each(|failure| {
            let operations = Arc::new(Mutex::new(Vec::new()));
            let provider = FakeProvider {
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
                let session = provider.open("run");
                if failure == FakeFailure::Open {
                    assert!(session.is_err());
                } else {
                    let session = session.expect("open fake session");
                    let script = Script::init().command(Command::init().name("true".to_string()).build()).build();
                    let executed = session.execute(&script, 0, 0, &ExecutionOptions::new());
                    assert!(executed.is_err());
                    assert_eq!(session.finish(false).is_err(), failure == FakeFailure::Finish);
                }
            }
            let recorded = operations.lock().expect("fake operation lock").clone();
            let expected = match failure {
                | FakeFailure::Discover => vec!["discover"],
                | FakeFailure::Open => vec!["discover", "open"],
                | FakeFailure::Execute | FakeFailure::Finish => vec!["discover", "open", "execute", "finish"],
            };
            assert_eq!(recorded, expected);
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
    let arguments = policy
        .invocation(&script, "xylem-run-0-0")
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
    let error = policy.invocation(&script, "xylem-run-0-0").expect_err("symlinked mount must fail");
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
fn sandbox_session_open_respects_optional_policy() {
    assert!(<dyn SandboxSession>::open("run", None).expect("open host mode").is_none());
    let directory = root("session");
    create_dir_all(&directory).expect("create policy fixture directory");
    let path = directory.join("policy.json");
    write(&path, policy("json")).expect("write policy fixture");
    let policy = EffectiveSandboxPolicy::read(&path).expect("parse policy fixture");
    assert!(<dyn SandboxSession>::open("run", Some(&policy)).expect("open sandbox session").is_some());
    remove_dir_all(directory).expect("remove policy fixture directory");
}
