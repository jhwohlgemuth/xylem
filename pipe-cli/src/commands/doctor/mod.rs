#![allow(dead_code)]
use std::path::PathBuf;

use pipe_lib::CommandResult;

pub fn run(
    _path: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    _all: bool,
    _pass: bool,
    _minimal: bool,
    _unique: bool,
) -> CommandResult {
    unimplemented!();
}

#[cfg(test)]
mod tests;
