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
    eprintln!("  dependency-policy                        Validate the dependency policy");
    eprintln!("  homebrew-formula <version> [path] [sha]  Render the Homebrew formula");
    eprintln!("  publish-crates                           Publish workspace crates in order");
    eprintln!("  version                                  Print the workspace version");
}

fn run(arguments: &[String]) -> Result<(), String> {
    match arguments {
        | [command] if command == "dependency-policy" => dependency::check(),
        | [command] if command == "publish-crates" => release::publish_crates(),
        | [command] if command == "version" => release::workspace_version().map(|version| println!("{version}")),
        | [command, flag] if command == "audit" && flag == "--offline" => dependency::audit(true),
        | [command] if command == "audit" => dependency::audit(false),
        | [command, version] if command == "homebrew-formula" => homebrew::generate(version, "Formula/xylem.rb", None),
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
