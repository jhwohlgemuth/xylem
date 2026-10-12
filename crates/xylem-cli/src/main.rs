//! Xylem command-line executable.
#[cfg(feature = "tui")]
use acorn_host::config::resolve_config_path;
use clap::Parser;
#[cfg(feature = "tui")]
use clap_verbosity_flag::VerbosityFilter;
#[cfg(feature = "tui")]
use std::env;
#[cfg(feature = "tui")]
use std::fs::canonicalize;
#[cfg(feature = "tui")]
use std::io;
#[cfg(feature = "tui")]
use std::path::{Path, PathBuf};
use std::process::exit;
#[cfg(feature = "tui")]
use std::sync::mpsc;
#[cfg(feature = "tui")]
use std::thread;
use tracing::instrument;
#[cfg(feature = "tui")]
use xylem_cli::cli::DEFAULT_CONFIG_NAMES;
use xylem_cli::cli::{Arguments, Commands};
#[cfg(feature = "tui")]
use xylem_cli::init_tracing_with_writer;
use xylem_cli::{commands, init_tracing, Void};
#[cfg(feature = "tui")]
use xylem_lib::run::RunCancellation;

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    #[cfg(feature = "tui")]
    let dashboard_log = match &args.command {
        | Some(Commands::Tui { .. }) => Some(xylem_tui::DiagnosticLog::default()),
        | Some(Commands::Run { .. }) | None => None,
    };
    #[cfg(feature = "tui")]
    match dashboard_log.clone() {
        | Some(log) => {
            let filter = match args.verbose.filter() {
                | VerbosityFilter::Debug => "debug",
                | VerbosityFilter::Off => "off",
                | VerbosityFilter::Trace => "trace",
                | VerbosityFilter::Error | VerbosityFilter::Info | VerbosityFilter::Warn => "info",
            };
            init_tracing_with_writer(args.env.as_ref(), &args.verbose, Some(filter), false, move || log.clone())?;
        }
        | None => init_tracing(args.env.as_ref(), &args.verbose, None, true)?,
    }
    #[cfg(not(feature = "tui"))]
    init_tracing(args.env.as_ref(), &args.verbose, None, true)?;
    match &args.command {
        | Some(Commands::Run {
            artifact_output,
            container_cleanup,
            config,
            reporter,
            report_output,
            retain_workspace,
            input,
            dry_run,
            gpu_monitor,
            offline,
            no_container_env,
            resume,
            allowed,
            skip_verify_checksum,
            skip_verify_requirements,
            verbose,
            ..
        }) => match commands::run::run(
            config,
            &args.sandbox_policy,
            reporter,
            report_output,
            artifact_output,
            input,
            *dry_run,
            *gpu_monitor,
            *offline,
            *resume,
            allowed,
            *skip_verify_checksum,
            *skip_verify_requirements,
            *container_cleanup,
            *no_container_env,
            *verbose,
            args.threads,
            args.no_progress,
            None,
            *retain_workspace,
        ) {
            | Ok(()) => Ok(()),
            | Err(failure) => {
                eprintln!("Error: {failure}");
                exit(failure.exit_code());
            }
        },
        #[cfg(feature = "tui")]
        | Some(Commands::Tui { config }) => {
            let directory = env::current_dir()?;
            let (selected, working_directory) = tui_configuration_context(config.as_ref(), &directory)?;
            env::set_current_dir(working_directory)?;
            let limits = args.threads;
            let sandbox_policy = args.sandbox_policy.clone();
            xylem_tui::run_with_diagnostics(selected, dashboard_log.unwrap_or_default(), move |config| {
                let cancellation = RunCancellation::default();
                let child_cancellation = cancellation.clone();
                let cancel = cancellation.clone();
                let sandbox_policy = sandbox_policy.clone();
                let (sender, completion) = mpsc::channel();
                thread::spawn(move || {
                    // The dashboard renders the run bundle; CLI reporters would write into its alternate screen.
                    let result = commands::run::run(
                        &Some(config),
                        &sandbox_policy,
                        &[],
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
                        None,
                        false,
                        clap_verbosity_flag::Verbosity::default(),
                        limits,
                        true,
                        Some(child_cancellation),
                        false,
                    )
                    .map_err(|why| why.to_string());
                    let _ = sender.send(result);
                });
                xylem_tui::LaunchHandle::new(completion, move || cancel.request())
            })?;
            Ok(())
        }
        | None => Ok(()),
    }
}
#[cfg(feature = "tui")]
fn tui_configuration_context(config: Option<&PathBuf>, directory: &Path) -> io::Result<(Option<PathBuf>, PathBuf)> {
    let selected = match config {
        | Some(path) if path.is_file() => Ok(Some(path.clone())),
        | Some(path) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("configuration was not found: {}", path.display()),
        )),
        | None => Ok(resolve_config_path(None, directory, &DEFAULT_CONFIG_NAMES)),
    };
    selected.and_then(|selected| {
        selected.map_or_else(
            || Ok((None, directory.to_path_buf())),
            |path| {
                canonicalize(path).map(|path| {
                    let working_directory = path.parent().map_or_else(|| directory.to_path_buf(), Path::to_path_buf);
                    (Some(path), working_directory)
                })
            },
        )
    })
}

#[cfg(all(test, feature = "tui"))]
mod tests {
    use super::tui_configuration_context;
    use std::fs::{canonicalize, create_dir_all, remove_dir_all, write};
    use std::path::Path;

    #[test]
    fn test_tui_uses_the_configuration_directory_as_its_working_directory() {
        let root = std::env::temp_dir().join(format!("xylem-tui-context-{}", std::process::id()));
        create_dir_all(&root).expect("create TUI context directory");
        let configuration = root.join("workflow.json");
        write(&configuration, b"{}").expect("write TUI configuration");
        let (selected, working_directory) = tui_configuration_context(Some(&configuration), Path::new(".")).expect("resolve TUI context");
        assert_eq!(selected, Some(canonicalize(&configuration).expect("canonical configuration")));
        assert_eq!(working_directory, canonicalize(&root).expect("canonical working directory"));
        remove_dir_all(root).expect("remove TUI context directory");
    }
}
