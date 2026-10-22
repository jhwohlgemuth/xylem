//! Repository security, dependency, and release automation.

extern crate alloc;

mod dependency;
mod homebrew;
mod release;

use std::process::ExitCode;

fn usage() {
    eprintln!("Usage: cargo xtask <command> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  audit [--offline]                         Run security and dependency gates");
    eprintln!("  check-release-crates                     Verify the crates.io publication list");
    eprintln!("  check-version                            Verify release version metadata");
    eprintln!("  dependency-policy                        Validate the dependency policy");
    eprintln!("  homebrew-formula <version> [path] [sha]  Render the Homebrew formula");
    eprintln!("  publish-crates                           Publish workspace crates in order");
    eprintln!("  release-version <version>                Update workspace version and internal pins");
    eprintln!("  version                                  Print the workspace version");
}

fn run(arguments: &[String]) -> Result<(), String> {
    match arguments {
        | [command] if command == "audit" => dependency::audit(false),
        | [command] if command == "check-release-crates" => release::check_release_crates(),
        | [command] if command == "check-version" => release::check_version(),
        | [command] if command == "dependency-policy" => dependency::check(),
        | [command] if command == "publish-crates" => release::publish_crates(),
        | [command] if command == "version" => release::workspace_version().map(|version| println!("{version}")),
        | [command, flag] if command == "audit" && flag == "--offline" => dependency::audit(true),
        | [command, version] if command == "homebrew-formula" => homebrew::generate(version, "Formula/xylem.rb", None),
        | [command, version] if command == "release-version" => release::release_version(version),
        | [command, version, output] if command == "homebrew-formula" => homebrew::generate(version, output, None),
        | [command, version, output, sha256] if command == "homebrew-formula" => homebrew::generate(version, output, Some(sha256)),
        | _ => Err("unknown or invalid xtask command".to_string()),
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        | Ok(()) => ExitCode::SUCCESS,
        | Err(error) => {
            eprintln!("xtask: {error}");
            usage();
            ExitCode::FAILURE
        }
    }
}
