#![allow(dead_code)]
//! Python script modules should have necessary files and folder structure
//!
//! ### Example:
//! ```diff
//! + some-module/
//! +    ├── template.json
//! -    └── environment.yml
//! ```

use pipe_lib::config::Parameters;
use pipe_lib::Output;
use std::path::Path;

/// Process check on a given path to a directory
pub fn run(_item: &Path, _: Option<Parameters>) -> Output {
    vec![]
}

#[cfg(test)]
mod tests {}
