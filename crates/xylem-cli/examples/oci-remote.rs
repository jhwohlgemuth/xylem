//! Run a script acquired from a digest-pinned Savannah OCI artifact.
//!
//!     cargo run --example oci-remote
fn main() -> xylem_cli::Void {
    xylem_cli::run_example("examples/oci-remote/.xylem.json")
}
