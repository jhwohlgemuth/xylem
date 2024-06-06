use crate::commands::doctor::*;
use crate::util::to_string;

#[test]
fn test_script_from_template() {
    let expected_value: Vec<String> = to_string(vec!["-m", "lib", "--input", "{{INPUT_DIRECTORY}}", "--output", "{{OUTPUT_DIRECTORY}}"]);
    // "shell" format arguments
    let path = PathBuf::from("tests/fixtures/template.json");
    let script = Script::from_template(path);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
    // "exec" format arguments
    let path = PathBuf::from("tests/fixtures/template_exec.json");
    let script = Script::from_template(path);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
}

#[test]
fn test_script_run() {
    let script = Script {
        command: Command {
            name: "echo".to_string(),
            conda_environment: None,
        },
        arguments: to_string(vec!["hello", "world"]),
    };
    let output = script.run();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello world\n");
    let path = PathBuf::from("tests/fixtures/template_run.json");
    let script = Script::from_template(path);
    assert_eq!(script.run().stdout, b"hello world\n");
}

#[test]
fn test_import_config() {
    let path = "tests/fixtures/config.json";
    let content = import_config(PathBuf::from(path)).expect("[ERROR] => Cannot import config in tests");
    assert_eq!(content.variables.unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(content.modules.len(), 3);
}

#[test]
fn test_is_command() {
    let cargo: Command = Command {
        name: "cargo".to_string(),
        conda_environment: None,
    };
    let nonexistent: Command = Command {
        name: "does-not-exist".to_string(),
        conda_environment: None,
    };
    let python: Command = Command {
        name: "python".to_string(),
        conda_environment: Some("base".to_string()),
    };
    assert!(is_command(cargo));
    assert!(!is_command(nonexistent));
    assert!(is_command(python));
}
