//! Run a module pulled directly from a local OCI image layout bundle.
//!
//!     cargo run --example oci-local
fn main() -> xylem_cli::Void {
    xylem_cli::run_example("examples/oci-local/.xylem.json")
}
