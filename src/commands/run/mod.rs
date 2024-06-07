#![allow(dead_code)]
use serde::ser::StdError;
use std::path::PathBuf;

pub fn run(
    _path: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    _all: bool,
    _pass: bool,
    _minimal: bool,
    _unique: bool,
) -> Result<(), Box<(dyn StdError + 'static)>> {
    unimplemented!();
}

#[cfg(test)]
mod tests;
