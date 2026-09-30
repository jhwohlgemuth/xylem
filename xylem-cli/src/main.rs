//! Xylem command-line executable.
use clap::Parser;
use std::process::exit;
use tracing::instrument;

use xylem_cli::cli::{Arguments, Commands};
use xylem_cli::{commands, init_tracing, Void};

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    init_tracing(args.env.as_ref(), &args.verbose, None, true)?;
    match &args.command {
        | Some(Commands::Run {
            artifact_output,
            container_cleanup,
            config,
            reporter,
            report_output,
            input,
            dry_run,
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
        ) {
            | Ok(()) => Ok(()),
            | Err(failure) => {
                eprintln!("Error: {failure}");
                exit(failure.exit_code());
            }
        },
        | None => Ok(()),
    }
}
