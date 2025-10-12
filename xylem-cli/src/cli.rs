use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

///
/// ░  ░░░░  ░░  ░░░░  ░░  ░░░░░░░░        ░░  ░░░░  ░
/// ▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒  ▒▒▒▒▒▒▒▒  ▒▒▒▒▒▒▒▒   ▒▒   ▒
/// ▓▓▓    ▓▓▓▓▓▓    ▓▓▓▓  ▓▓▓▓▓▓▓▓      ▓▓▓▓        ▓
/// ██  ██  ██████  █████  ████████  ████████  █  █  █
/// █  ████  █████  █████        ██        ██  ████  █
///
/// "Digital xylem" that transports data through workflows with structure and support
///
#[derive(Debug, Parser)]
#[command(author, version, about, verbatim_doc_comment)]
pub struct Arguments {
    #[command(flatten)]
    pub verbose: Verbosity,
    #[command(subcommand)]
    pub command: Option<Commands>,
}
#[derive(Debug, Subcommand)]
#[command(long_about = None)]
pub enum Commands {
    /// Run checks on a given path to a directory
    ///
    ///     $> xylem check /path/to/folder
    ///
    ///     $> echo /path/to/folder | xylem check -pm
    ///
    ///     $> xylem check /path/to/folder --reporter json console --output results.json
    ///
    #[clap(verbatim_doc_comment)]
    Check {
        /// Path to check
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        path: Option<std::path::PathBuf>,
        /// Path to configuration file
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Reporter(s) to use to output the results - console (default) and/or json
        #[arg(short, long, value_name = "STRING", num_args=0..=2)]
        reporter: Option<Vec<String>>,
        /// Destination path for reporters that use one
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        output: Option<std::path::PathBuf>,
        /// Return all results that pass or fail check conditions
        #[arg(short, long, value_name = "BOOL")]
        all: bool,
        /// Only return results that pass their associated check
        #[arg(short, long, value_name = "BOOL")]
        pass: bool,
        /// Report results in minimal format
        #[arg(short, long, value_name = "BOOL")]
        minimal: bool,
        /// Deduplicate results
        #[arg(short, long, value_name = "BOOL")]
        unique: bool,
    },
    /// Check environment parameters and verify Xylem requirements are met
    ///
    ///     $> xylem doctor --config /path/to/config
    ///
    ///     $> xylem doctor --fix
    ///
    #[clap(verbatim_doc_comment)]
    Doctor {
        /// Path to configuration file
        ///
        /// Validate Xylem requirements are met for a given configuration
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Scan environment and attempt to fix unmet requirements
        #[arg(short, long, value_name = "BOOL")]
        fix: bool,
    },
    /// Run workflow using configuration file
    ///
    ///     Run with INFO logging level, loads config.json from current directory
    ///     $> xylem run -vv
    ///
    ///     Perform dry run using configuration file at /path/to/config.json
    ///     $> xylem run --config /path/to/config --dry-run
    ///
    #[clap(verbatim_doc_comment)]
    Run {
        /// Path to configuration file
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        config: Option<std::path::PathBuf>,
        /// Reporter(s) to use to output the results - console (default) and/or json
        #[arg(short, long, value_name = "STRING", num_args=0..=2)]
        reporter: Option<Vec<String>>,
        /// Destination path for reporters that use one
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        output: Option<std::path::PathBuf>,
        /// Value(s) to use as INPUT variable in workflow
        #[arg(short, long, value_name = "LIST", value_delimiter = ',')]
        input: Option<Vec<String>>,
        /// Execute workflow dry run (do not actually make changes to environment)
        #[arg(short, long = "dry-run", value_name = "BOOL")]
        dry_run: bool,
        /// Execute workflow without connection to the internet
        #[arg(long, value_name = "BOOL")]
        offline: bool,
        /// Skip calculating and verifying checksum for downloaded binary files
        #[arg(long = "skip-verify-checksum", value_name = "BOOL")]
        skip_verify_checksum: bool,
        /// Skip enforcing workflow configured requirements (mostly for troubleshooting)
        #[arg(long = "skip-verify-requirements", value_name = "BOOL")]
        skip_verify_requirements: bool,
        #[command(flatten)]
        verbose: Verbosity,
    },
}
