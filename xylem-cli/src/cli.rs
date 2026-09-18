use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use xylem_lib::execution::ExecutionLimits;

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
    /// Maximum number of Rayon workers used by Xylem (does not limit child tools)
    #[arg(default_value_t = ExecutionLimits::default(), short = 't', long, value_name = "N", global = true)]
    pub threads: ExecutionLimits,
    #[command(subcommand)]
    pub command: Option<Commands>,
}
#[derive(Debug, Subcommand)]
#[command(long_about = None)]
pub enum Commands {
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
        #[arg(short = 'X', long, value_name = "BOOL")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(arguments: &[&str]) -> Arguments {
        Arguments::try_parse_from(arguments).expect("arguments should parse")
    }

    #[test]
    fn threads_default_to_ten() {
        assert_eq!(parse(&["xylem"]).threads.threads(), 10);
    }
    #[test]
    fn threads_accept_short_and_long_flags() {
        assert_eq!(parse(&["xylem", "-t", "1", "run"]).threads.threads(), 1);
        assert_eq!(parse(&["xylem", "run", "--threads", "24"]).threads.threads(), 24);
    }
    #[test]
    fn threads_reject_zero_non_numeric_and_missing_values() {
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
    #[test]
    fn help_documents_thread_contract() {
        let help = Arguments::command().render_long_help().to_string();
        assert!(help.contains("-t, --threads <N>"));
        assert!(help.contains("[default: 10]"));
        assert!(help.contains("does not limit child tools"));
    }
}
