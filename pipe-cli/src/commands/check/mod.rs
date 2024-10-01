pub mod checks;

use color_eyre::eyre::{Report, Result};
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use pipe_lib::{is_stdin_piped, Config, Label, Parameters, Reporter, ReporterType, StdinIsPipedResult};
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::reporters;
use checks::*;

const DEFAULT_CONFIG_NAME: &str = "config.json";

#[allow(clippy::too_many_arguments)]
pub fn run(
    path: &Option<PathBuf>,
    config: &Option<PathBuf>,
    reporter: &Option<Vec<String>>,
    output: &Option<PathBuf>,
    all: bool,
    pass: bool,
    minimal: bool,
    unique: bool,
) -> Result<(), Report> {
    let default_config = match env::current_dir() {
        | Ok(path) => path.join(DEFAULT_CONFIG_NAME),
        | _ => PathBuf::from(DEFAULT_CONFIG_NAME),
    };
    let config_path = match config {
        | Some(value) => value.to_path_buf(),
        | None => {
            let path = default_config.to_str().unwrap();
            warn!(path, "=> {} Default configuration", Label::using());
            default_config
        }
    };
    let builtin_rules = HashMap::from([("is_valid_python_module", is_valid_python_module::run)]);
    let _rules = match Config::read(config_path) {
        | Some(cfg) => match cfg.check {
            | Some(value) => value.rules,
            | None => {
                error!("=> {} Check rules not configured", Label::not_found());
                std::process::exit(1);
            }
        },
        | None => {
            error!("=> {} Could not import configuration", Label::fail());
            std::process::exit(1);
        }
    };
    let mut reporters: Vec<Reporter> = vec![];
    let parameters = Parameters {
        output_path: output.clone(),
        all,
        pass,
        minimal,
        custom: None,
    };
    let default_reporter = Reporter {
        name: ReporterType::Console,
        run: reporters::console::run,
        parameters: Some(parameters.clone()),
    };
    match reporter {
        | Some(values) => {
            for value in values {
                let name = value.to_uppercase();
                info!(name, "=> {} Reporter", Label::using());
                match name.as_str() {
                    | "CONSOLE" => reporters.push(default_reporter.clone()),
                    | "JSON" => reporters.push(Reporter {
                        name: ReporterType::Json,
                        run: reporters::json::run,
                        parameters: Some(parameters.clone()),
                    }),
                    | _ => {
                        error!(name, "=> {} Not a valid reporter", Label::not_found());
                    }
                }
            }
        }
        | None => {
            let name = default_reporter.name.to_string().to_uppercase();
            info!(name, "=> {} Reporter", Label::using());
            reporters.push(default_reporter)
        }
    }
    let pipeline_input: Option<String> = match is_stdin_piped() {
        | StdinIsPipedResult::StdinIsPiped => {
            let stdin = io::stdin();
            let reader = stdin.lock();
            let lines = reader.lines();
            let result = lines.last().expect("Failed to read line").unwrap();
            info!("Received input from stdin");
            Some(result)
        }
        | StdinIsPipedResult::StdinIsNotPiped => None,
    };
    let current_dir = env::current_dir().unwrap();
    let working_dir = match pipeline_input {
        | Some(x) => x,
        | None => match &path {
            | Some(x) => x.to_str().unwrap_or(".").to_string(),
            | None => current_dir.to_str().unwrap_or(".").to_string(),
        },
    };
    info!(path = working_dir, "=> {} Working directory", Label::using());
    eprintln!("\n");
    let mut results = Vec::new();
    for rule in builtin_rules {
        let files: Vec<_> = glob(&format!("{}{}", working_dir, r"/**/")).unwrap().collect();
        let progress = ProgressBar::new(files.len() as u64).with_message(rule.0);
        let template = "{pos:>5}/{len:<5}{spinner:.cyan/blue} {bar:.cyan/blue} {msg:.gray}";
        progress.set_style(ProgressStyle::with_template(template).unwrap());
        let output: Vec<_> = files
            .par_iter()
            .flat_map(|path| {
                progress.inc(1);
                let input = path.as_ref().unwrap();
                (rule.1)(input, None)
            })
            .collect();
        progress.finish();
        results.extend(output);
    }
    eprintln!();
    if minimal {
        results.sort();
    }
    if unique {
        results.dedup();
    };
    reporters.par_iter().for_each(|reporter| {
        let _ = (reporter.run)(results.clone(), reporter.parameters.clone());
    });
    Ok(())
}

#[cfg(test)]
mod tests;
