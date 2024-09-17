use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use exitcode;
use nanoid::nanoid;
use pipe_lib::{get_conda_environment_name, get_parent, Command, Config, EnvironmentValue, Label, ModuleLanguage, ModuleType, ModuleUri, Script};
use rayon::prelude::*;
use std::convert::TryFrom;
use std::env;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use uriparse::Scheme;
use uriparse::URI;

const DEFAULT_CONFIG_NAME: &str = "config.json";

pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
    offline: bool,
    _skip_verify_checksum: bool,
    skip_verify_requirements: bool,
    verbose: Verbosity,
) -> Result<(), Report> {
    let id = nanoid!(10, &nanoid::alphabet::SAFE);
    let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
    match std::fs::create_dir_all(root.clone()) {
        | Ok(_) => {
            let path = root.clone().into_os_string().into_string().unwrap();
            debug!(path, "Created temporary directory");
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
    match Config::read(config_path) {
        | Some(cfg) => {
            // Check requirements
            if !skip_verify_requirements {
                cfg.requirements.iter().for_each(|name| {
                    let command = Command::init().name(name.clone()).build();
                    if !command.test() {
                        error!(name, "=> {} Requirement not met", Label::not_found());
                        std::process::exit(exitcode::UNAVAILABLE);
                    }
                });
            } else {
                debug!("Skipping requirements verification");
            }
            // Prepare modules
            cfg.modules.par_iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri, .. } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) if !offline => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            let url = value.to_string();
                            debug!(module = module.name, url, "=> {} Remote binary", Label::found());
                            // TODO: Filter URL using whitelist
                            // TODO: Check that URL exists
                            // TODO: Download binary to temp directory
                            // TODO: Verify binary checksum
                            unimplemented!("Remote binary modules are not supported yet");
                        }
                        | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote binary modules"),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            debug!(module = module.name, path, "=> {} Local binary", Label::found());
                        }
                        | _ => {
                            let path = value.into_os_string().into_string().unwrap();
                            error!(module = module.name, path, "=> {} Local binary", Label::not_found());
                            std::process::exit(exitcode::UNAVAILABLE);
                        }
                    },
                    | _ => unimplemented!("Only URLs and files are currently supported for binary modules"),
                },
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Generic => match ModuleUri::from(uri) {
                        | ModuleUri::Url(value) if !offline => match value.scheme() {
                            | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                                debug!(uri, "=> {} Remote script repository", Label::found());
                                // TODO: Filter URL using whitelist
                                // TODO: Check that URL exists
                                // TODO: Git clone module
                                // TODO: Check script repository with is_valid_pipe_module_python
                                unimplemented!("Remote Generic script modules are not supported yet");
                            }
                            | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote generic script modules"),
                        },
                        | ModuleUri::File(value) => match value.try_exists() {
                            | Ok(true) => {
                                let path = value.into_os_string().into_string().unwrap();
                                debug!(module = module.name, path, "=> {} Local script repository", Label::found());
                                // TODO: Check script repository with is_valid_pipe_module_python
                            }
                            | _ => {
                                let path = value.into_os_string().into_string().unwrap();
                                error!(module = module.name, path, "=> {} Local script module", Label::not_found());
                                std::process::exit(exitcode::UNAVAILABLE);
                            }
                        },
                        | _ => unimplemented!("Only URLs and files are currently supported for Generic script modules"),
                    },
                    | ModuleLanguage::Python => match ModuleUri::from(uri) {
                        | ModuleUri::Url(value) if !offline => match value.scheme() {
                            | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                                debug!(uri, "=> {} Remote {} script repository", Label::found(), programming_language);
                                // TODO: Filter URL using whitelist
                                // TODO: Check that URL exists
                                if ModuleUri::from(uri).test() {
                                    info!(module = module.name, uri, "=> {}", Label::found());
                                } else {
                                    error!(module = module.name, uri, "=> {}", Label::rejected());
                                    std::process::exit(exitcode::USAGE);
                                };
                                let current_dir = root.clone().into_os_string().into_string().unwrap();
                                let module_root = root.join(ModuleUri::from(uri).hash()).into_os_string().into_string().unwrap();
                                if let Some(script) = Script::maybe_git_clone(uri.clone(), current_dir, verbose.log_level()) {
                                    if dry_run {
                                        let cwd = script.clone().current_dir;
                                        warn!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                    } else {
                                        script.run();
                                    }
                                }
                                // TODO: Check script repository with is_valid_pipe_module_python
                                if let Some(script) = Script::maybe_create_conda_environment(module_root, verbose.log_level()) {
                                    if dry_run {
                                        let cwd = script.clone().current_dir;
                                        info!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                    } else {
                                        script.run();
                                    }
                                };
                            }
                            | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for Python script modules"),
                        },
                        | ModuleUri::File(value) => match value.try_exists() {
                            | Ok(true) => {
                                let path = value.into_os_string().into_string().unwrap();
                                debug!(
                                    module = module.name,
                                    path,
                                    "=> {} Local {} script repository",
                                    Label::found(),
                                    programming_language
                                );
                                // TODO: Check script repository with is_valid_pipe_module_python
                                if let Some(script) = Script::maybe_create_conda_environment(uri.to_string(), verbose.log_level()) {
                                    if dry_run {
                                        let cwd = script.clone().current_dir;
                                        info!(module = module.name, script = script.to_string(), cwd, "=> {}", Label::dry_run());
                                    } else {
                                        script.run();
                                    }
                                };
                            }
                            | _ => {
                                let path = value.into_os_string().into_string().unwrap();
                                error!(
                                    module = module.name,
                                    path,
                                    "=> {} Local {} script module",
                                    Label::not_found(),
                                    programming_language
                                );
                                std::process::exit(exitcode::UNAVAILABLE);
                            }
                        },
                        | _ => unimplemented!("Only URLs and files are currently supported for Python script modules"),
                    },
                    | _ => unimplemented!("Only Generic and Python script modules are currently supported"),
                },
                | _ => unimplemented!("Only binary and script modules are currently supported"),
            });
            // Run workflow
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { ref uri, .. } => {
                    let envs = EnvironmentValue::get_from_config(&cfg, Some(module.clone()));
                    let parent = get_parent(PathBuf::from(uri)).to_string_lossy().to_string();
                    let working_dir = URI::try_from(parent.as_str()).unwrap().clone().path().to_string();
                    let loaded = Script::from_module(module.clone(), Some(PathBuf::from(working_dir.clone())));
                    let script = loaded.expand_arguments_from(envs).with_current_dir(working_dir);
                    if dry_run {
                        EnvironmentValue::print_all();
                        info!(
                            module = module.name,
                            script = script.to_string(),
                            cwd = script.current_dir,
                            "=> {}",
                            Label::dry_run()
                        );
                    } else {
                        script.run();
                    }
                }
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Generic => {
                        let working_dir = ModuleUri::from(uri).working_directory(Some(root.clone()));
                        let loaded = Script::from_module(module.clone(), Some(root.clone()));
                        let envs = EnvironmentValue::get_from_config(&cfg, Some(module.clone()));
                        let script = loaded.expand_arguments_from(envs).with_current_dir(working_dir);
                        if dry_run {
                            EnvironmentValue::print_all();
                            info!(
                                module = module.name,
                                script = script.to_string(),
                                cwd = script.current_dir,
                                "=> {}",
                                Label::dry_run()
                            );
                        } else {
                            script.run();
                        }
                    }
                    | ModuleLanguage::Python => {
                        let working_dir = ModuleUri::from(uri).working_directory(Some(root.clone()));
                        let manifest_path = PathBuf::from(&working_dir).join("environment.yml");
                        debug!(
                            module = module.name,
                            path = manifest_path.to_str().unwrap(),
                            "=> {} Manifest file",
                            Label::using()
                        );
                        let loaded = Script::from_module(module.clone(), Some(root.clone()));
                        let environment = match get_conda_environment_name(manifest_path) {
                            | Some(value) => Some(value),
                            | None => match &module.template {
                                | Some(_) => loaded.clone().command.virtual_environment,
                                | None => None,
                            },
                        };
                        let envs = EnvironmentValue::get_from_config(&cfg, Some(module.clone()));
                        let script = loaded
                            .expand_arguments_from(envs)
                            .with_current_dir(working_dir)
                            .with_environment(environment);
                        if dry_run {
                            EnvironmentValue::print_all();
                            info!(
                                module = module.name,
                                script = script.to_string(),
                                cwd = script.current_dir,
                                "=> {}",
                                Label::dry_run()
                            );
                        } else {
                            script.run();
                        }
                    }
                    | _ => unimplemented!("Only Generic and Python script modules are currently supported"),
                },
                | _ => unimplemented!("Only binary and script modules are currently supported"),
            });
        }
        | None => error!("=> {} Could not import configuration", Label::fail()),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
