//! Command line (multi)tool for parameterized validation of folder structure and file content
use clap::{Parser, Subcommand};
use dotenv::dotenv;
use env_logger::Env;

mod commands;
mod reporters;
mod util;

use util::Void;

// Environment variables
const PIPE_LOG_LEVEL: &str = "PIPE_LOG_LEVEL";
const PIPE_LOG_STYLE: &str = "PIPE_LOG_STYLE";
/// Multi-tool for verifying myriad aspects of data processing pipelines
///
/// ██████  ██ ██████  ███████
/// ██   ██ ██ ██   ██ ██
/// ██████  ██ ██████  █████
/// ██      ██ ██      ██
/// ██      ██ ██      ███████
///
#[derive(Debug, Parser)]
#[command(author, version, about, verbatim_doc_comment)]
struct Arguments {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Debug, Subcommand)]
#[command(long_about = None)]
enum Commands {
    /// Run checks on a given path to a directory
    ///
    ///     $> pipe check /path/to/folder
    ///
    ///     $> echo /path/to/folder | pipe check -pm
    ///
    ///     $> pipe check /path/to/folder --reporter json console --output results.json
    ///
    #[clap(verbatim_doc_comment)]
    Check {
        /// Path to check
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        path: Option<std::path::PathBuf>,
        /// Path to configuration file
        #[arg(short = 'c', long = "config", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Exclude certain files
        #[arg(short = 'e', long = "exclude", value_name = "STRING")]
        exclude: Option<String>,
        /// Reporter(s) to use to output the results - console (default) and/or json
        #[arg(short = 'r', long = "reporter", value_name = "STRING", num_args=0..=2)]
        reporter: Option<Vec<String>>,
        /// Destination path for reporters that use one
        #[arg(short = 'o', long = "output", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        output: Option<std::path::PathBuf>,
        /// Return all results that pass or fail check conditions
        #[arg(short = 'a', long = "all", value_name = "BOOL")]
        all: bool,
        /// Only return results that pass their associated check
        #[arg(short = 'p', long = "pass", value_name = "BOOL")]
        pass: bool,
        /// Report results in minimal format
        #[arg(short = 'm', long = "minimal", value_name = "BOOL")]
        minimal: bool,
        /// Deduplicate results
        #[arg(short = 'u', long = "unique", value_name = "BOOL")]
        unique: bool,
    },
    /// Check environment parameters and verify PIPE requirements are met
    ///
    ///     $> pipe doctor --config /path/to/config
    ///
    ///     $> pipe doctor --fix
    ///
    #[clap(verbatim_doc_comment)]
    Doctor {
        /// Path to configuration file
        ///
        /// Validate PIPE requirements are met for a given configuration
        #[arg(short = 'c', long = "config", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Scan environment and attempt to fix unmet requirements
        #[arg(short = 'f', long = "fix", value_name = "BOOL")]
        fix: bool,
    },
    /// Execute module(s)
    ///
    ///     $> pipe run /path/to/module/code --config /path/to/config
    ///
    ///     $> echo /path/to/module/code | pipe run
    ///
    ///     $> pipe check /path/to/module/code --reporter json console
    ///
    #[clap(verbatim_doc_comment)]
    Run {
        /// Path to module(s)
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        path: Option<std::path::PathBuf>,
        /// Path to configuration file
        #[arg(short = 'c', long = "config", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Reporter(s) to use to output the results - console (default) and/or json
        #[arg(short = 'r', long = "reporter", value_name = "STRING", num_args=0..=2)]
        reporter: Option<Vec<String>>,
        /// Destination path for reporters that use one
        #[arg(short = 'o', long = "output", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        output: Option<std::path::PathBuf>,
    },
}
fn main() -> Void {
    dotenv().ok();
    let env = Env::default().filter_or(PIPE_LOG_LEVEL, "error").write_style_or(PIPE_LOG_STYLE, "always");
    env_logger::init_from_env(env);
    let args = Arguments::parse();
    match &args.command {
        | Some(Commands::Check {
            path,
            reporter,
            output,
            all,
            pass,
            minimal,
            unique,
            ..
        }) => commands::check::run(path, reporter, output, *all, *pass, *minimal, *unique),
        | Some(Commands::Doctor { .. }) => Ok(()),
        | Some(Commands::Run { .. }) => Ok(()),
        | None => Ok(()),
    }
}
