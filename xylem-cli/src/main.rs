use clap::Parser;
use std::process::exit;
use tracing::instrument;
use xylem_lib::Void;

use xylem_cli::cli::{Arguments, Commands};
use xylem_cli::{commands, init_tracing};

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    init_tracing(args.env.as_ref(), &args.verbose, None, true)?;
    match &args.command {
        | Some(Commands::Run {
            artifact_output,
            config,
            reporter,
            report_output,
            input,
            dry_run,
            offline,
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
