use crate::util::{
    create_conda_environment, get_conda_environment_name, is_command, to_string, Command, Config, EnvironmentValue, ModuleLanguage, ModuleType,
    ModuleUri, Script, PREFIX,
};
use log::{error, info};
use nanoid::nanoid;
// use rayon::prelude::*;
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
        | None => default_config,
    };
    let cfg = Config::read_json(config_path);
    match cfg {
        | Ok(cfg) => {
            // Check requirements
            cfg.requirements.iter().for_each(|name| {
                let command = Command {
                    modifier: "".to_string(),
                    name: name.clone(),
                    virtual_environment: None,
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
                            unimplemented!()
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
                                unimplemented!()
                            }
                            | _ => unimplemented!(),
                        },
                        | ModuleUri::File(value) => match value.try_exists() {
                            | Ok(true) => {
                                // TODO: Check script repository with is_valid_pipe_module_python
                                create_conda_environment(uri, dry_run);
                            }
                            | _ => {
                                error!("Unable to find script module: {:?}", value);
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
            // Run workflow
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { .. } => {}
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Python => {
                        let working_dir = ModuleUri::from(uri).working_directory(None);
                        let manifest_path = PathBuf::from(&working_dir).join("environment.yml");
                        let template_path = match &module.template {
                            | Some(value) => PathBuf::from(&working_dir).join(value),
                            | None => PathBuf::from(&working_dir).join("template.json"),
                        };
                        let loaded = match template_path.try_exists() {
                            | Ok(true) => Script::from_template(template_path),
                            | _ => {
                                let command = "python".to_string();
                                let arguments = to_string(vec![
                                    "-m",
                                    "lib",
                                    "--directory",
                                    "{{ INPUT_DIRECTORY }}",
                                    "--output",
                                    "{{ OUTPUT_DIRECTORY }}",
                                ]);
                                Script::new(command, arguments)
                            }
                        };
                        let environment = match get_conda_environment_name(manifest_path) {
                            | Some(value) => Some(value),
                            | None => match &module.template {
                                | Some(_) => loaded.clone().command.virtual_environment,
                                | None => None,
                            },
                        };
                        let script = loaded.expand_arguments().with_current_dir(&working_dir).with_environment(environment);
                        if dry_run {
                            EnvironmentValue::print_all(PREFIX);
                            info!("=> [DRYRUN] Execute: {:?}", script.to_string());
                        } else {
                            // script.run();
                        }
                    }
                    | _ => unimplemented!(),
                },
                | _ => unimplemented!(),
            });
        }
        | Err(err) => error!("{:?}", err),
    };
    info!("ID: {:?}, Root: {:#?}", id, root);
    Ok(())
}

#[cfg(test)]
mod tests;
