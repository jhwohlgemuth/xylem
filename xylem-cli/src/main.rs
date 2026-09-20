use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use std::path::PathBuf;
use tracing::instrument;
use tracing_log::AsTrace;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use xylem_lib::Void;

mod cli;
mod commands;

use cli::{Arguments, Commands};

// Environment variables
const LOG_LEVEL: &str = "XYLEM_LOG_LEVEL";
const DISPLAY_LEVEL: bool = true;
const DISPLAY_LINE_NUMBER: bool = false;
const DISPLAY_TARGET: bool = false;
const DISPLAY_THREAD_ID: bool = false;
const DISPLAY_THREAD_NAME: bool = true;

#[instrument]
fn main() -> Void {
    color_eyre::install()?;
    let args = Arguments::parse();
    let dotenv_loaded = load_dotenv(args.env.as_ref())?;
    match dotenv_loaded {
        | true if !dotenvy::var(LOG_LEVEL).unwrap_or_default().is_empty() => {
            // Configure tracing with a .env file
            let format = fmt::layer()
                .with_ansi(true)
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
                .with_ansi(true)
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
        | Some(Commands::Run {
            config,
            reporter,
            output,
            input,
            dry_run,
            offline,
            allowed,
            skip_verify_checksum,
            skip_verify_requirements,
            verbose,
            ..
        }) => commands::run::run(
            config,
            reporter,
            output,
            input,
            *dry_run,
            *offline,
            allowed,
            *skip_verify_checksum,
            *skip_verify_requirements,
            *verbose,
            args.threads,
            args.no_progress,
        ),
        | None => Ok(()),
    }
}

fn load_dotenv(path: Option<&PathBuf>) -> Result<bool> {
    match path {
        | Some(path) => dotenvy::from_path(path)
            .map(|_| true)
            .map_err(|why| eyre!("Could not load dotenv file {} — {why}", path.display())),
        | None => Ok(dotenvy::dotenv().is_ok()),
    }
}
