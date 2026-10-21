//! Generated CLI help contract tests.
fn help(arguments: &[&str]) -> String {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("xylem");
    let output = command.args(arguments).output().expect("render CLI help");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let help = String::from_utf8(output.stdout).expect("CLI help should be UTF-8").replace("\r\n", "\n");
    help.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n"
}

#[test]
fn test_generated_help_is_current() {
    assert_eq!(help(&["--help"]), include_str!("../../../docs/src/generated/xylem-help.txt"));
    assert_eq!(
        help(&["export", "--help"]),
        include_str!("../../../docs/src/generated/xylem-export-help.txt")
    );
    assert_eq!(help(&["run", "--help"]), include_str!("../../../docs/src/generated/xylem-run-help.txt"));
    assert_eq!(help(&["tui", "--help"]), include_str!("../../../docs/src/generated/xylem-tui-help.txt"));
}
