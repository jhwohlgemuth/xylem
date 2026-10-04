//! Xylem command-line executable.
#[cfg(feature = "tui")]
use acorn_host::config::resolve_config_path;
use clap::Parser;
#[cfg(feature = "tui")]
use std::env;
#[cfg(feature = "tui")]
use std::io;
use std::process::exit;
#[cfg(feature = "tui")]
use std::sync::mpsc;
#[cfg(feature = "tui")]
use std::thread;
use tracing::instrument;
#[cfg(feature = "tui")]
use xylem_cli::cli::DEFAULT_CONFIG_NAMES;
use xylem_cli::cli::{Arguments, Commands};
use xylem_cli::{commands, init_tracing, Void};
#[cfg(feature = "tui")]
use xylem_lib::run::RunCancellation;

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    let filter = match &args.command {
        | Some(Commands::Run { .. }) | None => None,
        #[cfg(feature = "tui")]
        | Some(Commands::Tui { .. }) => Some("off"),
    };
    init_tracing(args.env.as_ref(), &args.verbose, filter, true)?;
    match &args.command {
        | Some(Commands::Run {
            artifact_output,
            container_cleanup,
            config,
            reporter,
            report_output,
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
            let selected = match config {
                | Some(path) if path.is_file() => Some(path.clone()),
                | Some(path) => {
                    return Err(io::Error::new(io::ErrorKind::NotFound, format!("configuration was not found: {}", path.display())).into())
                }
                | None => resolve_config_path(None, &directory, &DEFAULT_CONFIG_NAMES),
            };
            let limits = args.threads;
            xylem_tui::run(selected, move |config| {
                let cancellation = RunCancellation::default();
                let child_cancellation = cancellation.clone();
                let cancel = cancellation.clone();
                let (sender, completion) = mpsc::channel();
                thread::spawn(move || {
                    let result = commands::run::run(
                        &Some(config),
                        &[xylem_lib::ReporterType::Console],
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
