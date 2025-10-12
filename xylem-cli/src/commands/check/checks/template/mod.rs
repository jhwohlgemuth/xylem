use std::path::Path;
use xylem_lib::config::Parameters;
use xylem_lib::Output;

/// Process check on a given path to a directory
pub fn _run(_item: &Path, _: Option<Parameters>) -> Output {
    vec![]
}

#[cfg(test)]
mod tests {}
