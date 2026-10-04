//! Run the local, dependency-free smoke workflow.
//!
//!     cargo run --example offline-smoke
fn main() -> xylem_cli::Void {
    xylem_cli::run_example("examples/offline-smoke/.xylem.json")
}
