use crate::cli::Arguments;
use assert_cmd::Command;
use clap::CommandFactory;

#[test]
fn test_cli() {
    Arguments::command().debug_assert();
}

#[test]
fn test_help() {
    let expected = "Multi-tool for verifying myriad aspects of data processing pipelines

   ███████  ██  ███████  ████▓▒░
   ██   ██  ██  ██   ██  ██
░▒▓██████████████████████████▓▒░
   ██       ██  ██       ██
   ██       ██  ██       ████▓▒░
Parallel Integration & Processing Engine

Usage: pipe-cli [OPTIONS] [COMMAND]

Commands:
  check   Run checks on a given path to a directory
  doctor  Check environment parameters and verify PIPE requirements are met
  run     Run workflow using configuration file
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...
          Increase logging verbosity

  -q, --quiet...
          Decrease logging verbosity

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version\n";
    let mut cmd = Command::cargo_bin("pipe-cli").unwrap();
    let assert = cmd.arg("help").assert();
    assert.success().stdout(expected);
}

#[test]
fn test_run() {
    let expected = "error: a value is required for '--config <PATH>' but none was supplied

For more information, try '--help'.\n";
    let mut cmd = Command::cargo_bin("pipe-cli").unwrap();
    let assert = cmd.args(["run", "--config"]).assert();
    assert.failure().code(2).stderr(expected);
}

#[test]
fn test_version() {
    let expected = "pipe-cli 0.0.3\n";
    let mut cmd = Command::cargo_bin("pipe-cli").unwrap();
    let assert = cmd.args(["--version"]).assert();
    assert.success().code(0).stdout(expected);
}
