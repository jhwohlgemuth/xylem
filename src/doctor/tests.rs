use crate::doctor::*;

const CARGO: Command = Command {
    name: "cargo",
    conda_environment: None,
};
const NONEXISTENT: Command = Command {
    name: "does-not-exist",
    conda_environment: None,
};
const PYTHON: Command = Command {
    name: "python",
    conda_environment: Some("base"),
};

#[test]
fn test_script() {
    let script = Script {
        command: Command {
            name: "echo",
            conda_environment: None,
        },
        arguments: vec!["hello", "world"],
    };
    let output = script.run();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello world\n");
}

#[test]
fn test_is_command() {
    assert!(is_command(CARGO));
    assert!(!is_command(NONEXISTENT));
    assert!(is_command(PYTHON));
}
