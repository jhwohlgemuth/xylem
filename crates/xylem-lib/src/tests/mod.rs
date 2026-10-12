extern crate alloc;

use crate::remote::{redact_url, RemotePolicy};
use crate::script::*;
use crate::*;
use acorn_core::{Location, Scheme};
use alloc::sync::Arc;
use core::time::Duration;
use pyproject_toml::Project;
use std::collections::HashMap;
use std::env::set_var;
use std::fs::{remove_file, write};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

mod completion;
mod error;
mod plan;

const FIXTURES: &str = "../../tests/fixtures";

#[derive(Clone)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture log output").write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// `core::io` is still unstable (rust-lang/rust#154046), so this lint's
// `core::io::ErrorKind` suggestion does not compile.
fn to_string(values: Vec<&str>) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_acorn_location_contract() {
    let remote = Location::from("https://code.ornl.gov/some/project");
    assert_eq!(remote.scheme(), Scheme::HTTPS);
    assert_eq!(remote.hash(), "code_ornl_gov_some_project");

    let root = PathBuf::from("workflow-root");
    assert_eq!(
        module_working_directory("ssh://git@code.ornl.gov/some/project", Some(&root)),
        Some(root.join("code_ornl_gov_some_project").to_string_lossy().to_string())
    );
}
#[test]
fn test_command() {
    let nonexistent: Command = Command::init().name("does-not-exist".to_string()).build();
    assert!(!nonexistent.test());
    let base = VirtualEnvironment::init()
        .name("base".to_string())
        .manager(VirtualEnvironmentManager::Conda)
        .build();
    let executable = std::env::current_exe().expect("test executable should have a path");
    let executable_name = executable.to_string_lossy().into_owned();
    let command = Command::init().name(executable_name.clone()).virtual_environment(base).build();
    assert_eq!(command.name, executable_name);
    assert_eq!(command.virtual_environment.as_ref().map(ToString::to_string), Some("base".to_string()));
    assert!(command.test());
}
#[test]
fn test_config_environment_variables_without_input() {
    let config: Config = serde_json::from_value(serde_json::json!({
        "requirements": [],
        "variables": {
            "GREETING": "Hello",
            "MESSAGE": "{{ GREETING }}, workflow!"
        },
        "modules": [
            {
                "name": "Environment Probe",
                "type": "script",
                "programmingLanguage": "generic",
                "uri": "file:./module",
                "variables": {
                    "GREETING": "Goodbye"
                }
            },
            {
                "name": "Second Probe",
                "type": "script",
                "programmingLanguage": "generic",
                "uri": "file:./module"
            }
        ]
    }))
    .expect("test config should deserialize");
    let module = config.modules[0].clone();
    let variables = config.get_environment_variables(Some(module), None);

    assert_eq!(variables.get("_XYLEM_GREETING"), Some(&"Goodbye".to_string()));
    assert_eq!(variables.get("_XYLEM_MESSAGE"), Some(&"Goodbye, workflow!".to_string()));
    assert_eq!(variables.get("_XYLEM_MODULE_LIST"), Some(&"environment_probe,second_probe".to_string()));
}
#[test]
fn test_config_get_inputs() {
    match Config::read(PathBuf::from(format!("{FIXTURES}/config.json"))) {
        | Ok(config) => {
            assert_eq!(
                config.get_inputs().expect("resolve list inputs"),
                Some(to_string(vec!["input_01", "input_02", "input_03"]))
            );
        }
        | Err(error) => panic!("Cannot read config in get_input test: {error}"),
    }
    match Config::read(PathBuf::from(format!("{FIXTURES}/config_with_path_input.json"))) {
        | Ok(config) => {
            let inputs = config
                .get_inputs()
                .expect("resolve directory inputs")
                .expect("configured inputs")
                .iter()
                .map(|x| PathBuf::from(x).file_stem().unwrap().to_str().unwrap().to_string())
                .collect::<Vec<String>>();
            assert_eq!(inputs, to_string(vec!["one", "three", "two"]));
        }
        | Err(error) => panic!("Cannot read config in get_input test: {error}"),
    }
}
#[test]
fn test_config_get_module_list() {
    let path = format!("{FIXTURES}/config.json");
    match Config::read(PathBuf::from(path)) {
        | Ok(config) => {
            let expected = "local_binary_module,local_python_script_module,remote_python_script_module";
            assert_eq!(config.get_module_list(), expected);
        }
        | Err(error) => panic!("Cannot read config in tests: {error}"),
    }
}
#[test]
fn test_config_read() {
    let path = format!("{FIXTURES}/config.json");
    // Load variables into configuration (JSON or YAML)
    let config = Config::read(PathBuf::from(path)).expect("read JSON configuration");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 3);
    let path = format!("{FIXTURES}/config.yaml");
    let config = Config::read(PathBuf::from(path)).expect("read YAML configuration");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 3);
}
#[test]
fn test_config_read_json() {
    // Load variables into configuration from JSON file
    let path = format!("{FIXTURES}/config.json");
    let config = Config::read_json(PathBuf::from(path)).expect("=> [ERROR] Cannot import config in tests");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 3);
}
#[test]
fn test_config_read_yaml() {
    // Load variables into configuration from YAML file
    let path = format!("{FIXTURES}/config.yaml");
    let config = Config::read_yaml(PathBuf::from(path)).expect("=> [ERROR] Cannot import config in tests");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 3);
}
#[test]
fn test_module() {
    let module = Module {
        acquisition: None,
        acquisition_policy: None,
        completion: None,
        name: "module-a".to_string(),
        module_path: vec!["module-a".to_string()],
        module_type: ModuleType::Configuration {
            checksum: None,
            uri: Location::from("uri"),
        },
        description: None,
        variables: None,
        before_script: None,
        after_script: None,
        template: None,
    };
    assert_eq!(module.to_string(), "module-a");
}
#[test]
fn test_module_language_alias() {
    let module: Module = serde_json::from_value(serde_json::json!({
        "name": "Python Module",
        "type": "script",
        "language": "python",
        "uri": "file:./module"
    }))
    .expect("language alias should deserialize");
    assert!(matches!(
        module.module_type,
        ModuleType::Script {
            programming_language: Some(ModuleLanguage::Python),
            ..
        }
    ));
}
#[test]
fn test_read_pyproject_toml() {
    let path = format!("{FIXTURES}/pyproject.toml");
    match PyProjectToml::read(PathBuf::from(path)) {
        | Ok(pyproject) => {
            match &pyproject.project {
                | Some(Project { name, .. }) => {
                    assert_eq!(name, "xylem-test-fixture");
                }
                | None => panic!("No project in pyproject.toml"),
            }
            assert_eq!(pyproject.get_module_name(), Some("some-module".to_string()));
        }
        | Err(_) => panic!("Cannot read pyproject.toml"),
    }
}
#[test]
fn test_remote_absent_or_empty_allowlist_allows_hosts() {
    assert!(RemotePolicy::from_hosts(None)
        .expect("valid policy")
        .ensure_allowed("ssh://git@example.com/project.git")
        .is_ok());
    assert!(RemotePolicy::from_hosts(Some(&[]))
        .expect("valid policy")
        .ensure_allowed("https://example.org/file")
        .is_ok());
}
#[test]
fn test_remote_allowlist_matches_exact_normalized_authorities() {
    let hosts = vec!["Example.COM.".to_string(), "example.net:8443".to_string()];
    let policy = RemotePolicy::from_hosts(Some(&hosts)).expect("valid policy");
    assert!(policy.ensure_allowed("https://example.com/module").is_ok());
    assert!(policy.ensure_allowed("https://EXAMPLE.com:443/module").is_ok());
    assert!(policy.ensure_allowed("https://example.net:8443/module").is_ok());
    assert!(policy.ensure_allowed("https://sub.example.com/module").is_err());
    assert!(policy.ensure_allowed("https://example.net/module").is_err());
    assert!(policy.ensure_allowed("https://example.com:8443/module").is_err());
}
#[test]
fn test_remote_allowlist_restriction_cannot_expand_parent_policy() {
    let parent_hosts = vec!["example.com".to_string(), "shared.example".to_string()];
    let child_hosts = vec!["shared.example".to_string(), "child.example".to_string()];
    let policy = RemotePolicy::from_hosts(Some(&parent_hosts))
        .expect("valid parent policy")
        .restrict(Some(&child_hosts))
        .expect("valid child policy");
    assert!(policy.ensure_allowed("https://shared.example/module").is_ok());
    assert!(policy.ensure_allowed("https://example.com/module").is_err());
    assert!(policy.ensure_allowed("https://child.example/module").is_err());
}
#[test]
fn test_remote_malformed_allowlist_entries_are_rejected() {
    for entry in [
        "",
        "https://example.com",
        "*.example.com",
        "user@example.com",
        "example.com/path",
        "example.com:",
        ":443",
        "example.com:not-a-port",
    ] {
        assert!(RemotePolicy::from_hosts(Some(&[entry.to_string()])).is_err(), "{entry:?}");
    }
}
#[test]
fn test_remote_redact_url_hides_sensitive_urls() {
    assert_eq!(redact_url("https://user:secret@example.com/file?token=secret#part"), "[REDACTED URL]");
}
#[test]
fn test_script() {
    let script = Script::init().build();
    println!("{script:?}");
    assert_eq!(script.command.name, "".to_string());
    assert_eq!(script.command.virtual_environment, None);
    assert!(script.arguments.is_empty());
    assert_eq!(script.envs, None);
}
#[test]
fn test_script_ensure_relative() {
    let command = Command::init().name("xylem".to_string()).build();
    let script = Script::init().command(command).build();
    assert_eq!(script.command.name, "xylem".to_string());
    let expected = "./xylem".to_string();
    // Should add ./ to command name
    assert_eq!(script.clone().ensure_relative().command.name, expected);
    // Should not add ./ to command name if already relative
    assert_eq!(script.ensure_relative().ensure_relative().command.name, expected);
}
#[test]
fn test_script_expand_arguments() {
    set_var("_XYLEM_INPUT_DIRECTORY", "/path/to/input");
    set_var("_XYLEM_OUTPUT_DIRECTORY", "/path/to/output");
    set_var("_XYLEM_FOO", "one!");
    set_var("_XYLEM_BAR", "two!");
    set_var("_XYLEM_BAZ", "three!");
    let command = Command::init().name("python".to_string()).build();
    let arguments = to_string(vec![
        "-m",
        "lib",
        "--input",
        "{{ INPUT_DIRECTORY }}",
        "--output",
        "{{ OUTPUT_DIRECTORY }}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        script.clone().expand_arguments().arguments,
        vec!["-m", "lib", "--input", "/path/to/input", "--output", "/path/to/output"]
    );
    let environment = VirtualEnvironment::init()
        .name("foo".to_string())
        .manager(VirtualEnvironmentManager::Conda)
        .build();
    assert_eq!(
        script
            .clone()
            .expand_arguments()
            .with_virtual_environment(Some(environment.clone()))
            .to_string(),
        format!("conda run --name {environment} --no-capture-output python -m lib --input /path/to/input --output /path/to/output")
    );
    assert_eq!(
        script.clone().expand_arguments().with_virtual_environment(None).to_string(),
        "python -m lib --input /path/to/input --output /path/to/output"
    );
    let command = Command::init().name("echo".to_string()).build();
    let arguments = to_string(vec![
        "{{ FOO }}",
        "{{ BAR }} and",
        "{{ BAZ }}",
        "{{ FOO }} {{ BAR }} {{ BAZ }}",
        "{{FOO}}/{{BAZ}}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        script.clone().expand_arguments().with_virtual_environment(None).to_string(),
        "echo one! two! and three! one! two! three! one!/three!"
    );
}
#[test]
fn test_script_expand_arguments_from() {
    let command = Command::init().name("python".to_string()).build();
    let arguments = to_string(vec![
        "-m",
        "lib",
        "--input",
        "{{ INPUT_DIRECTORY }}",
        "--output",
        "{{ OUTPUT_DIRECTORY }}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    // Expand from HashMap parameter instead of environment
    let envs = HashMap::from([
        ("INPUT_DIRECTORY".to_string(), "FOO".to_string()),
        ("OUTPUT_DIRECTORY".to_string(), "BAR".to_string()),
    ]);
    let environment = VirtualEnvironment::init()
        .name("foo".to_string())
        .manager(VirtualEnvironmentManager::Conda)
        .build();
    assert_eq!(
        script
            .clone()
            .expand_arguments_from(envs.clone())
            .with_virtual_environment(Some(environment.clone()))
            .to_string(),
        format!("conda run --name {environment} --no-capture-output python -m lib --input FOO --output BAR")
    );
    assert_eq!(
        script
            .clone()
            .expand_arguments_from(envs.clone())
            .with_virtual_environment(None)
            .to_string(),
        "python -m lib --input FOO --output BAR"
    );
    let command = Command::init().name("echo".to_string()).build();
    let arguments = to_string(vec![
        "{{ FOO }}",
        "{{ BAR }} and",
        "{{ BAZ }}",
        "{{ FOO }} {{ BAR }} {{ BAZ }}",
        "{{FOO}}/{{BAZ}}",
    ]);
    let envs = HashMap::from([
        ("FOO".to_string(), "one".to_string()),
        ("BAR".to_string(), "two".to_string()),
        ("BAZ".to_string(), "three".to_string()),
    ]);
    let script = Script::init()
        .command(command)
        .arguments(arguments)
        .build()
        .clone()
        .expand_arguments_from(envs.clone())
        .with_virtual_environment(None)
        .to_string();
    assert_eq!(script, "echo one two and three one two three one/three");
}
#[test]
fn test_script_from_template() {
    // "shell" format arguments
    let path = PathBuf::from(format!("{FIXTURES}/template.json"));
    let script = Script::from_path(path).expect("read shell template");
    let expected_value: Vec<String> = to_string(vec!["-m", "lib", "--input", "{{INPUT_DIRECTORY}}", "--output", "{{OUTPUT_DIRECTORY}}"]);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
    // "exec" format arguments
    let path = PathBuf::from(format!("{FIXTURES}/template_exec.json"));
    let script = Script::from_path(path).expect("read exec template");
    let expected = "conda run --name base --no-capture-output python -m lib --input {{INPUT_DIRECTORY}} --output {{OUTPUT_DIRECTORY}}";
    assert_eq!(format!("{script}"), expected);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
    // Pixi virtual environment manager
    let path = PathBuf::from(format!("{FIXTURES}/template_pixi.json"));
    let script = Script::from_path(path).expect("read Pixi template");
    let expected = "pixi run --environment cuda python -m lib --input {{INPUT_DIRECTORY}} --output {{OUTPUT_DIRECTORY}}";
    assert_eq!(format!("{script}"), expected);
    // Docker + Pixi
    let path = PathBuf::from(format!("{FIXTURES}/template_docker_pixi.json"));
    let script = Script::from_path(path).expect("read Docker Pixi template");
    let expected = "docker exec CONTAINER pixi run --environment ENVIRONMENT python -m lib --method {{METHOD}} --profile {{PROFILE}}";
    assert_eq!(format!("{script}"), expected);
}
#[test]
fn test_script_get_arguments() {
    let command = Command::init().name("echo".to_string()).build();
    let container = ContainerEnvironment::init().name("CONTAINER".to_string()).build();
    let manager = VirtualEnvironmentManager::Conda;
    let virtual_environment = VirtualEnvironment::init().name("base".to_string()).manager(manager).build();
    let arguments = to_string(vec!["hello", "world"]);
    let script = Script::init().command(command).arguments(arguments).build();
    // Docker runtime (default)
    assert_eq!(format!("{script}"), "echo hello world");
    assert_eq!(
        format!("{}", script.clone().with_virtual_environment(Some(virtual_environment.clone()))),
        "conda run --name base --no-capture-output echo hello world"
    );
    assert_eq!(
        format!("{}", script.clone().with_container(container.clone())),
        "docker exec CONTAINER echo hello world"
    );
    let forwarded = script.clone().with_container(container.clone()).with_envs(HashMap::from([
        ("_XYLEM_ZETA".to_string(), "last".to_string()),
        ("_XYLEM_ALPHA".to_string(), "first".to_string()),
    ]));
    assert_eq!(
        forwarded.to_string(),
        "docker exec --env _XYLEM_ALPHA --env _XYLEM_ZETA CONTAINER echo hello world"
    );
    assert_eq!(forwarded.with_container_env(false).to_string(), "docker exec CONTAINER echo hello world");
    assert_eq!(
        format!(
            "{}",
            script
                .clone()
                .with_container(container)
                .with_virtual_environment(Some(virtual_environment))
        ),
        "docker exec CONTAINER conda run --name base --no-capture-output echo hello world"
    );
    // Docker runtime (default)
    let command = Command::init().name("echo".to_string()).build();
    let container = ContainerEnvironment::init()
        .name("CONTAINER".to_string())
        .runtime(ContainerRuntime::Apptainer)
        .build();
    let manager = VirtualEnvironmentManager::Conda;
    let virtual_environment = VirtualEnvironment::init().name("base".to_string()).manager(manager).build();
    let arguments = to_string(vec!["hello", "world"]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        format!("{}", script.clone().with_container(container.clone())),
        "apptainer exec CONTAINER echo hello world"
    );
    assert_eq!(
        format!("{}", script.clone().with_container(container.clone()).with_container_env(false)),
        "apptainer exec --cleanenv CONTAINER echo hello world"
    );
    assert_eq!(
        format!(
            "{}",
            script
                .clone()
                .with_container(container)
                .with_virtual_environment(Some(virtual_environment))
        ),
        "apptainer exec CONTAINER conda run --name base --no-capture-output echo hello world"
    );
    let mamba_environment = VirtualEnvironment::init()
        .name("base".to_string())
        .manager(VirtualEnvironmentManager::Mamba)
        .build();
    assert_eq!(
        format!("{}", script.with_virtual_environment(Some(mamba_environment))),
        "mamba run --name base echo hello world"
    );
}
#[test]
fn test_script_git_clone() {
    let url = "https://code.ornl.gov/some/project".to_string();
    let current_dir = ".".to_string();
    if let Some(script) = Script::maybe_git_clone(url.clone(), current_dir) {
        assert_eq!(
            script.to_string(),
            format!("git -c http.followRedirects=false clone {} code_ornl_gov_some_project", url.clone())
        );
    };
    let ssh = "ssh://git@code.ornl.gov/some/project".to_string();
    let script = Script::maybe_git_clone(ssh.clone(), ".".to_string()).expect("SSH is supported as a Git transport");
    assert_eq!(
        script.to_string(),
        format!("git -c http.followRedirects=false clone {ssh} code_ornl_gov_some_project")
    );
}
#[test]
fn test_script_run() {
    if cfg!(target_os = "windows") {
        // WIP
    } else {
        let command = Command::init().name("echo".to_string()).build();
        let arguments = to_string(vec!["hello", "world"]);
        let script = Script::init().command(command).arguments(arguments).build();
        let output = script.run().expect("echo should run");
        assert_eq!(&output, &vec!["hello world".to_string()]);
        let working_dir = std::env::temp_dir()
            .canonicalize()
            .expect("temporary directory should have a canonical path");
        let expected_working_dir = working_dir.display().to_string();
        let command = Command::init().name("pwd".to_string()).build();
        let script = Script::init().command(command).build();
        let output = script.with_current_dir(expected_working_dir.clone()).run().expect("pwd should run");
        assert_eq!(&output, &vec![expected_working_dir]);
        let path = PathBuf::from(format!("{FIXTURES}/template_run.json"));
        let script = Script::from_path(path).expect("read runnable template");
        assert_eq!(script.run().expect("template should run"), vec!["hello world".to_string()]);
    }
}
#[test]
fn test_script_run_checked_reports_failure() {
    if !cfg!(target_os = "windows") {
        let command = Command::init().name("sh".to_string()).build();
        let arguments = to_string(vec!["-c", "exit 7"]);
        let script = Script::init().command(command).arguments(arguments).build();
        assert!(script.run_checked().is_err());
    }
}
#[test]
fn test_script_run_checked_streams_output_before_exit() {
    if !cfg!(target_os = "windows") {
        let release = std::env::temp_dir().join(format!("xylem-stream-output-{}", std::process::id()));
        let _ = remove_file(&release);
        let command = Command::init().name("sh".to_string()).build();
        let arguments = vec![
            "-c".to_string(),
            "printf 'streamed before exit\\n'; while [ ! -f \"$1\" ]; do sleep 0.01; done".to_string(),
            "sh".to_string(),
            release.display().to_string(),
        ];
        let script = Script::init().command(command).arguments(arguments).build();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = SharedWriter(captured.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_target(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        let observed = captured.clone();
        let release_signal = release.clone();
        let watcher = thread::spawn(move || {
            let streamed = (0..100).any(|_| {
                let found = String::from_utf8_lossy(&observed.lock().expect("read captured log output")).contains("streamed before exit");
                if !found {
                    thread::sleep(Duration::from_millis(10));
                }
                found
            });
            write(release_signal, b"release").expect("release script");
            streamed
        });
        let result = tracing::subscriber::with_default(subscriber, || script.run_checked());
        let streamed = watcher.join().expect("join output watcher");
        let _ = remove_file(release);
        assert!(streamed, "output should be logged while the script is running");
        assert!(result.is_ok());
    }
}
#[test]
fn test_script_run_checked_traces_decoded_output() {
    if !cfg!(target_os = "windows") {
        let command = Command::init().name("sh".to_string()).build();
        let arguments = to_string(vec!["-c", "printf 'snowman ☃\\n'; printf 'check ✔\\n' >&2"]);
        let script = Script::init().command(command).arguments(arguments).build();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = SharedWriter(captured.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_target(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        let output = tracing::subscriber::with_default(subscriber, || script.run_checked().expect("script should run"));
        let logs = String::from_utf8(captured.lock().expect("read captured log output").clone()).expect("log output should be UTF-8");
        assert_eq!(output, vec!["snowman ☃".to_string(), "check ✔".to_string()]);
        assert!(logs.contains("snowman ☃"));
        assert!(logs.contains("check ✔"));
        assert!(!logs.contains("CapturedOutput"));
        assert!(!logs.contains("bytes:"));
        assert!(!logs.contains("ExecutionResult"));
    }
}
#[test]
fn test_string_template_expand() {
    let vars = vec![
        ("GREETING".to_string(), "Hello".to_string()),
        ("LOCATION".to_string(), "World".to_string()),
    ];
    let expected = "Hello World!!!";
    let template = StringTemplate::new("{{ GREETING }} {{ LOCATION }}!!!");
    assert_eq!(template.expand(vars.clone()), expected);
    let template = StringTemplate::new("Hello {{ LOCATION }}!!!");
    assert_eq!(template.expand(vars.clone()), expected);
    let template = StringTemplate::new("Hello World!!!");
    assert_eq!(template.expand(vars.clone()), expected);
    let template = StringTemplate::new("GREETING={{ GREETING }}; UNKNOWN={{ UNKNOWN }}");
    assert_eq!(template.expand(vars), "GREETING=Hello; UNKNOWN={{UNKNOWN}}");
}
#[test]
fn test_string_template_handlebars() {
    // invalid string values
    let not_upper_case = "{{ not_upper_case }}";
    let not_snake_case = "{{ NOT-SNAKE-CASE }}";
    let pascal_case = "{{ PascalCase }}";
    // is_handlebars_template
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{FOO}}")));
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{INPUT_DIRECTORY}}")));
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{ INPUT_DIRECTORY }}")));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(not_upper_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(not_snake_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(pascal_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new("Not a handlebars template")));
    // parse_handlebars_template
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{FOO}}")), "FOO");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{ FOO }}")), "FOO");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{BAR}}")), "BAR");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{  BAR   }}")), "BAR");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("BAZ")), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(not_upper_case)), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(not_snake_case)), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(pascal_case)), "");
    // trim_handlebars_template
    let expected = "{{FOO}}";
    assert_eq!(StringTemplate::new("{{FOO}}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{FOO  }}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{  FOO}}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{  FOO  }}").remove_spaces().value, expected);
    let expected = "FOO {{BAR}} BAZ";
    assert_eq!(StringTemplate::new("FOO {{BAR}} BAZ").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("FOO {{ BAR }} BAZ").remove_spaces().value, expected);
    assert_eq!(
        StringTemplate::new("FOO {{ BAR }} BAZ {{ BIN }}").remove_spaces().value,
        "FOO {{BAR}} BAZ {{BIN}}"
    );
}
#[test]
fn test_to_string() {
    let input = vec!["a", "b", "c"];
    let expected_output = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(to_string(input), expected_output);
}
