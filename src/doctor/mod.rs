#![allow(dead_code)]
use log::info;
use std::fmt;
use std::path::PathBuf;
use std::process;

#[derive(Clone, Debug)]
pub struct Command<'a> {
    pub name: &'a str,
    pub conda_environment: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct Script<'a> {
    pub command: Command<'a>,
    pub arguments: Vec<&'a str>,
}
pub trait Run<'a> {
    fn run(&'a self) -> process::Output;
}
impl<'a> fmt::Display for Script<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl<'a> Run<'a> for Script<'a> {
    fn run(&'a self) -> process::Output {
        const ERROR_MESSAGE: &str = "[ERROR] => Failed to execute process";
        return process::Command::new(self.command.name)
            .args(self.arguments.clone().iter())
            .output()
            .expect(ERROR_MESSAGE);
    }
}
pub fn is_command(value: Command) -> bool {
    let script = if cfg!(target_os = "windows") {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda",
                    conda_environment: Some(name),
                },
                arguments: vec!["run", "--name", name, "pwsh", "-NoProfile", "-Command", "command", value.name],
            },
            | None => Script {
                command: Command {
                    name: "pwsh",
                    conda_environment: None,
                },
                arguments: vec!["-NoProfile", "-Command", "command", value.name],
            },
        }
    } else {
        match value.conda_environment {
            | Some(name) => Script {
                command: Command {
                    name: "conda",
                    conda_environment: Some(name),
                },
                arguments: vec!["run", "--name", name, "which", value.name],
            },
            | None => Script {
                command: Command {
                    name: "which",
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
