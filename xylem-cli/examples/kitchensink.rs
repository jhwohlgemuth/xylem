//! Run the kitchensink example workflow.
//!
//!     cargo run --example kitchensink
use color_eyre::eyre::eyre;
use std::path::Path;
use std::process::Command;

fn build_binary() -> xylem_cli::Void {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| eyre!("the CLI crate should be a member of the workspace"))
        .and_then(|workspace| {
            let binary = workspace.join("examples/kitchensink/binary/finalize");
            let source = binary.with_extension("rs");
            Command::new("rustc")
                .args(["--edition=2024", "-O", "-o"])
                .arg(&binary)
                .arg(source)
                .status()
                .map_err(|why| eyre!("build Kitchensink binary: {why}"))
                .and_then(|status| match status.success() {
                    | true => Ok(()),
                    | false => Err(eyre!("build Kitchensink binary: rustc exited with {status}")),
                })
        })
}

fn main() -> xylem_cli::Void {
    build_binary().and_then(|()| xylem_cli::run_example("examples/kitchensink/.xylem.json"))
}
