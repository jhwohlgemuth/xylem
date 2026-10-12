//! Xylem Command-Line Interface
//! > Digital xylem that transports data through workflows with structure and support
extern crate alloc;

use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::{eyre, Result};
use std::env::set_current_dir;
use std::io::stderr;
use std::path::{Path, PathBuf};
use std::process::exit;
use tracing_log::AsTrace;
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use xylem_lib::execution::ExecutionLimits;
use xylem_lib::ReporterType;

/// Command-line argument definitions.
pub mod cli;
/// Command implementations.
pub mod commands;
// Environment variables
const DISPLAY_LEVEL: bool = true;
const DISPLAY_LINE_NUMBER: bool = false;
const DISPLAY_TARGET: bool = false;
const DISPLAY_THREAD_ID: bool = false;
const DISPLAY_THREAD_NAME: bool = true;
/// Show module output at TRACE while pinning verbose configuration dumps to INFO, so examples read as a workflow narrative instead of a debug dump.
const EXAMPLE_LOG: &str = "trace,xylem_lib::config=info,xylem_cli::commands::run=info,xylem_cli::output=trace";
const LOG_LEVEL: &str = "XYLEM_LOG_LEVEL";
/// Result returned by CLI entry points.
pub type Void = color_eyre::Result<()>;
/// Install the global tracing subscriber, preferring `XYLEM_LOG_LEVEL` from a dotenv file over `verbose`.
///
/// `filter` is an optional [EnvFilter] directive.
/// When set it takes precedence over `verbose`, which lets callers keep module output visible while pinning noisy internal targets to a quieter level.
///
/// `sanitize` escapes ANSI control characters inside logged values to guard against terminal injection.
/// It also escapes the color codes that [Label](acorn_host::terminal::Label) embeds in messages, so pass `false` to render those labels in color when every logged value is trusted.
pub fn init_tracing(env: Option<&PathBuf>, verbose: &Verbosity, filter: Option<&str>, sanitize: bool) -> Void {
    init_tracing_output(env, verbose, filter, sanitize, true, stderr)
}
fn init_tracing_output<W>(env: Option<&PathBuf>, verbose: &Verbosity, filter: Option<&str>, sanitize: bool, ansi: bool, writer: W) -> Void
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    load_dotenv(env).map(|dotenv_loaded| {
        let directive = dotenvy::var(LOG_LEVEL).ok().filter(|value| !value.is_empty());
        match directive.filter(|_| dotenv_loaded) {
            | Some(directive) => {
                let format = fmt::layer()
                    .with_ansi(ansi)
                    .with_level(DISPLAY_LEVEL)
                    .with_line_number(DISPLAY_LINE_NUMBER)
                    .with_target(DISPLAY_TARGET)
                    .with_thread_ids(DISPLAY_THREAD_ID)
                    .with_thread_names(DISPLAY_THREAD_NAME)
                    .with_writer(writer)
                    .compact();
                tracing_subscriber::registry().with(format).with(EnvFilter::new(directive)).init();
            }
            | None => {
                let format = fmt::format()
                    .with_ansi(ansi)
                    .with_level(DISPLAY_LEVEL)
                    .with_line_number(DISPLAY_LINE_NUMBER)
                    .with_target(DISPLAY_TARGET)
                    .with_thread_ids(DISPLAY_THREAD_ID)
                    .with_thread_names(DISPLAY_THREAD_NAME)
                    .without_time()
                    .compact();
                let filter = match filter {
                    | Some(directive) => EnvFilter::new(directive),
                    | None => EnvFilter::new(verbose.log_level_filter().as_trace().to_string()),
                };
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_ansi_sanitization(sanitize)
                    .with_writer(writer)
                    .event_format(format)
                    .init();
            }
        }
    })
}
/// Install tracing with plain-text events written to a caller-provided destination.
pub fn init_tracing_with_writer<W>(env: Option<&PathBuf>, verbose: &Verbosity, filter: Option<&str>, sanitize: bool, writer: W) -> Void
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    init_tracing_output(env, verbose, filter, sanitize, false, writer)
}
fn load_dotenv(path: Option<&PathBuf>) -> Result<bool> {
    match path {
        | Some(path) => dotenvy::from_path(path)
            .map(|_| true)
            .map_err(|why| eyre!("Could not load dotenv file {} — {why}", path.display())),
        | None => Ok(dotenvy::dotenv().is_ok()),
    }
}
/// Run a discoverable example configuration at a path relative to the workspace root.
///
/// Relative module URIs resolve against the process working directory, so this
/// changes into the configuration's directory before it is read. Progress bars
/// are disabled so the workflow output stays readable. Set `XYLEM_LOG_LEVEL` to
/// override the default log filter.
pub fn run_example(config: &str) -> Void {
    color_eyre::install()?;
    let filter = match dotenvy::var(LOG_LEVEL) {
        | Ok(value) if !value.is_empty() => value,
        | _ => EXAMPLE_LOG.to_string(),
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| eyre!("the CLI crate should be a member of the workspace"))?;
    let configuration = workspace.join(config);
    let directory = configuration
        .parent()
        .ok_or_else(|| eyre!("example configuration should have a parent directory"))?;
    let config = configuration
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("example configuration should have a file name"))?;
    set_current_dir(directory).map_err(|why| eyre!("use example directory {}: {why}", directory.display()))?;
    init_tracing(None, &Verbosity::default(), Some(filter.as_str()), false)?;
    match commands::run::run(
        &Some(config),
        &None,
        &[ReporterType::Console],
        &None,
        &None,
        &None,
        false,
        false,
        false,
        false,
        &None,
        false,
        false,
        false,
        None,
        false,
        Verbosity::default(),
        ExecutionLimits::default(),
        true,
        None,
        false,
    ) {
        | Ok(()) => Ok(()),
        | Err(failure) => {
            eprintln!("Error: {failure}");
            exit(failure.exit_code());
        }
    }
}
