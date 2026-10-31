use clap_verbosity_flag::Verbosity;
use core::sync::atomic::{AtomicBool, Ordering};
pub use failure::RunFailure;
use std::path::PathBuf;
use std::process;
use xylem_lib::config::FailurePolicy;
use xylem_lib::execution::ExecutionLimits;
use xylem_lib::run::RunCancellation;
use xylem_lib::script::ContainerCleanup;
use xylem_lib::ReporterType;

mod dispatch;
mod execution;
mod executor;
mod failure;
mod preparation;
mod progress;
mod reporting;
#[cfg(test)]
mod tests;

struct RunRequest<'a> {
    artifact_output: &'a Option<PathBuf>,
    cancellation: Option<RunCancellation>,
    cli_allow_insecure_http: bool,
    cli_allowed: &'a Option<Vec<String>>,
    cli_container_cleanup: Option<ContainerCleanup>,
    cli_dry_run: bool,
    cli_failure_policy: Option<FailurePolicy>,
    cli_gpu_monitor: bool,
    cli_input: &'a Option<Vec<String>>,
    cli_no_container_env: bool,
    cli_offline: bool,
    cli_resume: bool,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    config: &'a Option<PathBuf>,
    limits: ExecutionLimits,
    module_index: Option<usize>,
    no_progress: bool,
    report_output: &'a Option<PathBuf>,
    reporter: &'a [ReporterType],
    retain_workspace: bool,
    sandbox_policy: &'a Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignalAction {
    Cancel,
    Exit,
}
fn install_headless_signal_handler(cancellation: RunCancellation) -> Result<(), RunFailure> {
    let received = AtomicBool::new(false);
    ctrlc::set_handler(move || match signal_action(&received) {
        | SignalAction::Cancel => cancellation.force(),
        | SignalAction::Exit => process::exit(failure::CANCELLED_EXIT_CODE),
    })
    .map_err(|why| RunFailure::new(exitcode::OSERR, format!("install workflow signal handler — {why}")))
}
#[allow(clippy::too_many_arguments)]
/// Resolve, validate, plan, execute, journal, and report one workflow run.
pub fn run(
    config: &Option<PathBuf>,
    sandbox_policy: &Option<PathBuf>,
    reporter: &[ReporterType],
    report_output: &Option<PathBuf>,
    artifact_output: &Option<PathBuf>,
    cli_input: &Option<Vec<String>>,
    cli_dry_run: bool,
    cli_failure_policy: Option<FailurePolicy>,
    cli_gpu_monitor: bool,
    cli_offline: bool,
    cli_resume: bool,
    cli_allowed: &Option<Vec<String>>,
    cli_allow_insecure_http: bool,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    cli_container_cleanup: Option<ContainerCleanup>,
    cli_no_container_env: bool,
    _verbose: Verbosity,
    limits: ExecutionLimits,
    no_progress: bool,
    cancellation: Option<RunCancellation>,
    retain_workspace: bool,
    module_index: Option<usize>,
) -> Result<(), RunFailure> {
    let request = RunRequest {
        artifact_output,
        cancellation,
        cli_allow_insecure_http,
        cli_allowed,
        cli_container_cleanup,
        cli_dry_run,
        cli_failure_policy,
        cli_gpu_monitor,
        cli_input,
        cli_no_container_env,
        cli_offline,
        cli_resume,
        cli_skip_verify_checksum,
        cli_skip_verify_requirements,
        config,
        limits,
        module_index,
        no_progress,
        report_output,
        reporter,
        retain_workspace,
        sandbox_policy,
    };
    preparation::prepare(&request).and_then(|prepared| {
        match prepared.config.options.as_ref().and_then(|options| options.dry_run).unwrap_or_default() {
            | true => reporting::render_dry_run(&prepared.plan, reporter, report_output),
            | false => {
                let signals = match (request.cancellation.as_ref(), prepared.context.as_ref()) {
                    | (None, Some(context)) => install_headless_signal_handler(context.cancellation().clone()),
                    | (None, None) | (Some(_), _) => Ok(()),
                };
                signals.and_then(|()| execution::execute(&request, prepared))
            }
        }
    })
}
fn signal_action(received: &AtomicBool) -> SignalAction {
    match received.swap(true, Ordering::AcqRel) {
        | true => SignalAction::Exit,
        | false => SignalAction::Cancel,
    }
}
