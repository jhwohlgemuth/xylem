#![allow(dead_code)]
use pipe_lib::config::Parameters;
use pipe_lib::{Output, Void};

pub fn run(_: Output, _: Option<Parameters>) -> Void {
    Ok(())
}
