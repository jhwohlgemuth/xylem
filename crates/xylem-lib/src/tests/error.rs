use crate::error::{AcquisitionError, ProcessError, PyProjectError, RequirementError, TemplateError};
use crate::remote::{RemoteError, RemotePolicy};
use crate::script::{Command, Script, ScriptTemplate};
use crate::PyProjectToml;
use core::error::Error;
use std::fs::{remove_dir_all, write};
use std::io;

#[test]
fn test_acquisition_error_preserves_module_and_source() {
    let error = AcquisitionError::Module {
        module: "imagery".to_string(),
        source: io::Error::new(io::ErrorKind::NotFound, "missing checkout"),
    };
    assert_eq!(error.to_string(), "prepare module imagery: missing checkout");
    assert!(error.source().is_some());
}

#[test]
fn test_process_spawn_error_preserves_working_directory_and_source() {
    let script = Script::init()
        .command(Command::init().name("xylem-command-that-cannot-exist".to_string()).build())
        .current_dir("missing-working-directory".to_string())
        .build();
    let error = script.run_checked().expect_err("spawn should fail");
    match &error {
        | ProcessError::Spawn {
            script, working_directory, ..
        } => {
            assert!(script.contains("xylem-command-that-cannot-exist"));
            assert_eq!(working_directory, &std::path::PathBuf::from("missing-working-directory"));
        }
        | ProcessError::Managed { .. } | ProcessError::Output { .. } | ProcessError::Unsuccessful { .. } => {
            panic!("expected spawn failure")
        }
    }
    assert!(error.source().is_some());
}

#[test]
fn test_pyproject_errors_preserve_path_and_source() {
    let path = std::env::temp_dir().join(format!("xylem-missing-pyproject-{}", std::process::id()));
    let error = PyProjectToml::read(path.clone()).expect_err("missing project should fail");
    assert!(matches!(&error, PyProjectError::Read { path: error_path, .. } if error_path == &path));
    assert!(error.source().is_some());
}

#[test]
fn test_remote_errors_preserve_safe_context_and_source() {
    let policy = RemotePolicy::default();
    let error = policy.ensure_allowed("not a URL").expect_err("invalid URL should fail");
    assert!(matches!(&error, RemoteError::InvalidUrl { value, .. } if value == "not a URL"));
    assert!(error.to_string().contains("[INVALID URL]"));
    assert!(error.source().is_some());
}

#[test]
fn test_requirement_error_preserves_command() {
    let error = RequirementError {
        command: "missing-tool".to_string(),
    };
    assert_eq!(error.to_string(), "required command was not found: missing-tool");
    assert!(error.source().is_none());
}

#[test]
fn test_template_errors_preserve_path_and_source() {
    let root = std::env::temp_dir().join(format!("xylem-template-error-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create template error root");
    let path = root.join("template.json");
    write(&path, "{").expect("write malformed template");
    let error = ScriptTemplate::from_path(path.clone()).expect_err("malformed template should fail");
    assert!(matches!(&error, TemplateError::Parse { path: error_path, .. } if error_path == &path));
    assert!(error.source().is_some());
    remove_dir_all(root).expect("remove template error root");
}
