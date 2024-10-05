use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use nanoid::nanoid;
use pipe_lib::{Config, ConfigOptions, EnvironmentValue, Label};
use rayon::prelude::*;
use std::env;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

const DEFAULT_CONFIG_NAME: &str = "config.json";

#[allow(clippy::too_many_arguments)]
pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
    offline: bool,
    skip_verify_checksum: bool,
    skip_verify_requirements: bool,
    _verbose: Verbosity,
) -> Result<(), Report> {
    let id = nanoid!(10, &nanoid::alphabet::SAFE);
    let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
    match std::fs::create_dir_all(root.clone()) {
        | Ok(_) => {
            let path = root.clone().into_os_string().into_string().unwrap();
            debug!(path, "=> {} Root directory", Label::using());
        }
        | Err(err) => error!("{}", err),
    }
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
    let options = ConfigOptions::init()
        .root(root.clone())
        .dry_run(dry_run)
        .offline(offline)
        .skip_verify_checksum(skip_verify_checksum)
        .skip_verify_requirements(skip_verify_requirements)
        .build();
    match Config::read(config_path) {
        | Some(cfg) => {
            let config = cfg.verify_requirements(&options).verify_resources(&options).prepare_modules(&options);
            match config.clone().get_inputs() {
                | Some(values) if !values.is_empty() => values.par_iter().for_each(|value| {
                    info!(value, "=> {} Configuration with input", Label::using());
                    config.clone().run_workflow(Some(value.to_string()), &options);
                }),
                | Some(_) | None => {
                    info!("=> {} Configuration with {}", Label::using(), Label::fmt_not_found("NO INPUTS"));
                    config.run_workflow(None, &options);
                }
            };
        }
        | None => error!("=> {} Could not import configuration", Label::fail()),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
