use pipe_lib::config::Parameters;
use pipe_lib::util::{is_stdout_piped, StdoutIsPipedResult};
use pipe_lib::{Details, Output, Void};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use tracing::{debug, info};

pub fn run(output: Output, parameters: Option<Parameters>) -> Void {
    eprintln!("\n");
    let all_results = match &parameters {
        | Some(Parameters { all, .. }) => all,
        | None => &false,
    };
    let only_passed = match &parameters {
        | Some(Parameters { pass, .. }) => pass,
        | None => &false,
    };
    let results: Vec<Details> = output
        .into_iter()
        .flatten()
        .filter(|x| *all_results || !(x.condition || *only_passed) || (x.condition && *only_passed))
        .collect();
    let json_string = serde_json::to_string_pretty(&results)?;
    debug!("{}", json_string);
    match is_stdout_piped() {
        | StdoutIsPipedResult::StdoutIsPiped => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(json_string.as_bytes())?;
            stdout.flush()?;
            drop(stdout);
        }
        | StdoutIsPipedResult::StdoutIsNotPiped => {
            let default_filename = "pipe_results.json";
            let default_dir = match env::var("PWD") {
                | Ok(pwd) => PathBuf::from(pwd).join(default_filename),
                | Err(_) => env::current_dir()?.with_file_name(default_filename),
            };
            let output_path = match parameters {
                | Some(Parameters { ref output_path, .. }) => {
                    if let Some(path) = output_path {
                        path
                    } else {
                        &default_dir
                    }
                }
                | None => &default_dir,
            };
            fs::write(output_path, &json_string)?;
            info!("Output saved to {}", output_path.to_str().unwrap());
        }
    }
    Ok(())
}
