use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use exitcode;
use nanoid::nanoid;
use pipe_lib::{
    download_binary, get_checksum, get_parent, Command, Config, EnvironmentValue, Label, ModuleLanguage, ModuleType, ModuleUri, Script,
    ScriptTemplate, TemplateAttribute,
};
use rayon::prelude::*;
use std::convert::TryFrom;
use std::env;
use std::fs;
#[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use uriparse::Scheme;
use uriparse::URI;

const DEFAULT_CONFIG_NAME: &str = "config.json";

#[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
fn make_executable(path: &PathBuf) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(windows)]
fn make_executable(path: &PathBuf) {
    // TODO: Add windows support...pass through?
}
fn check_requirements(config: &Config, skip: bool) {
    if !skip {
        config.requirements.iter().for_each(|name| {
            let command = Command::init().name(name.clone()).build();
            if !command.test() {
                error!(name, "=> {} Requirement not met", Label::not_found());
                std::process::exit(exitcode::UNAVAILABLE);
            }
        });
    } else {
        warn!("Skipping requirements verification");
    }
}
fn check_resources(_config: &Config, _skip: bool) {}

#[allow(clippy::too_many_arguments)]
pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    dry_run: bool,
    offline: bool,
    skip_verify_checksum: bool,
    skip_verify_requirements: bool,
    verbose: Verbosity,
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
    match Config::read(config_path) {
        | Some(cfg) => {
            check_requirements(&cfg, skip_verify_requirements);
            check_resources(&cfg, false);
            //
            // Prepare modules
            //
            cfg.modules.par_iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri, checksum } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) if !offline => match value.scheme() {
                        | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                            let url = value.to_string();
                            // TODO: Filter URL using whitelist
                            // TODO: Check that URL exists
                            debug!(module = module.name, url, "=> {} Remote binary", Label::found());
                            if !dry_run {
                                let output = download_binary(url.clone(), root.clone());
                                if !skip_verify_checksum {
                                    let calculated_checksum = match output.clone() {
                                        | Ok(path) => get_checksum(path),
                                        | _ => "".to_string(),
                                    };
                                    match checksum {
                                        | Some(value) if value == &calculated_checksum => {
                                            info!(module = module.name, url, "=> {} Checksum verified", Label::pass());
                                            if let Ok(path) = output {
                                                make_executable(&path);
                                            }
                                        }
                                        | Some(_) => {
                                            error!(
                                                module = module.name,
                                                url,
                                                checksum,
                                                calculated = calculated_checksum,
                                                "=> {} Remote binary checksum does not match",
                                                Label::fail()
                                            );
                                            std::process::exit(exitcode::USAGE);
                                        }
                                        | None => {
                                            error!(
                                                module = module.name,
                                                url,
                                                checksum,
                                                "=> {} Remote binary checksum does not match",
                                                Label::fail()
                                            );
                                            std::process::exit(exitcode::UNAVAILABLE);
                                        }
                                    }
                                    // TODO: Verify binary is executable (https://docs.rs/is_executable/latest/is_executable/)
                                } else {
                                    warn!("Skipping checksum verification");
                                }
                            } else {
                                info!(module = module.name, url, "=> {} Download remote binary", Label::dry_run());
                            }
                            unimplemented!("Remote binary modules are not supported yet");
                        }
                        | _ => unimplemented!("Only HTTP, HTTPS, and SSH are currently supported for remote binary modules"),
                    },
                    | ModuleUri::File(value) => match value.try_exists() {
                        | Ok(true) => {
                            let path = value.into_os_string().into_string().unwrap();
                            // TODO: Verify binary is executable (https://docs.rs/is_executable/latest/is_executable/)
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
                                // TODO: Add support for designating virtual environment manager
                                // let manager = match &module.template {
                                //     | Some(TemplateAttribute::Template(ScriptTemplate { manager, .. })) => manager,
                                //     | Some(TemplateAttribute::Path(_)) => &None,
                                //     | _ => &None,
                                // };
                                if let Some(script) = Script::maybe_create_virtual_environment(module_root, None, verbose.log_level()) {
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
                                // TODO: Add support for designating virtual environment manager
                                // let manager = match &module.template {
                                //     | Some(TemplateAttribute::Template(ScriptTemplate { manager, .. })) => manager,
                                //     | Some(TemplateAttribute::Path(_)) => &None,
                                //     | _ => &None,
                                // };
                                if let Some(script) = Script::maybe_create_virtual_environment(uri.to_string(), None, verbose.log_level()) {
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
                        let virtual_environment = match &module.template {
                            | Some(TemplateAttribute::Path(value)) => {
                                ScriptTemplate::from_path(PathBuf::from(&working_dir).join(value)).virtual_environment
                            }
                            | Some(TemplateAttribute::Template(value)) => value.virtual_environment.clone(),
                            | None => ScriptTemplate::resolve_python_virtual_environment(PathBuf::from(working_dir.clone())),
                        };
                        let envs = EnvironmentValue::get_from_config(&cfg, Some(module.clone()));
                        let script = Script::from_module(module.clone(), Some(root.clone()))
                            .expand_arguments_from(envs)
                            .with_current_dir(working_dir)
                            .with_virtual_environment(virtual_environment);
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
