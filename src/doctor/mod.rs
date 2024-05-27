#![allow(dead_code)]
use log::info;
use std::path::PathBuf;
use std::process;

pub struct Command<'a> {
    pub name: &'a str,
    pub conda_environment: Option<&'a str>,
}
pub fn is_command(value: Command) -> bool {
    const ERROR_MESSAGE: &'static str = "[ERROR] => Failed to execute process";
    let output = if cfg!(target_os = "windows") {
        // TODO: pwsh -NoProfile -Command command value
        todo!()
    } else {
        match value.conda_environment {
            | Some(name) => process::Command::new("conda")
                .args(["run", "--name", name, "which"])
                .arg(value.name)
                .output()
                .expect(ERROR_MESSAGE),
            | None => process::Command::new("which").arg(value.name).output().expect(ERROR_MESSAGE),
        }
    };
    match output {
        | process::Output { stdout, .. } if stdout.len() > 0 => {
            let path = String::from_utf8_lossy(&stdout);
            info!("Command found at {:?}", path.trim());
            return PathBuf::from(path.trim()).exists();
        }
        | _ => false,
    }
}

#[cfg(test)]
mod tests;
