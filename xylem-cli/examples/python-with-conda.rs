//! Run the Python with Conda example workflow.
//!
//!     cargo run --example python-with-conda
fn main() -> xylem_lib::Void {
    xylem_cli::run_example("examples/python-with-conda/config.json")
}
