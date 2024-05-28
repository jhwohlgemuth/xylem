#![allow(dead_code)]
use crate::util::{read_file, to_string};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::path::PathBuf;
use std::process;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Instruction {
    ShellFormat(String),
    ExecFormat(Vec<String>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModuleLanguage {
    Binary,
    Csharp,
    JavaScript,
    Julia,
    Python,
    Rust,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ModuleType {
    Script {
        language: ModuleLanguage,
        uri: String,
    },
    #[serde(alias = "wasm")]
    WebAssembly {
        run: String,
    },
}
#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub conda_environment: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    requirements: Vec<String>,
    tags: Option<Vec<String>>,
    modules: Vec<Module>,
    variables: Option<Map<String, Value>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Module {
    name: String,
    #[serde(flatten)]
    module_type: ModuleType,
    description: Option<String>,
    variables: Option<Map<String, Value>>,
    before_script: Option<Instruction>,
    after_script: Option<Instruction>,
    template: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Script {
    pub command: Command,
    pub arguments: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptTemplate {
    command: String,
    environment: Option<String>,
    arguments: Instruction,
}
impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl<'a> Script {
    pub fn new(command: String, arguments: Vec<String>) -> Script {
        Script {
            command: Command {
                name: command,
                conda_environment: None,
            },
            arguments,
        }
    }
    fn from_template(path: PathBuf) -> Script {
        println!("{:?}", path.display());
        let content = match read_file(path.clone()) {
            | Ok(value) => value,
            | Err(_) => {
                error!("Failed to read config file: {:?}", path);
                "".to_owned()
            }
        };
        let json: serde_json::Result<ScriptTemplate> = serde_json::from_str(&content);
        let template = json.expect("[ERROR] => Failed to parse script template").clone();
        let arguments = match template.arguments {
            | Instruction::ShellFormat(value) => value.split_whitespace().map(|s| s.to_owned()).collect::<Vec<_>>(),
            | Instruction::ExecFormat(values) => values,
        };
        Script::new(template.command, arguments)
    }
    fn run(&'a self) -> process::Output {
        const ERROR_MESSAGE: &str = "[ERROR] => Failed to execute process";
        return process::Command::new(self.command.name.clone())
            .args(self.arguments.clone().iter())
            .output()
            .expect(ERROR_MESSAGE);
    }
}
pub fn import_config(path: PathBuf) -> serde_json::Result<Config> {
    let content = match read_file(path.clone()) {
        | Ok(value) => value,
        | Err(_) => {
            error!("Failed to read config file: {:?}", path);
            "".to_owned()
        }
    };
    let json: serde_json::Result<Config> = serde_json::from_str(&content);
    info!("Imported config: {:?}", json);
    json
}
pub fn is_command(value: Command) -> bool {
    let script = if cfg!(target_os = "windows") {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda".to_owned(),
                    conda_environment: Some(name.clone()),
                },
                arguments: to_string(vec!["run", "--name", &name, "pwsh", "-NoProfile", "-Command", "command", &value.name]),
            },
            | None => Script {
                command: Command {
                    name: "pwsh".to_owned(),
                    conda_environment: None,
                },
                arguments: to_string(vec!["-NoProfile", "-Command", "command", &value.name]),
            },
        }
    } else {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda".to_owned(),
                    conda_environment: Some(name.clone()),
                },
                arguments: to_string(vec!["run", "--name", &name, "which", &value.name]),
            },
            | None => Script {
                command: Command {
                    name: "which".to_owned(),
                    conda_environment: None,
                },
                arguments: vec![value.name],
            },
        }
    };
    let output = script.run();
    match output {
        | process::Output { stdout, .. } if !stdout.is_empty() => {
            let path = String::from_utf8_lossy(&stdout);
            info!("Command found at {:?}", path.trim());
            return PathBuf::from(path.trim()).exists();
        }
        | _ => false,
    }
}

#[cfg(test)]
mod tests;
