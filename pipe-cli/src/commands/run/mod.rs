use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use exitcode;
use nanoid::nanoid;
use pipe_lib::{
    download_binary, get_checksum, Config, ConfigOptions, EnvironmentValue, InputAttribute, Label, ModuleLanguage, ModuleType, ModuleUri, Script,
    ScriptTemplate, TemplateAttribute,
};
use rayon::prelude::*;
use std::env;
use std::fs;
#[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use uriparse::Scheme;

const DEFAULT_CONFIG_NAME: &str = "config.json";

#[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
fn make_executable(path: &PathBuf) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(windows)]
fn make_executable(path: &PathBuf) {
    // TODO: Add windows support...pass through?
}
/// Prepare workflow modules for execution
///
/// Module preparation is done in parallel and includes the following steps:
/// - Download binary files and verify file integrity
/// - Git clone script projects and setup virtual environments
/// - Validate local and downloaded remote modules are ready for execution
///
fn prepare_modules(config: &Config, options: &ConfigOptions) {
    config.modules.par_iter().for_each(|module| match &module.module_type {
        | ModuleType::Binary { uri, checksum } => match ModuleUri::from(uri) {
            | ModuleUri::Url(value) if !options.offline => match value.scheme() {
                | Scheme::HTTP | Scheme::HTTPS | Scheme::SSH => {
                    let url = value.to_string();
                    // TODO: Filter URL using whitelist
                    // TODO: Check that URL exists
                    debug!(module = module.name, url, "=> {} Remote binary", Label::found());
                    if !options.dry_run {
                        let output = download_binary(url.clone(), options.root.clone());
                        if !options.skip_verify_checksum {
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
                            warn!(module = module.name, url, "=> {} Skipping checksum verification", Label::pass());
                        }
                    } else {
                        info!(module = module.name, url, "=> {} Download remote binary", Label::dry_run());
                    }
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
                | ModuleUri::Url(value) if !options.offline => match value.scheme() {
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
                | ModuleUri::Url(value) if !options.offline => match value.scheme() {
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
                        let current_dir = options.root.clone().into_os_string().into_string().unwrap();
                        let module_root = options.root.join(ModuleUri::from(uri).hash()).into_os_string().into_string().unwrap();
                        if let Some(script) = Script::maybe_git_clone(uri.clone(), current_dir, options.verbose.log_level()) {
                            if options.dry_run {
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
                        if let Some(script) = Script::maybe_create_virtual_environment(module_root, None, options.verbose.log_level()) {
                            if options.dry_run {
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
                        if let Some(script) = Script::maybe_create_virtual_environment(uri.to_string(), None, options.verbose.log_level()) {
                            if options.dry_run {
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
}
/// Run workflow modules
///
/// Modules are run in the order they are defined in the configuration
///
fn execute_modules(config: &Config, options: &ConfigOptions) {
    config.modules.iter().for_each(|module| match &module.module_type {
        | ModuleType::Binary { .. } => {
            let envs = EnvironmentValue::get_from_config(config, Some(module.clone()));
            let working_dir = options.root.clone().into_os_string().into_string().unwrap();
            let loaded = Script::from_module(module.clone(), Some(PathBuf::from(working_dir.clone())));
            let script = loaded.ensure_relative().expand_arguments_from(envs).with_current_dir(working_dir);
            if options.dry_run {
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
                let working_dir = ModuleUri::from(uri).working_directory(Some(options.root.clone()));
                let loaded = Script::from_module(module.clone(), Some(options.root.clone()));
                let envs = EnvironmentValue::get_from_config(config, Some(module.clone()));
                let script = loaded.expand_arguments_from(envs).with_current_dir(working_dir);
                if options.dry_run {
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
                let working_dir = ModuleUri::from(uri).working_directory(Some(options.root.clone()));
                let virtual_environment = match &module.template {
                    | Some(TemplateAttribute::Path(value)) => ScriptTemplate::from_path(PathBuf::from(&working_dir).join(value)).virtual_environment,
                    | Some(TemplateAttribute::Template(value)) => value.virtual_environment.clone(),
                    | None => ScriptTemplate::resolve_python_virtual_environment(PathBuf::from(working_dir.clone())),
                };
                let envs = EnvironmentValue::get_from_config(config, Some(module.clone()));
                let script = Script::from_module(module.clone(), Some(options.root.clone()))
                    .expand_arguments_from(envs)
                    .with_current_dir(working_dir)
                    .with_virtual_environment(virtual_environment);
                if options.dry_run {
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
    let options = ConfigOptions::init()
        .root(root.clone())
        .dry_run(dry_run)
        .offline(offline)
        .skip_verify_checksum(skip_verify_checksum)
        .skip_verify_requirements(skip_verify_requirements)
        .verbose(verbose.clone())
        .build();
    match Config::read(config_path) {
        | Some(cfg) => {
            cfg.clone().verify_requirements(&options).verify_resources(&options);
            prepare_modules(&cfg, &options);
            let input = match cfg.clone().input {
                | Some(value) => match value {
                    | InputAttribute::Path(_path) => {
                        // TODO: Verify path points to directory and return Vec<String> of children
                        Some(vec![])
                    }
                    | InputAttribute::List(value) => Some(value),
                    | InputAttribute::ShellScript(_script) => {
                        // TODO: Execute shell script and return Vec<String> of files (or folders)
                        unimplemented!("Shell script workflow input");
                    }
                },
                | None => None,
            };
            match input {
                | Some(value) => value.iter().for_each(|path| {
                    println!("Running workflow on {}", path);
                    // TODO: Run workflow with path as INPUT_DIRECTORY
                }),
                | None => {
                    info!("=> {} Configuration with no inputs", Label::using());
                    execute_modules(&cfg, &options);
                }
            };
        }
        | None => error!("=> {} Could not import configuration", Label::fail()),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
