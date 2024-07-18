use crate::util::{
    create_conda_environment, get_conda_environment_name, is_command, to_string, Command, Config, EnvironmentValue, ModuleLanguage, ModuleType,
    ModuleUri, Script,
};
use clap_verbosity_flag::Verbosity;
use nanoid::nanoid;
use rayon::prelude::*;
use serde::ser::StdError;
use std::env;
use std::path::PathBuf;
use tracing::{debug, error, info};
use uriparse::Scheme;

pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
    verbose: Verbosity,
) -> Result<(), Box<(dyn StdError + 'static)>> {
    let id = nanoid!(10, &nanoid::alphabet::SAFE);
    let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
    match std::fs::create_dir_all(root.clone()) {
        | Ok(_) => {
            debug!("Created temporary directory: {:?}", root.clone().into_os_string().into_string().unwrap());
        }
        | Err(err) => error!("{}", err),
    }
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
            cfg.modules.par_iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            debug!("Found remote binary: {:?}", value);
                            // TODO: Download binary to temp directory
                            unimplemented!()
                        }
                        | _ => unimplemented!(),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            debug!("Found local binary: {:?}", value);
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
                            | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                                debug!("Found remote script repository: {:?}", uri);
                                let module_uri = ModuleUri::from(uri);
                                let current_dir = root.clone().into_os_string().into_string().unwrap();
                                let command = "git".to_string();
                                let arguments = to_string(vec!["clone", &uri, &module_uri.hash()]);
                                let script = Script::new(command, arguments).with_current_dir(current_dir);
                                if dry_run {
                                    info!("=> [DRYRUN] Execute: {:?}", script.to_string());
                                } else {
                                    script.run();
                                }
                                // TODO: Check script repository with is_valid_pipe_module_python
                                let module_root = root.join(module_uri.hash()).into_os_string().into_string().unwrap();
                                create_conda_environment(&module_root, dry_run, verbose.log_level());
                            }
                            | _ => unimplemented!(),
                        },
                        | ModuleUri::File(value) => match value.try_exists() {
                            | Ok(true) => {
                                debug!("Found local script repository: {:?}", value);
                                // TODO: Check script repository with is_valid_pipe_module_python
                                create_conda_environment(uri, dry_run, verbose.log_level());
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
            // Run workflow
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { .. } => {}
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Python => {
                        let working_dir = ModuleUri::from(uri).working_directory(Some(root.clone()));
                        let manifest_path = PathBuf::from(&working_dir).join("environment.yml");
                        let template_path = match &module.template {
                            | Some(value) => PathBuf::from(&working_dir).join(value),
                            | None => PathBuf::from(&working_dir).join("template.json"),
                        };
                        debug!("=> [INFO] Manifest path: {}", manifest_path.to_str().unwrap());
                        debug!("=> [INFO] Template path: {}", template_path.to_str().unwrap());
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
                        let config_vars = cfg.variables.clone().unwrap_or_default();
                        let module_vars = module.variables.clone().unwrap_or_default();
                        let mut envs = EnvironmentValue::get_from_serde(&config_vars);
                        envs.extend(EnvironmentValue::get_from_serde(&module_vars));
                        let script = loaded
                            .expand_arguments_from(envs)
                            .with_current_dir(working_dir)
                            .with_environment(environment);
                        if dry_run {
                            EnvironmentValue::print_all();
                            info!("=> [DRYRUN] Execute: {:?}", script.to_string());
                        } else {
                            script.run();
                        }
                    }
                    | _ => unimplemented!(),
                },
                | _ => unimplemented!(),
            });
        }
        | Err(err) => error!("{:?}", err),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
