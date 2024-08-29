use pipe_lib::{Output, Parameters};
use std::path::Path;

/// Process check on a given path to a directory
pub fn _run(_item: &Path, _: Option<Parameters>) -> Output {
    vec![]
}

#[cfg(test)]
mod tests {}
