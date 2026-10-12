use super::executor::LoggedModuleExecutor;
use super::progress::WorkflowProgress;
use super::RunFailure;
use acorn_host::terminal::Label;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use std::path::Path;
use tracing::{debug, info, warn};
use xylem_lib::config::{Config, ConfigOptions};
use xylem_lib::container::ContainerLifecycle;
use xylem_lib::execution::ExecutionContext;
use xylem_lib::journal::{ContainerState, ExecutionState, JournalContainer, RunJournal};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::plan::{ExecutionPlan, MemoryPlanMode};
use xylem_lib::progress::ProgressPhase;
use xylem_lib::run::store::Bundle;
use xylem_lib::run::RunCancellation;
use xylem_lib::sandbox::{Prepared, SandboxInput, SandboxSession};
use xylem_lib::workspace::InputWorkspace;

pub(super) struct DispatchContext<'a> {
    pub(super) bundle: &'a Bundle,
    pub(super) config: &'a Config,
    pub(super) execution: &'a ExecutionContext,
    pub(super) id: &'a str,
    pub(super) input_kind: &'a str,
    pub(super) input_workspaces: &'a [InputWorkspace],
    pub(super) inputs: &'a [Option<String>],
    pub(super) journal: &'a RunJournal,
    pub(super) journal_containers: &'a [JournalContainer],
    pub(super) options: &'a ConfigOptions,
    pub(super) plan: &'a ExecutionPlan,
    pub(super) progress: &'a WorkflowProgress,
    pub(super) required_modules: &'a BTreeSet<usize>,
    pub(super) root: &'a Path,
    pub(super) run_cancellation: &'a RunCancellation,
    pub(super) sandbox: Option<&'a Arc<dyn SandboxSession>>,
}
pub(super) fn dispatch_inputs(context: DispatchContext<'_>) -> Result<Vec<Option<WorkflowOutcome>>, RunFailure> {
    let concurrency = context.execution.input_concurrency(context.options.batch);
    debug!(target: "xylem_cli::commands::run", concurrency, "=> {} Active workflow input limit", Label::using());
    let lifecycle = match context.options.sandbox_policy.is_some() {
        | true => Ok(ContainerLifecycle::default()),
        | false => ContainerLifecycle::start(
            context.config,
            context.required_modules,
            context.options.root.as_deref().unwrap_or(context.root),
            context.id,
            context.options.offline.unwrap_or_default(),
            context.options.container_cleanup,
        ),
    }
    .map_err(|why| {
        context.journal_containers.iter().for_each(|container| {
            let _ = context
                .journal
                .transition_container(container.index, ContainerState::Failed, Some(&why.to_string()));
        });
        RunFailure::new(exitcode::UNAVAILABLE, format!("prepare containers — {why}"))
    });
    lifecycle.and_then(|lifecycle| {
        context.journal_containers.iter().for_each(|container| {
            let state = match container.owned {
                | true => ContainerState::Ready,
                | false => ContainerState::External,
            };
            let _ = context.journal.transition_container(container.index, state, None);
        });
        let operation = |index, input: &Option<String>| execute_input(&context, index, input);
        let failed = |outcome: &WorkflowOutcome| !outcome.is_success();
        let dispatched = match context.plan.memory.as_ref() {
            | Some(memory) if memory.mode == MemoryPlanMode::MemoryAware => memory.reservations().map_or_else(
                || Err(RunFailure::new(exitcode::SOFTWARE, "memory-aware plan is missing an input reservation")),
                |reservations| {
                    context
                        .execution
                        .dispatch_inputs_fail_fast_memory(
                            context.inputs,
                            context.options.batch,
                            memory.budget.clone(),
                            reservations,
                            operation,
                            failed,
                        )
                        .map_err(|why| RunFailure::new(exitcode::OSERR, format!("run memory-aware input dispatcher — {why}")))
                },
            ),
            | Some(_) | None => context
                .execution
                .dispatch_inputs_fail_fast(context.inputs, context.options.batch, operation, failed)
                .map_err(|why| RunFailure::new(exitcode::OSERR, format!("initialize Xylem input worker pool — {why}"))),
        };
        finish_lifecycle(context.journal, context.journal_containers, lifecycle, dispatched)
    })
}
fn execute_active_input(context: &DispatchContext<'_>, index: usize, input: &Option<String>) -> WorkflowOutcome {
    match input {
        | Some(value) => info!(target: "xylem_cli::commands::run", value, source = context.input_kind, "=> {} Workflow input", Label::using()),
        | None => info!(target: "xylem_cli::commands::run", "=> {} Configuration with {}", Label::using(), Label::fmt_not_found("NO INPUTS")),
    }
    let transition = context
        .journal
        .transition_input(index, ExecutionState::Pending, None)
        .map_err(|why| format!("record pending input state — {why}"))
        .and_then(|()| {
            context
                .journal
                .transition_input(index, ExecutionState::Running, None)
                .map_err(|why| format!("record running input state — {why}"))
        });
    match transition {
        | Err(message) => {
            let state = ExecutionState::Failed;
            let _ = context.journal.transition_input(index, state, Some(&message));
            WorkflowOutcome::failed(input.clone(), message)
        }
        | Ok(()) => {
            let prepared = context.sandbox.map_or_else(
                || {
                    context
                        .input_workspaces
                        .get(index)
                        .ok_or_else(|| format!("input {index} workspace is missing"))
                        .and_then(|workspace| {
                            context
                                .config
                                .clone()
                                .prepare_modules_selected_in_workspace(context.required_modules, Some(context.progress), workspace)
                                .map(|config| (config, None))
                                .map_err(|why| why.to_string())
                        })
                },
                |session| {
                    context
                        .input_workspaces
                        .get(index)
                        .ok_or_else(|| format!("input {index} workspace is missing"))
                        .and_then(|workspace| {
                            context
                                .config
                                .clone()
                                .materialize_modules_selected_in_workspace(context.required_modules, None, workspace)
                                .map_err(|why| why.to_string())
                                .and_then(|materialized| {
                                    SandboxInput::new_with_input_count(
                                        materialized.clone(),
                                        index,
                                        context.inputs.len(),
                                        context.required_modules.iter().copied(),
                                        workspace.scratch().join("sandbox"),
                                        input.clone(),
                                    )
                                    .and_then(|requested| {
                                        requested.prepare(session.as_ref(), |module_index, state| {
                                            if let Some(module) = materialized.modules.get(module_index) {
                                                ProgressPhase::Preparation.emit(
                                                    state.into(),
                                                    &module.name,
                                                    &module.module_path,
                                                    Some(context.progress),
                                                    input.as_deref(),
                                                );
                                            }
                                        })
                                    })
                                    .map(|sandbox_input| (materialized, Some(sandbox_input)))
                                    .map_err(|why| why.to_string())
                                })
                        })
                },
            );
            let mut outcome = match prepared {
                | Err(why) => WorkflowOutcome::failed(input.clone(), why.to_string()),
                | Ok((prepared_config, sandbox_input)) => {
                    let execution_config = sandbox_input
                        .as_ref()
                        .map_or(prepared_config, |value: &SandboxInput<Prepared>| value.prepared_config().clone());
                    let execution_options = execution_config.options.clone().unwrap_or_else(|| context.options.clone());
                    let execution_input = sandbox_input
                        .as_ref()
                        .and_then(|value| value.prepared_input().map(str::to_string))
                        .or_else(|| input.clone());
                    let module_executor = LoggedModuleExecutor::new(context.bundle.clone(), context.run_cancellation.clone(), sandbox_input.clone());
                    let mut outcome = execution_config.run_workflow_recorded(
                        execution_input,
                        &execution_options,
                        Some(context.progress),
                        Some(&module_executor),
                        Some(context.journal),
                        index,
                    );
                    if let Some(sandbox_input) = sandbox_input {
                        let observed_peak = match (context.plan.memory.is_some(), outcome.is_success()) {
                            | (true, true) => match sandbox_input.observed_memory_peak() {
                                | Ok(peak) => peak,
                                | Err(why) => {
                                    warn!(
                                        target: "xylem_cli::commands::run",
                                        input_index = index,
                                        error = %why,
                                        "=> Sandbox peak memory is unavailable"
                                    );
                                    None
                                }
                            },
                            | _ => None,
                        };
                        if let Err(why) = sandbox_input.finish(outcome.is_success()) {
                            let cleanup = format!("finish sandbox input {index} — {why}");
                            outcome.state = ExecutionState::Failed;
                            outcome.error = Some(match outcome.error {
                                | Some(primary) => format!("{primary}; additionally, {cleanup}"),
                                | None => cleanup,
                            });
                        }
                        if let (true, Some(peak)) = (outcome.is_success(), observed_peak) {
                            if let Err(why) = context.journal.record_memory_peak(index, peak) {
                                let observation = format!("record sandbox input {index} peak memory — {why}");
                                outcome.state = ExecutionState::Failed;
                                outcome.error = Some(match outcome.error {
                                    | Some(primary) => format!("{primary}; additionally, {observation}"),
                                    | None => observation,
                                });
                            }
                        }
                    }
                    outcome
                }
            };
            let terminal_transition = context.journal.transition_input(index, outcome.state, outcome.error.as_deref());
            if let Err(why) = terminal_transition {
                let message = format!("record terminal input state — {why}");
                outcome.state = ExecutionState::Failed;
                outcome.error = Some(match outcome.error {
                    | Some(primary) => format!("{primary}; additionally, {message}"),
                    | None => message,
                });
            }
            outcome
        }
    }
}
fn execute_input(context: &DispatchContext<'_>, index: usize, input: &Option<String>) -> WorkflowOutcome {
    match context.run_cancellation.is_requested() {
        | true => {
            let message = "workflow cancellation requested";
            let _ = context.journal.transition_input(index, ExecutionState::Cancelled, Some(message));
            WorkflowOutcome::cancelled(input.clone(), message)
        }
        | false => execute_active_input(context, index, input),
    }
}
fn finish_lifecycle(
    journal: &RunJournal,
    journal_containers: &[JournalContainer],
    lifecycle: ContainerLifecycle,
    dispatched: Result<Vec<Option<WorkflowOutcome>>, RunFailure>,
) -> Result<Vec<Option<WorkflowOutcome>>, RunFailure> {
    let succeeded = dispatched
        .as_ref()
        .is_ok_and(|outcomes| outcomes.iter().flatten().all(WorkflowOutcome::is_success));
    let cleanup = lifecycle.finish(succeeded);
    match (dispatched, cleanup) {
        | (Ok(outcomes), Ok(finished)) => {
            journal_containers.iter().filter(|container| container.owned).for_each(|container| {
                let state = match finished.retained.contains(&container.name) {
                    | true => ContainerState::Retained,
                    | false => ContainerState::Removed,
                };
                let _ = journal.transition_container(container.index, state, None);
            });
            finished
                .retained
                .iter()
                .for_each(|name| info!(target: "xylem_cli::commands::run", name, "=> Retained managed container"));
            Ok(outcomes)
        }
        | (Err(primary), Ok(_)) => Err(primary),
        | (Ok(_), Err(cleanup)) => {
            mark_owned_containers_failed(journal, journal_containers, &cleanup.to_string());
            Err(RunFailure::new(exitcode::IOERR, format!("clean up containers — {cleanup}")))
        }
        | (Err(primary), Err(cleanup)) => {
            mark_owned_containers_failed(journal, journal_containers, &cleanup.to_string());
            Err(primary.with_secondary(format!("clean up containers — {cleanup}")))
        }
    }
}
fn mark_owned_containers_failed(journal: &RunJournal, journal_containers: &[JournalContainer], message: &str) {
    journal_containers.iter().filter(|container| container.owned).for_each(|container| {
        let _ = journal.transition_container(container.index, ContainerState::Failed, Some(message));
    });
}
