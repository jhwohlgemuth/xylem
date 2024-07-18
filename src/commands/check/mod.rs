pub mod checks;

use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::ser::StdError;
use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use tracing::info;

use crate::reporters;
use crate::util::{is_stdin_piped, Parameters, Reporter, ReporterType, Rule, StdinIsPipedResult};
use checks::*;

pub fn run(
    path: &Option<PathBuf>,
    reporter: &Option<Vec<String>>,
    output: &Option<PathBuf>,
    all: bool,
    pass: bool,
    minimal: bool,
    unique: bool,
) -> Result<(), Box<(dyn StdError + 'static)>> {
    let rules: Vec<_> = vec![
        // Rule definitions go here
        Rule {
            name: "template",
            run: template::run,
            expression: r"/**/",
            parameters: None,
        },
    ];
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
                info!("Using {} reporter", name);
                match name.as_str() {
                    | "CONSOLE" => reporters.push(default_reporter.clone()),
                    | "JSON" => reporters.push(Reporter {
                        name: ReporterType::Json,
                        run: reporters::json::run,
                        parameters: Some(parameters.clone()),
                    }),
                    | _ => {
                        todo!("{} reporter not yet implemented", name)
                    }
                }
            }
        }
        | None => {
            let name = default_reporter.name.to_string().to_uppercase();
            info!("Using {} reporter", name);
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
    info!("Working directory: {}", working_dir);
    eprintln!("\n");
    let mut results = Vec::new();
    for rule in rules {
        let files: Vec<_> = glob(&format!("{}{}", working_dir, rule.expression)).unwrap().collect();
        let progress = ProgressBar::new(files.len() as u64).with_message(rule.name);
        let template = "{pos:>5}/{len:<5}{spinner:.cyan/blue} {bar:.cyan/blue} {msg:.gray}";
        progress.set_style(ProgressStyle::with_template(template).unwrap());
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
