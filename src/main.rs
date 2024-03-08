//! Command line (multi)tool for parameterized validation of folder structure and file content
use clap::{Parser, Subcommand};
use dotenv::dotenv;
use env_logger::Env;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use rayon::prelude::*;
use std::env;
use std::io::{self, BufRead};

mod checks;
mod reporters;
mod util;

use checks::*;
use util::{is_stdin_piped, Parameters, Reporter, ReporterType, Rule, StdinIsPipedResult, Void};

// Environment variables
const GERBER_LOG_LEVEL: &str = "GERBER_LOG_LEVEL";
const GERBER_LOG_STYLE: &str = "GERBER_LOG_STYLE";
/// Multi-tool for verifying myriad aspects of data processing pipelines
///
███    ███ ██ ██████   █████  ███████ 
████  ████ ██ ██   ██ ██   ██ ██      
██ ████ ██ ██ ██   ██ ███████ ███████ 
██  ██  ██ ██ ██   ██ ██   ██      ██ 
██      ██ ██ ██████  ██   ██ ███████ 
///
///
#[derive(Debug, Parser)]
#[command(author, version, about, verbatim_doc_comment)]
struct Arguments {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Debug, Subcommand)]
#[command(long_about = None)]
enum Commands {
    /// Run checks on a given path to a directory
    ///
    ///     $> gerber check /path/to/folder
    ///
    ///     $> echo /path/to/folder | gerber check -pm
    ///
    ///     $> gerber check /path/to/folder --reporter json console --output results.json
    ///
    #[clap(verbatim_doc_comment)]
    Check {
        /// Path to check
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        path: Option<std::path::PathBuf>,
        /// Exclude certain files
        #[arg(short = 'e', long = "exclude", value_name = "STRING")]
        exclude: Option<String>,
        /// Reporter(s) to use to output the results - console (default) and/or json
        #[arg(short = 'r', long = "reporter", value_name = "STRING", num_args=0..=2)]
        reporter: Option<Vec<String>>,
        /// Destination path for reporters that use one
        #[arg(short = 'o', long = "output", value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
        output: Option<std::path::PathBuf>,
        /// Return all results that pass or fail check conditions
        #[arg(short = 'a', long = "all", value_name = "BOOL")]
        all: bool,
        /// Only return results that pass their associated check
        #[arg(short = 'p', long = "pass", value_name = "BOOL")]
        pass: bool,
        /// Report results in minimal format
        #[arg(short = 'm', long = "minimal", value_name = "BOOL")]
        minimal: bool,
        /// Deduplicate results
        #[arg(short = 'u', long = "unique", value_name = "BOOL")]
        unique: bool,
    },
}
fn main() -> Void {
    dotenv().ok();
    let env = Env::default()
        .filter_or(GERBER_LOG_LEVEL, "error")
        .write_style_or(GERBER_LOG_STYLE, "always");
    env_logger::init_from_env(env);
    let rules: Vec<_> = vec![
        // Rule definitions go here
    ];
    let pipeline_input: Option<String> = match is_stdin_piped() {
        StdinIsPipedResult::StdinIsPiped => {
            let stdin = io::stdin();
            let reader = stdin.lock();
            let lines = reader.lines();
            let result = lines.last().expect("Failed to read line")?;
            info!("Received input from stdin");
            Some(result)
        }
        StdinIsPipedResult::StdinIsNotPiped => None,
    };
    let args = Arguments::parse();
    let current_dir = env::current_dir()?;
    match &args.command {
        Some(Commands::Check {
            path,
            reporter,
            output,
            all,
            pass,
            minimal,
            unique,
            ..
        }) => {
            let mut reporters: Vec<Reporter> = vec![];
            let parameters = Parameters {
                output_path: output.clone(),
                all: *all,
                pass: *pass,
                minimal: *minimal,
                custom: None,
            };
            let default_reporter = Reporter {
                name: ReporterType::Console,
                run: reporters::console::run,
                parameters: Some(parameters.clone()),
            };
            match reporter {
                Some(values) => {
                    for value in values {
                        let name = value.to_uppercase();
                        info!("Using {} reporter", name);
                        match name.as_str() {
                            "CONSOLE" => reporters.push(default_reporter.clone()),
                            "JSON" => reporters.push(Reporter {
                                name: ReporterType::Json,
                                run: reporters::json::run,
                                parameters: Some(parameters.clone()),
                            }),
                            _ => {
                                todo!("{} reporter not yet implemented", name)
                            }
                        }
                    }
                }
                None => {
                    let name = default_reporter.name.to_string().to_uppercase();
                    info!("Using {} reporter", name);
                    reporters.push(default_reporter)
                }
            }
            let working_dir = match pipeline_input {
                Some(x) => x,
                None => match &path {
                    Some(x) => x.to_str().unwrap_or(".").to_string(),
                    None => current_dir.to_str().unwrap_or(".").to_string(),
                },
            };
            info!("Working directory: {}", working_dir);
            eprintln!("\n");
            let mut results = Vec::new();
            for rule in rules {
                let files: Vec<_> = glob(&format!("{}{}", working_dir, rule.expression))?.collect();
                let progress = ProgressBar::new(files.len() as u64).with_message(rule.name);
                let template = "{pos:>5}/{len:<5}{spinner:.cyan/blue} {bar:.cyan/blue} {msg:.gray}";
                progress.set_style(ProgressStyle::with_template(template)?);
                let output: Vec<_> = files
                    .par_iter()
                    .flat_map(|path| {
                        progress.inc(1);
                        let input = path.as_ref().unwrap();
                        (rule.run)(input, None)
                    })
                    .collect();
                progress.finish();
                results.extend(output);
            }
            eprintln!();
            if *minimal {
                results.sort();
            }
            if *unique {
                results.dedup();
            };
            reporters.par_iter().for_each(|reporter| {
                let _ = (reporter.run)(results.clone(), reporter.parameters.clone());
            });
            Ok(())
        }
        None => Ok(()),
    }
}
