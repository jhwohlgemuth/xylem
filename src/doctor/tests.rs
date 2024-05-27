use crate::doctor::*;

#[test]
fn test_is_command() {
    let cargo = Command {
        name: "cargo",
        conda_environment: None,
    };
    let nonexistent = Command {
        name: "does-not-exist",
        conda_environment: None,
    };
    let python = Command {
        name: "python",
        conda_environment: Some("base"),
    };
    assert!(is_command(cargo));
    assert!(!is_command(nonexistent));
    assert!(is_command(python));
}
