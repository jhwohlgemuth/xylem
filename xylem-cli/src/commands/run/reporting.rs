use super::progress::WorkflowProgress;
use super::RunFailure;
use std::fs::{copy, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use xylem_lib::journal::{FinalStatus, RunJournal};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::plan::ExecutionPlan;
use xylem_lib::run::store::Bundle;
use xylem_lib::util::deduplicate;
use xylem_lib::ReporterType;

const CANCELLED_EXIT_CODE: i32 = 130;
pub(super) struct ReportContext<'a> {
    pub(super) bundle: &'a Bundle,
    pub(super) id: &'a str,
    pub(super) journal: &'a RunJournal,
    pub(super) journal_path: &'a Path,
    pub(super) progress: &'a WorkflowProgress,
    pub(super) report_output: &'a Option<PathBuf>,
    pub(super) reporters: &'a [ReporterType],
}
pub(super) fn finalize(context: ReportContext<'_>, dispatched: Result<Vec<Option<WorkflowOutcome>>, RunFailure>) -> Result<(), RunFailure> {
    let mut primary_failure = match &dispatched {
        | Ok(outcomes) => outcomes.iter().flatten().find(|outcome| !outcome.is_success()).map(|outcome| {
            let exit_code = match outcome.state.is_cancelled() {
                | true => CANCELLED_EXIT_CODE,
                | false => exitcode::SOFTWARE,
            };
            RunFailure::new(exit_code, outcome.error.clone().unwrap_or_else(|| "workflow input failed".to_string()))
        }),
        | Err(why) => Some(RunFailure::new(why.exit_code(), why.to_string())),
    };
    context.progress.finish_execution(primary_failure.is_none());
    let cancelled = dispatched
        .as_ref()
        .is_ok_and(|outcomes| outcomes.iter().flatten().any(|outcome| outcome.state.is_cancelled()));
    let final_status = match (cancelled, primary_failure.as_ref()) {
        | (true, _) => FinalStatus::Cancelled,
        | (false, Some(failure)) => FinalStatus::from(failure),
        | (false, None) => FinalStatus::Succeeded,
    };
    let final_error = primary_failure.as_ref().map(ToString::to_string);
    if let Err(why) = context.journal.finish(final_status, final_error.as_deref()) {
        primary_failure = Some(match primary_failure {
            | Some(failure) => failure.with_secondary(format!("finalize execution journal — {why}")),
            | None => RunFailure::new(exitcode::IOERR, format!("finalize execution journal — {why}")),
        });
    }
    let summary = match RunJournal::read(context.journal_path) {
        | Ok(summary) => Some(summary),
        | Err(why) => {
            primary_failure = Some(match primary_failure {
                | Some(failure) => failure.with_secondary(format!("recover execution journal — {why}")),
                | None => RunFailure::new(exitcode::IOERR, format!("recover execution journal — {why}")),
            });
            None
        }
    };
    if let Some(summary) = summary {
        if let Err(why) = context.bundle.write_summary(&summary) {
            primary_failure = Some(match primary_failure {
                | Some(failure) => failure.with_secondary(format!("write canonical run summary — {why}")),
                | None => RunFailure::new(exitcode::IOERR, format!("write canonical run summary — {why}")),
            });
        }
        let selected = deduplicate(context.reporters);
        if let Some(directory) = context.report_output {
            let copied = create_dir_all(directory)
                .and_then(|()| copy(context.journal_path, directory.join(format!("{}.journal.jsonl", context.id))))
                .map(|_| ());
            if let Err(why) = copied {
                primary_failure = Some(match primary_failure {
                    | Some(failure) => failure.with_secondary(format!("export execution journal — {why}")),
                    | None => RunFailure::new(exitcode::IOERR, format!("export execution journal — {why}")),
                });
            }
        }
        if should_write_journal_location(&selected) {
            let stderr = io::stderr();
            if let Err(why) = writeln!(stderr.lock(), "Journal: {}", context.journal_path.display()) {
                primary_failure = Some(match primary_failure {
                    | Some(failure) => failure.with_secondary(format!("write journal location — {why}")),
                    | None => RunFailure::new(exitcode::IOERR, format!("write journal location — {why}")),
                });
            }
        }
        let render = summary.render(&selected, context.report_output.as_deref(), context.journal_path);
        if let Err(why) = render {
            primary_failure = Some(match primary_failure {
                | Some(failure) => failure.with_secondary(format!("render run report — {why}")),
                | None => RunFailure::new(exitcode::IOERR, format!("render run report: {why}")),
            });
        }
    }
    match primary_failure {
        | Some(failure) => Err(failure),
        | None => Ok(()),
    }
}
pub(super) fn render_dry_run(plan: &ExecutionPlan, reporters: &[ReporterType], report_output: &Option<PathBuf>) -> Result<(), RunFailure> {
    match report_output {
        | Some(_) => Err(RunFailure::new(
            exitcode::USAGE,
            "--report-output cannot be used with a write-free dry run",
        )),
        | None => {
            let selected = deduplicate(reporters);
            selected.iter().try_for_each(|reporter| match reporter {
                | ReporterType::Console => {
                    println!("Dry-run execution plan");
                    plan.modules.iter().for_each(|module| {
                        println!(
                            "{} · {}: {}",
                            module.module_path.join("::"),
                            module.input.as_deref().unwrap_or("<no input>"),
                            module.decision
                        );
                    });
                    Ok(())
                }
                | ReporterType::Json => serde_json::to_writer_pretty(io::stdout().lock(), plan)
                    .map_err(|why| RunFailure::new(exitcode::IOERR, format!("render dry-run plan: {why}")))
                    .map(|()| println!()),
            })
        }
    }
}
fn should_write_journal_location(reporters: &[ReporterType]) -> bool {
    !reporters.is_empty() && !reporters.contains(&ReporterType::Console)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_run_does_not_write_journal_location() {
        assert!(!should_write_journal_location(&[]));
        assert!(!should_write_journal_location(&[ReporterType::Console]));
        assert!(should_write_journal_location(&[ReporterType::Json]));
    }
}
