use crate::util::{
    create_conda_environment, get_conda_environment_name, is_command, to_string, Command, Config, EnvironmentValue, Label, ModuleLanguage,
    ModuleType, ModuleUri, Script, TemplateAttribute,
};
use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use nanoid::nanoid;
use rayon::prelude::*;
use std::env;
use std::path::PathBuf;
use tracing::{debug, error, warn};
use uriparse::Scheme;

pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
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
        | Ok(path) => path.join("config.json"),
        | _ => PathBuf::from("./config.json"),
    };
    let config_path = match config {
        | Some(value) => value.to_path_buf(),
        | None => default_config,
    };
    match Config::read(config_path) {
        | Some(cfg) => {
            // Check requirements
            cfg.requirements.iter().for_each(|name| {
                let command = Command::init().name(name.clone()).build();
                if !is_command(command) {
                    error!(name, "=> {} Requirement not met", Label::not_found());
                    std::process::exit(1);
                }
            });
            // Prepare modules
            cfg.modules.par_iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            let url = value.to_string();
                            debug!(module = module.name, url, "=> {} Remote binary", Label::found());
                            // TODO: Download binary to temp directory
                            // TODO: Verify binary checksum
                            unimplemented!()
                        }
                        | _ => unimplemented!(),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            debug!(module = module.name, path, "=> {} Local binary", Label::found());
                        }
                        | _ => {
                            let path = value.into_os_string().into_string().unwrap();
                            error!(module = module.name, path, "=> {} Local binary", Label::not_found());
                            std::process::exit(1);
                        }
                    },
                    | _ => unimplemented!(),
                },
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Python => match ModuleUri::from(uri) {
                        | ModuleUri::Url(value) => match value.scheme() {
                            | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                                debug!(uri, "=> {} Remote script repository", Label::found());
                                let module_uri = ModuleUri::from(uri);
                                let current_dir = root.clone().into_os_string().into_string().unwrap();
                                let command = Command::init().name("git".to_string()).build();
                                let arguments = to_string(vec!["clone", &uri, &module_uri.hash()]);
                                let script = Script::init().command(command).arguments(arguments).build().with_current_dir(current_dir);
                                if dry_run {
                                    warn!(module = module.name, script = script.to_string(), "=> {}", Label::dry_run());
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
                                let path = value.into_os_string().into_string().unwrap();
                                debug!(module = module.name, path, "=> {} Local script repository", Label::found());
                                // TODO: Check script repository with is_valid_pipe_module_python
                                create_conda_environment(uri, dry_run, verbose.log_level());
                            }
                            | _ => {
                                let path = value.into_os_string().into_string().unwrap();
                                error!(module = module.name, path, "=> {} Local script module", Label::not_found());
                                std::process::exit(1);
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
                        debug!(
                            module = module.name,
                            path = manifest_path.to_str().unwrap(),
                            "=> {} Manifest file",
                            Label::using()
                        );
                        let default_template_path = PathBuf::from(&working_dir).join("template.json");
                        let loaded = match &module.template {
                            | Some(attribute) => match attribute {
                                | TemplateAttribute::Path(value) => match PathBuf::from(value).try_exists() {
                                    | Ok(true) => {
                                        let path = PathBuf::from(&working_dir).join(value);
                                        debug!(module = module.name, path = path.to_str().unwrap(), "=> {} Template file", Label::using());
                                        Script::from_path(path)
                                    }
                                    | _ => {
                                        warn!(module = module.name, "=> {} Default template file", Label::using());
                                        Script::from_path(default_template_path)
                                    }
                                },
                                | TemplateAttribute::Template(value) => {
                                    debug!(module = module.name, "=> {} Template attribute", Label::using());
                                    Script::from_template(value.clone())
                                }
                            },
                            | None => match default_template_path.try_exists() {
                                | Ok(true) => {
                                    warn!(module = module.name, "=> {} Default template file", Label::using());
                                    Script::from_path(default_template_path)
                                }
                                | _ => {
                                    warn!(module = module.name, "=> {} Default Python script", Label::using());
                                    Script::python_default()
                                }
                            },
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
                            warn!(
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
                    | _ => unimplemented!(),
                },
                | _ => unimplemented!(),
            });
        }
        | None => error!("=> {} Could not import configuration", Label::fail()),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
