use crate::util::{is_command, to_string, Command, Config, ModuleLanguage, ModuleType, ModuleUri, Script};
use log::info;
use serde::ser::StdError;
use std::path::PathBuf;
use uriparse::Scheme::{HTTP, HTTPS};

pub fn run(config: &Option<PathBuf>, _reporter: &Option<Vec<String>>, _output: &Option<PathBuf>) -> Result<(), Box<(dyn StdError + 'static)>> {
    let default_config: &str = "tests/fixtures/config.json";
    // let default_script: Script = Script {
    //     command: Command {
    //         name: "python".to_string(),
    //         conda_environment: None,
    //     },
    //     arguments: to_string(vec!["-m", "lib", "--input", "{{INPUT_DIRECTORY}}", "--output", "{{OUTPUT_DIRECTORY}}"]),
    // };
    let path = match config {
        | Some(value) => value.to_path_buf(),
        | None => PathBuf::from(default_config),
    };
    let cfg = Config::read_json(path);
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
                    panic!("{:?} not found", name)
                }
            });
            // Prepare modules
            cfg.modules.iter().for_each(|module| match &module.module_type {
                | ModuleType::Binary { uri } => match ModuleUri::from(uri) {
                    | ModuleUri::Url(value) => match value.scheme() {
                        | HTTP | HTTPS => {
                            println!("Remote: {:?}", value);
                        }
                        | _ => unimplemented!(),
                    },
                    | ModuleUri::File(value) => {
                        println!("Local Binary: {:?}", value);
                    }
                    | _ => unimplemented!(),
                },
                | ModuleType::Script { programming_language, uri } => match programming_language {
                    | ModuleLanguage::Python => match ModuleUri::from(uri) {
                        | ModuleUri::Url(value) => match value.scheme() {
                            | HTTP | HTTPS => {
                                println!("Remote: {:?}", value);
                            }
                            | _ => unimplemented!(),
                        },
                        | ModuleUri::File(value) => {
                            info!("Loading module: {:?}", value);
                            match value.try_exists() {
                                | Ok(true) => {
                                    info!("Module found: {:?}", value);
                                }
                                | _ => {
                                    info!("Module not found: {:?}", value);
                                }
                            }
                        }
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
                | ModuleType::Script { .. } => {}
                | _ => unimplemented!(),
            });
        }
        | Err(err) => println!("{:?}", err),
    };
    Ok(())
}

#[cfg(test)]
mod tests;
