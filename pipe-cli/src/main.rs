use clap::Parser;
use dotenv::dotenv;
use pipe_lib::Void;
use tracing::instrument;
use tracing_log::AsTrace;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod reporters;

use cli::{Arguments, Commands};

// Environment variables
const LOG_LEVEL: &str = "PIPE_LOG_LEVEL";
const DISPLAY_LEVEL: bool = true;
const DISPLAY_LINE_NUMBER: bool = false;
const DISPLAY_TARGET: bool = false;
const DISPLAY_THREAD_ID: bool = false;
const DISPLAY_THREAD_NAME: bool = true;

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    match dotenv() {
        | Ok(_) if !dotenv::var(LOG_LEVEL).unwrap_or_default().is_empty() => {
            // Configure tracing with a .env file
            let format = fmt::layer()
                .with_level(DISPLAY_LEVEL)
                .with_line_number(DISPLAY_LINE_NUMBER)
                .with_target(DISPLAY_TARGET)
                .with_thread_ids(DISPLAY_THREAD_ID)
                .with_thread_names(DISPLAY_THREAD_NAME)
                .compact();
            tracing_subscriber::registry().with(format).with(EnvFilter::from_env(LOG_LEVEL)).init();
        }
        | _ => {
            // Configure tracing with command line argument
            let format = fmt::format()
                .with_level(DISPLAY_LEVEL)
                .with_line_number(DISPLAY_LINE_NUMBER)
                .with_target(DISPLAY_TARGET)
                .with_thread_ids(DISPLAY_THREAD_ID)
                .with_thread_names(DISPLAY_THREAD_NAME)
                .without_time()
                .compact();
            tracing_subscriber::fmt()
                .with_max_level(args.verbose.log_level_filter().as_trace())
                .event_format(format)
                .init();
        }
    }
    match &args.command {
        | Some(Commands::Check {
            path,
            config,
            reporter,
            output,
            all,
            pass,
            minimal,
            unique,
            ..
        }) => commands::check::run(path, config, reporter, output, *all, *pass, *minimal, *unique),
        | Some(Commands::Doctor { .. }) => Ok(()),
        | Some(Commands::Run {
            config,
            reporter,
            output,
            dry_run,
            verbose,
            ..
        }) => commands::run::run(config, reporter, output, *dry_run, verbose.clone()),
        | None => Ok(()),
    }
}
