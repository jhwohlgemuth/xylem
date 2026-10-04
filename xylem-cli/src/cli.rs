use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use std::path::PathBuf;
use xylem_lib::execution::ExecutionLimits;
use xylem_lib::script::ContainerCleanup;
use xylem_lib::ReporterType;

/// Ordered default workflow configuration names.
pub const DEFAULT_CONFIG_NAMES: [&str; 4] = [".xylem.json", ".xylem.jsonc", ".xylem.yaml", ".xylem.yml"];

#[derive(Debug, Subcommand)]
#[command(long_about = None)]
/// Top-level Xylem commands.
pub enum Commands {
    /// Run workflow using configuration file
    ///
    /// ```text
    ///     Run with INFO logging level, discovers a .xylem.* config in the current directory
    ///     $> xylem run -vv
    ///
    ///     Perform dry run using configuration file at /path/to/config.json
    ///     $> xylem run /path/to/config.json --dry-run
    /// ```
    ///
    #[clap(verbatim_doc_comment)]
    Run {
        /// Path to configuration file
        #[arg(value_name = "CONFIG", value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
        /// Reporter(s) used for the final durable summary
        #[arg(short, long, value_enum, value_delimiter = ',', num_args = 1.., default_value = "console")]
        reporter: Vec<ReporterType>,
        /// Workflow artifact root; overrides configuration output
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        artifact_output: Option<PathBuf>,
        /// Override cleanup policy for containers created by Xylem
        #[arg(long, value_enum)]
        container_cleanup: Option<ContainerCleanup>,
        /// Directory for the durable journal and file-based reports
        #[arg(short = 'o', long = "report-output", visible_alias = "output", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        report_output: Option<PathBuf>,
        /// Value(s) to use as INPUT variable in workflow
        #[arg(short, long, value_name = "LIST", value_delimiter = ',')]
        input: Option<Vec<String>>,
        /// Execute workflow dry run (do not actually make changes to environment)
        #[arg(short, long = "dry-run", value_name = "BOOL")]
        dry_run: bool,
        /// Collect run-wide NVIDIA GPU telemetry with nvidia-smi
        #[arg(long = "gpu-monitor")]
        gpu_monitor: bool,
        /// Execute workflow without connection to the internet
        #[arg(short = 'X', long, value_name = "BOOL")]
        offline: bool,
        /// Do not forward resolved Xylem environment variables into containers
        #[arg(long)]
        no_container_env: bool,
        /// Skip modules whose declared outputs and completion marker are current
        #[arg(long, value_name = "BOOL")]
        resume: bool,
        /// Exact remote module host(s) to allow; CLI values replace configured hosts
        #[arg(long = "allowed", value_name = "HOST[:PORT]", value_delimiter = ',')]
        allowed: Option<Vec<String>>,
        /// Skip calculating and verifying checksum for downloaded binary files
        #[arg(long = "skip-verify-checksum", value_name = "BOOL")]
        skip_verify_checksum: bool,
        /// Skip enforcing workflow configured requirements (mostly for troubleshooting)
        #[arg(long = "skip-verify-requirements", value_name = "BOOL")]
        skip_verify_requirements: bool,
        #[command(flatten)]
        /// Logging verbosity for this run.
        verbose: Verbosity,
    },
    /// Open the local workflow dashboard and retained run history.
    #[cfg(feature = "tui")]
    Tui {
        /// Workflow configuration preloaded into the New Run action.
        #[arg(value_name = "CONFIG", value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
}

///
///
/// ░  ░░░░  ░░  ░░░░  ░░  ░░░░░░░░        ░░  ░░░░  ░
/// ▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒  ▒▒▒▒▒▒▒▒  ▒▒▒▒▒▒▒▒   ▒▒   ▒
/// ▓▓▓    ▓▓▓▓▓▓    ▓▓▓▓  ▓▓▓▓▓▓▓▓      ▓▓▓▓        ▓
/// ██  ██  ██████  █████  ████████  ████████  █  █  █
/// █  ████  █████  █████        ██        ██  ████  █
///
/// Digital xylem that transports data through workflows with structure and support
///
#[derive(Debug, Parser)]
#[command(author, version, about, verbatim_doc_comment)]
pub struct Arguments {
    #[command(flatten)]
    /// Global logging verbosity.
    pub verbose: Verbosity,
    /// Maximum number of Rayon workers used by Xylem (does not limit child tools)
    #[arg(default_value_t = ExecutionLimits::default(), short = 't', long, value_name = "N", global = true)]
    pub threads: ExecutionLimits,
    /// Load environment variables from a specific dotenv file
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath, global = true)]
    pub env: Option<PathBuf>,
    /// Disable interactive workflow progress indicators
    #[arg(long, global = true)]
    pub no_progress: bool,
    #[command(subcommand)]
    /// Command to execute; omission prints no output and exits successfully.
    pub command: Option<Commands>,
}

#[cfg(test)]
mod tests;
