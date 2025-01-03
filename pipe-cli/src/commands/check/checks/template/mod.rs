use pipe_lib::config::Parameters;
use pipe_lib::Output;
use std::path::Path;

/// Process check on a given path to a directory
pub fn _run(_item: &Path, _: Option<Parameters>) -> Output {
    vec![]
}

#[cfg(test)]
mod tests {}
