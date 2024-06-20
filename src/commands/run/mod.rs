use crate::util::{is_command, to_string, Command, Config, ModuleLanguage, ModuleType, ModuleUri, Script, PREFIX};
use log::{error, info};
use nanoid::nanoid;
use serde::ser::StdError;
use std::env;
use std::path::PathBuf;
use uriparse::Scheme::{HTTP, HTTPS};

pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), Box<(dyn StdError + 'static)>> {
    let id = nanoid!(10, &nanoid::alphabet::SAFE);
    let root = env::temp_dir().join(PREFIX).join(id.clone());
    let default_config = match env::current_dir() {
        | Ok(path) => path.join("config.json"),
        | _ => PathBuf::from("./config.json"),
    };
    let config_path = match config {
        | Some(value) => value.to_path_buf(),
        | None => PathBuf::from(default_config),
    };
    let cfg = Config::read_json(config_path);
    match cfg {
        | Ok(cfg) => {
            info!("=> [INFO] Loaded config:{:?}", cfg);
            // Check requirements
            cfg.requirements.iter().for_each(|name| {
                let command = Command {
                    name: name.clone(),
                    conda_environment: None,
                };
                if !is_command(command) {
                    error!("Requirement not met. {:?} not found", name);
                }
            });
            // Prepare modules
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) => match value.scheme() {
                        | HTTP | HTTPS => {
                            info!("Identified remote binary: {:?}", value);
                            // TODO: Download binary to temp directory
                        }
                        | _ => unimplemented!(),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            info!("Found local binary: {:?}", value);
                        }
                        | _ => {
                            error!("Unable to find local binary: {:?}", value);
                        }
                    },
                    | _ => unimplemented!(),
                },
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Python => match ModuleUri::from(uri) {
                        | ModuleUri::Url(value) => match value.scheme() {
                            | HTTP | HTTPS => {
                                info!("Identified remote script repository: {:?}", value);
                                // TODO: Download/clone script repository to temp directory
                                // TODO: Check script repository with is_valid_pipe_module_python
                                // TODO: Create virtual environment
                            }
                            | _ => unimplemented!(),
                        },
                        | ModuleUri::File(value) => match value.try_exists() {
                            | Ok(true) => {
                                info!("Found local script module: {:?}", value);
                                // TODO: Check script repository with is_valid_pipe_module_python
                                // TODO: Create virtual environment
                            }
                            | _ => {
                                error!("Unable to find local script module: {:?}", value);
                            }
                        },
                        | _ => unimplemented!(),
                    },
                    | _ => unimplemented!(),
                },
                | _ => unimplemented!(),
            });
            // Set envionment variables
            let _ = cfg.set_environment_variables();
            for (key, value) in env::vars() {
                println!("{key}: {value}");
            }
            // Run workflow
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { .. } => {}
                | ModuleType::Script { .. } => {
                    // TODO: Load template.json or use default_script
                    // TODO: Handle dry-run
                    // TODO: Run script module
                    if dry_run {
                        error!("Not implemented");
                    } else {
                        let arguments = to_string(vec![
                            "-m",
                            "lib",
                            "--directory",
                            "{{INPUT_DIRECTORY}}",
                            "--output",
                            "{{OUTPUT_DIRECTORY}}",
                        ]);
                        let script = Script::new("python".to_string(), arguments);
                        script.with_current_dir(&root.to_string_lossy()).run();
                    }
                }
                | _ => unimplemented!(),
            });
        }
        | Err(err) => error!("{:?}", err),
    };
    println!("ID: {:?}, Root: {:?}", id, root);
    Ok(())
}

#[cfg(test)]
mod tests;
