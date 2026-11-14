#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
#[cfg(unix)]
use crate::error::ProcessError;
use crate::error::{ModuleLoadError, TemplateError};
use crate::script::{
    add_dot_slash, get_conda_environment_list, get_conda_environment_name, get_mamba_environment_list, Command, ContainerEnvironment,
    ContainerStartup, Instruction, Module, Script, ScriptTemplate, VirtualEnvironment, VirtualEnvironmentManager,
};
#[cfg(unix)]
use acorn_cmd::{CancellationToken, CaptureLimits, CommandError, ExecutionOptions, OutputStream, ProcessTermination};
#[cfg(unix)]
use core::time::Duration;
use std::ffi::OsString;
use std::fs::{create_dir_all, remove_dir_all, write};
#[cfg(unix)]
use std::io;
use std::path::{Path, PathBuf};

const FIXTURES: &str = "../../tests/fixtures";

#[cfg(unix)]
fn shell_script(command: &str) -> Script {
    Script::init()
        .command(Command::init().name("sh".to_string()).build())
        .arguments(["-c", command])
        .build()
}

#[test]
fn test_add_dot_slash() {
    assert_eq!(add_dot_slash(PathBuf::from("foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("./foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar")), PathBuf::from("./foo/bar"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar.json")), PathBuf::from("./foo/bar.json"));
}
#[test]
fn test_command_interpolation_expands_loaded_and_inline_templates_without_changing_arguments() {
    let root = std::env::temp_dir().join(format!("xylem-command-template-{}", std::process::id()));
    create_dir_all(&root).expect("create module directory");
    write(
        root.join("template.json"),
        r#"{"command":"{{ PROGRAM }}","arguments":["two words", "", "{{INPUT}}"]}"#,
    )
    .expect("write external template");
    let values = serde_json::json!({"PROGRAM": "{{ALIAS}}", "ALIAS": "path with spaces/tool"});
    let external: Module = serde_json::from_value(serde_json::json!({
        "name": "external", "type": "script", "uri": format!("file:{}", root.display()),
        "variables": values, "template": "template.json"
    }))
    .expect("parse external module");
    let loaded = Script::from_module(external, None).expect("load and expand external command");
    assert_eq!(loaded.command.name, "path with spaces/tool");
    assert_eq!(loaded.arguments, ["two words", "", "{{INPUT}}"]);
    let inline: Module = serde_json::from_value(serde_json::json!({
        "name": "inline", "type": "script", "uri": format!("file:{}", root.display()),
        "variables": {"PROGRAM": "path with spaces/tool"},
        "template": {"command": "{{PROGRAM}}", "arguments": ["two words", ""]}
    }))
    .expect("parse inline module");
    let loaded = Script::from_module(inline, None).expect("load and expand inline command");
    assert_eq!(loaded.command.name, "path with spaces/tool");
    assert_eq!(loaded.arguments, ["two words", ""]);
    remove_dir_all(root).expect("remove module directory");
}
#[test]
fn test_command_interpolation_rejects_unavailable_input_before_launch() {
    let root = std::env::temp_dir().join(format!("xylem-command-missing-{}", std::process::id()));
    create_dir_all(&root).expect("create module directory");
    let module: Module = serde_json::from_value(serde_json::json!({
        "name": "missing", "type": "script", "uri": format!("file:{}", root.display()),
        "template": {"command": "{{INPUT}}", "arguments": []}
    }))
    .expect("parse module");
    let error = Script::from_module(module, None).expect_err("input is unavailable to static command expansion");
    assert!(
        matches!(
            error,
            ModuleLoadError::Template {
                source: TemplateError::Interpolation { .. },
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("modules.missing.template.command"));
    remove_dir_all(root).expect("remove module directory");
}
#[test]
fn test_conda_and_mamba_use_input_owned_prefixes() {
    let prefix = std::env::temp_dir().join(format!("xylem-environment-prefix-{}", std::process::id()));
    let prefix_string = prefix.to_string_lossy().into_owned();
    let command = Command::init().name("python".to_string()).build();
    let conda = VirtualEnvironment::init()
        .name("ignored-name".to_string())
        .path(prefix_string.clone())
        .manager(VirtualEnvironmentManager::Conda)
        .build();
    let mamba = VirtualEnvironment::init()
        .name("ignored-name".to_string())
        .path(prefix_string.clone())
        .manager(VirtualEnvironmentManager::Mamba)
        .build();
    let script = Script::init().command(command).arguments(["-V"]).build();
    assert_eq!(
        script.clone().with_virtual_environment(Some(conda)).to_string(),
        format!("conda run --prefix {prefix_string} --no-capture-output python -V")
    );
    assert_eq!(
        script.with_virtual_environment(Some(mamba)).to_string(),
        format!("mamba run --prefix {prefix_string} python -V")
    );
}
#[test]
fn test_container_startup_resolution() {
    let image_default = ContainerEnvironment::init().name("default".to_string()).build();
    let explicit = ContainerEnvironment::init()
        .name("explicit".to_string())
        .command(vec!["serve".to_string(), "--port".to_string(), "8080".to_string()])
        .build();
    let keep_alive = ContainerEnvironment::init().name("keep-alive".to_string()).keep_alive(true).build();
    assert_eq!(image_default.resolved_startup(), (ContainerStartup::ImageDefault, Vec::new()));
    assert_eq!(
        explicit.resolved_startup(),
        (
            ContainerStartup::ExplicitCommand,
            ["serve", "--port", "8080"].map(OsString::from).to_vec()
        )
    );
    assert_eq!(
        keep_alive.resolved_startup(),
        (ContainerStartup::KeepAlive, ["sleep", "infinity"].map(OsString::from).to_vec())
    );
}
#[test]
fn test_create_conda_environment_uses_input_owned_prefix() {
    let path = PathBuf::from(format!("{FIXTURES}/environment.yml"));
    let prefix = std::env::temp_dir().join(format!("xylem-environment-create-prefix-{}", std::process::id()));
    let script = Script::maybe_create_virtual_environment(
        path.parent().expect("fixture parent").to_string_lossy().into_owned(),
        Some(
            VirtualEnvironment::init()
                .name("ignored-name".to_string())
                .path(prefix.display().to_string())
                .manager(VirtualEnvironmentManager::Conda)
                .build(),
        ),
    )
    .expect("inspect Conda environment")
    .expect("Conda manifest should produce a creation command");
    assert_eq!(
        script.to_string(),
        format!("conda env create --yes --prefix {} -f environment.yml", prefix.display())
    );
}
#[test]
fn test_create_pixi_environment() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-module"));
    let environment = VirtualEnvironment::init()
        .name("default".to_string())
        .manager(VirtualEnvironmentManager::Pixi)
        .build();
    let script = Script::maybe_create_virtual_environment(path.to_string_lossy().to_string(), Some(environment))
        .expect("inspect Pixi environment")
        .expect("Pixi manifest should produce an install command");

    assert_eq!(script.to_string(), "pixi install --environment default");
    assert!(Path::new(&script.current_dir).ends_with("tests/fixtures/pixi-module"));
}
#[test]
fn test_get_conda_environment_list() {
    let list = get_conda_environment_list();
    let conda = Command::init().name("conda".to_string()).build();
    if conda.test() {
        assert!(list.iter().all(|name| !name.is_empty()));
    } else {
        assert!(list.is_empty());
    }
}
#[test]
fn test_get_conda_environment_name() {
    let path = format!("{FIXTURES}/environment.yml");
    let manifest_path = PathBuf::from(path);
    assert!(get_conda_environment_name(manifest_path).is_some_and(|name| name.starts_with("some-really-interesting-name")));
    let manifest_path = PathBuf::from("/non/existing/manifest.yml");
    assert_eq!(get_conda_environment_name(manifest_path), None);
}
#[test]
fn test_get_mamba_environment_list() {
    let list = get_mamba_environment_list();
    let mamba = Command::init().name("mamba".to_string()).build();
    if mamba.test() {
        assert!(list.iter().all(|name| !name.is_empty()));
    } else {
        assert!(list.is_empty());
    }
}
#[test]
fn test_resolve_pixi_environment() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-module"));
    let environment = ScriptTemplate::resolve_python_virtual_environment(path)
        .expect("inspect Pixi environment")
        .expect("Pixi should be detected from pixi.toml");
    assert_eq!(environment.name, "default");
    assert_eq!(environment.manager, VirtualEnvironmentManager::Pixi);
}
#[test]
fn test_resolve_pixi_environment_from_pyproject() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-pyproject-module"));
    let environment = ScriptTemplate::resolve_python_virtual_environment(path)
        .expect("inspect Pixi environment")
        .expect("Pixi should be detected from pyproject.toml");
    assert_eq!(environment.name, "default");
    assert_eq!(environment.manager, VirtualEnvironmentManager::Pixi);
}
#[cfg(unix)]
#[test]
fn test_script_execute_checked_preserves_cancelled_result() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let options = ExecutionOptions::new().with_cancellation(cancellation);
    let error = shell_script("exit 0")
        .execute_checked(&options)
        .expect_err("pre-cancelled execution should fail");
    let result = error.result().expect("cancelled execution result");
    assert_eq!(result.termination(), ProcessTermination::Cancelled);
    assert!(result.status().is_none());
    assert!(matches!(
        error,
        ProcessError::Managed {
            source: CommandError::Cancelled(_),
            ..
        }
    ));
}
#[cfg(unix)]
#[test]
fn test_script_execute_checked_preserves_nonzero_result() {
    let error = shell_script(r"printf 'out\377'; printf 'err\376' >&2; exit 7")
        .execute_checked(&ExecutionOptions::new())
        .expect_err("nonzero execution should fail");
    let result = error.result().expect("nonzero execution result");
    assert_eq!(result.stdout().bytes(), b"out\xff");
    assert_eq!(result.stderr().bytes(), b"err\xfe");
    assert_eq!(result.status().and_then(acorn_cmd::ProcessStatus::code), Some(7));
    assert_eq!(result.termination(), ProcessTermination::Exited);
    assert!(error.context().is_none());
    assert!(error.to_string().contains("process exited with status 7"));
    assert!(matches!(
        error,
        ProcessError::Managed {
            source: CommandError::NonZeroExit(_),
            ..
        }
    ));
}
#[cfg(unix)]
#[test]
fn test_script_execute_checked_preserves_sink_context() {
    let options = ExecutionOptions::new().with_sink(|_: OutputStream, _: &[u8]| Err(io::Error::other("rejected output")));
    let error = shell_script("printf output")
        .execute_checked(&options)
        .expect_err("sink rejection should fail");
    let context = error.context().expect("sink failure context");
    assert!(context.duration() < Duration::from_secs(5));
    assert!(error.result().is_none());
    assert!(
        matches!(
            &error,
            ProcessError::Managed {
                source: CommandError::Cleanup { .. } | CommandError::Sink { .. },
                ..
            }
        ),
        "unexpected sink failure: {error:?}"
    );
}
#[cfg(unix)]
#[test]
fn test_script_execute_checked_preserves_timeout_result() {
    let options = ExecutionOptions::new().with_timeout(Duration::from_millis(20));
    let error = shell_script("sleep 2")
        .execute_checked(&options)
        .expect_err("timed execution should fail");
    let result = error.result().expect("timeout execution result");
    assert_eq!(result.termination(), ProcessTermination::TimedOut);
    assert!(result.duration() >= Duration::from_millis(20));
    assert!(matches!(
        error,
        ProcessError::Managed {
            source: CommandError::TimedOut(_),
            ..
        }
    ));
}
#[cfg(unix)]
#[test]
fn test_script_execute_preserves_separate_bounded_bytes() {
    let options = ExecutionOptions::new().with_capture_limits(CaptureLimits::new(Some(4), Some(10)));
    let result = shell_script(r"printf 'abcdef\377'; printf 'uvwxyz\376' >&2")
        .execute(&options)
        .expect("execute bounded output");
    assert_eq!(result.stdout().bytes(), b"def\xff");
    assert_eq!(result.stdout().truncated_bytes(), 3);
    assert_eq!(result.stderr().bytes(), b"xyz\xfe");
    assert_eq!(result.stderr().truncated_bytes(), 3);
    assert_eq!(result.status().and_then(acorn_cmd::ProcessStatus::code), Some(0));
    assert_eq!(result.termination(), ProcessTermination::Exited);
    assert!(result.duration() < Duration::from_secs(5));
}
#[test]
fn test_script_from_template_preserves_exec_arguments() {
    let template = ScriptTemplate {
        arguments: Instruction::ExecFormat(vec!["two words".to_string(), String::new(), "{{ INPUT }}".to_string()]),
        command: "echo".to_string(),
        container: None,
        virtual_environment: None,
    };
    let script = Script::from_template(template).expect("parse exec-format template");
    assert_eq!(script.arguments, ["two words", "", "{{INPUT}}"]);
}
#[test]
fn test_script_from_template_preserves_shell_arguments() {
    let template = ScriptTemplate {
        arguments: Instruction::ShellFormat(r#"--label "two words" '' escaped\ value "{{ INPUT }}""#.to_string()),
        command: "echo".to_string(),
        container: None,
        virtual_environment: None,
    };
    let script = Script::from_template(template).expect("parse shell-format template");
    assert_eq!(script.arguments, ["--label", "two words", "", "escaped value", "{{INPUT}}"]);
}
#[test]
fn test_script_from_template_rejects_malformed_shell_arguments() {
    let template = ScriptTemplate {
        arguments: Instruction::ShellFormat("'unterminated".to_string()),
        command: "echo".to_string(),
        container: None,
        virtual_environment: None,
    };
    assert!(Script::from_template(template).is_err());
}
#[test]
fn test_virtual_environment_manager_from_reference() {
    fn implements_try_from<T: for<'a> TryFrom<&'a str>>() {}
    implements_try_from::<VirtualEnvironmentManager>();
    assert_eq!(VirtualEnvironmentManager::from("conda"), VirtualEnvironmentManager::Conda);
    assert_eq!(VirtualEnvironmentManager::from("mamba"), VirtualEnvironmentManager::Mamba);
    assert_eq!(VirtualEnvironmentManager::from("unsupported"), VirtualEnvironmentManager::Unknown);
}
