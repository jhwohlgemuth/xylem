//! Command line (multi)tool for parameterized validation of folder structure and file content
use clap::Parser;
use dotenv::dotenv;
use env_logger::Env;

mod cli;
mod commands;
mod reporters;
mod util;

use cli::{Arguments, Commands};
use util::Void;

// Environment variables
const PIPE_LOG_LEVEL: &str = "PIPE_LOG_LEVEL";
const PIPE_LOG_STYLE: &str = "PIPE_LOG_STYLE";
fn main() -> Void {
    let args = Arguments::parse();
    match dotenv() {
        | Ok(_) if !dotenv::var(PIPE_LOG_LEVEL).unwrap_or_default().is_empty() => {
            let env = Env::default().filter_or(PIPE_LOG_LEVEL, "error").write_style_or(PIPE_LOG_STYLE, "always");
            env_logger::init_from_env(env);
        }
        | _ => {
            env_logger::Builder::new().filter_level(args.verbose.log_level_filter()).init();
        }
    }
    match &args.command {
        | Some(Commands::Check {
            path,
            reporter,
            output,
            all,
            pass,
            minimal,
            unique,
            ..
        }) => commands::check::run(path, reporter, output, *all, *pass, *minimal, *unique),
        | Some(Commands::Doctor { .. }) => Ok(()),
        | Some(Commands::Run {
            config,
            reporter,
            output,
            dry_run,
            ..
        }) => commands::run::run(config, reporter, output, *dry_run),
        | None => Ok(()),
    }
}
