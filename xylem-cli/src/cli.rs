use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use std::path::PathBuf;
use xylem_lib::execution::ExecutionLimits;
use xylem_lib::script::ContainerCleanup;
use xylem_lib::ReporterType;

#[derive(Debug, Subcommand)]
#[command(long_about = None)]
pub enum Commands {
    /// Run workflow using configuration file
    ///
    /// ```text
    ///     Run with INFO logging level, loads config.json from current directory
    ///     $> xylem run -vv
    ///
    ///     Perform dry run using configuration file at /path/to/config.json
    ///     $> xylem run --config /path/to/config --dry-run
    /// ```
    ///
    #[clap(verbatim_doc_comment)]
    Run {
        /// Path to configuration file
        #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
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
        verbose: Verbosity,
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
    pub command: Option<Commands>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(arguments: &[&str]) -> Arguments {
        Arguments::try_parse_from(arguments).expect("arguments should parse")
    }

    #[test]
    fn test_allowed_accepts_repeated_and_comma_delimited_values() {
        let args = parse(&["xylem", "run", "--allowed", "one.example,two.example:8443", "--allowed", "three.example"]);
        match args.command {
            | Some(Commands::Run { allowed, .. }) => assert_eq!(
                allowed,
                Some(vec![
                    "one.example".to_string(),
                    "two.example:8443".to_string(),
                    "three.example".to_string(),
                ])
            ),
            | None => panic!("run command should parse"),
        }
    }
    #[test]
    fn test_artifact_and_report_outputs_are_distinct() {
        let args = parse(&["xylem", "run", "--artifact-output", "artifacts", "--report-output", "reports"]);
        match args.command {
            | Some(Commands::Run {
                artifact_output,
                report_output,
                ..
            }) => {
                assert_eq!(artifact_output, Some(PathBuf::from("artifacts")));
                assert_eq!(report_output, Some(PathBuf::from("reports")));
            }
            | None => panic!("run command should parse"),
        }
        match parse(&["xylem", "run", "--output", "reports"]).command {
            | Some(Commands::Run { report_output, .. }) => {
                assert_eq!(report_output, Some(PathBuf::from("reports")));
            }
            | None => panic!("run command should parse"),
        }
    }
    #[test]
    fn test_container_policy_flags_parse() {
        match parse(&["xylem", "run", "--container-cleanup", "on-success", "--no-container-env"]).command {
            | Some(Commands::Run {
                container_cleanup,
                no_container_env,
                ..
            }) => {
                assert_eq!(container_cleanup, Some(ContainerCleanup::OnSuccess));
                assert!(no_container_env);
            }
            | None => panic!("run command should parse"),
        }
    }
    #[test]
    fn test_global_env_and_progress_flags_parse_before_or_after_run() {
        let before = parse(&["xylem", "--env", "custom.env", "--no-progress", "run"]);
        assert_eq!(before.env, Some(PathBuf::from("custom.env")));
        assert!(before.no_progress);
        let after = parse(&["xylem", "run", "--env", "custom.env", "--no-progress"]);
        assert_eq!(after.env, Some(PathBuf::from("custom.env")));
        assert!(after.no_progress);
    }
    #[test]
    fn test_help_documents_thread_contract() {
        let help = Arguments::command().render_long_help().to_string();
        assert!(help.contains("-t, --threads <N>"));
        assert!(help.contains("[default: 10]"));
        assert!(help.contains("does not limit child tools"));
    }
    #[test]
    fn test_reporters_are_typed_repeatable_and_default_to_console() {
        match parse(&["xylem", "run"]).command {
            | Some(Commands::Run { reporter, .. }) => assert_eq!(reporter, vec![ReporterType::Console]),
            | None => panic!("run command should parse"),
        }
        match parse(&["xylem", "run", "--reporter", "json,console", "--reporter", "json"]).command {
            | Some(Commands::Run { reporter, .. }) => assert_eq!(reporter, vec![ReporterType::Json, ReporterType::Console, ReporterType::Json]),
            | None => panic!("run command should parse"),
        }
        assert!(Arguments::try_parse_from(["xylem", "run", "--reporter", "unknown"]).is_err());
    }
    #[test]
    fn test_resume_is_explicit() {
        match parse(&["xylem", "run", "--resume"]).command {
            | Some(Commands::Run { resume, .. }) => assert!(resume),
            | None => panic!("run command should parse"),
        }
        match parse(&["xylem", "run"]).command {
            | Some(Commands::Run { resume, .. }) => assert!(!resume),
            | None => panic!("run command should parse"),
        }
    }
    #[test]
    fn test_threads_accept_short_and_long_flags() {
        assert_eq!(parse(&["xylem", "-t", "1", "run"]).threads.threads(), 1);
        assert_eq!(parse(&["xylem", "run", "--threads", "24"]).threads.threads(), 24);
    }
    #[test]
    fn test_threads_default_to_ten() {
        assert_eq!(parse(&["xylem"]).threads.threads(), 10);
    }
    #[test]
    fn test_threads_reject_zero_non_numeric_and_missing_values() {
        for arguments in [
            &["xylem", "--threads", "0"][..],
            &["xylem", "--threads", "many"][..],
            &["xylem", "--threads"][..],
        ] {
            assert!(Arguments::try_parse_from(arguments).is_err(), "{arguments:?} should fail");
        }
        let zero = Arguments::try_parse_from(["xylem", "--threads", "0"])
            .expect_err("zero should fail")
            .to_string();
        assert!(zero.contains("thread count must be greater than zero"));
    }
}
