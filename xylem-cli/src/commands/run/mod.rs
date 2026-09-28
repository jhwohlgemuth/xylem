use acorn::util::Label;
use acorn_schema::validation::Validate;
use clap_verbosity_flag::Verbosity;
use core::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nanoid::nanoid;
use owo_colors::OwoColorize;
use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::fs::create_dir_all;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use tracing::{debug, info};
use xylem_lib::config::{Config, ConfigOptions, ConfigReadError};
use xylem_lib::execution::{ExecutionContext, ExecutionLimits};
use xylem_lib::journal::{ConfigurationIdentity, ExecutionState, FinalStatus, JournalInput, RunJournal, RunMetadata};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::plan::ExecutionPlan;
use xylem_lib::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use xylem_lib::remote::RemotePolicy;
use xylem_lib::script::{EnvironmentValue, ModuleType, ModuleUri};
use xylem_lib::util::deduplicate;
use xylem_lib::ReporterType;

const DEFAULT_CONFIG_NAME: &str = "config.json";
/// Failure returned to the executable boundary with a stable exit code.
#[derive(Debug)]
pub struct RunFailure {
    exit_code: i32,
    message: String,
}
struct WorkflowProgress {
    preparation: ProgressBar,
    execution: ProgressBar,
}
impl From<&RunFailure> for FinalStatus {
    fn from(_: &RunFailure) -> Self {
        Self::Failed
    }
}
impl From<RunFailure> for FinalStatus {
    fn from(_: RunFailure) -> Self {
        Self::Failed
    }
}
impl fmt::Display for RunFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}
impl RunFailure {
    fn new(exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
    fn with_secondary(mut self, message: impl fmt::Display) -> Self {
        self.message = format!("{}; additionally, {message}", self.message);
        self
    }
    /// Return the process exit code owned by the CLI boundary.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}
impl std::error::Error for RunFailure {}
impl ProgressObserver for WorkflowProgress {
    fn update(&self, update: ProgressUpdate) {
        let bar = match update.phase {
            | ProgressPhase::Preparation => &self.preparation,
            | ProgressPhase::Execution => &self.execution,
        };
        match update.state {
            | ProgressState::Started => {
                let message = match update.input {
                    | Some(input) => format!("{} · {input}", update.module),
                    | None => update.module,
                };
                bar.set_message(message);
            }
            | ProgressState::Finished | ProgressState::Skipped | ProgressState::SkippedExisting => bar.inc(1),
            | ProgressState::Failed => {
                bar.inc(1);
                bar.set_message(format!("{} failed", update.module));
            }
        }
    }
}
impl WorkflowProgress {
    fn new(preparation_total: u64, execution_total: u64, disabled: bool) -> Self {
        match disabled {
            | true => Self {
                preparation: ProgressBar::hidden(),
                execution: ProgressBar::hidden(),
            },
            | false => {
                let multi = MultiProgress::new();
                let style = ProgressStyle::with_template("{spinner:.cyan} {msg:28} [{bar:32.cyan/blue}] {pos}/{len}")
                    .unwrap_or_else(|_| ProgressStyle::default_bar())
                    .progress_chars("=>-");
                let preparation = multi.add(ProgressBar::new(preparation_total));
                preparation.set_style(style.clone());
                preparation.set_message("Preparing modules");
                preparation.enable_steady_tick(Duration::from_millis(100));
                let execution = multi.add(ProgressBar::new(execution_total));
                execution.set_style(style);
                execution.set_message("Waiting to execute");
                execution.enable_steady_tick(Duration::from_millis(100));
                Self { preparation, execution }
            }
        }
    }
    fn finish_preparation(&self, succeeded: bool) {
        match succeeded {
            | true => self.preparation.finish_with_message("Modules prepared"),
            | false => self.preparation.abandon_with_message("Module preparation failed"),
        }
    }
    fn finish_execution(&self, succeeded: bool) {
        match succeeded {
            | true => self.execution.finish_with_message("Workflow complete"),
            | false => self.execution.abandon_with_message("Workflow failed"),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn run(
    config: &Option<PathBuf>,
    reporter: &[ReporterType],
    report_output: &Option<PathBuf>,
    artifact_output: &Option<PathBuf>,
    cli_input: &Option<Vec<String>>,
    cli_dry_run: bool,
    cli_offline: bool,
    cli_resume: bool,
    cli_allowed: &Option<Vec<String>>,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    _verbose: Verbosity,
    limits: ExecutionLimits,
    no_progress: bool,
) -> Result<(), RunFailure> {
    let default_config = env::current_dir()
        .map(|path| path.join(DEFAULT_CONFIG_NAME))
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_NAME));
    let config_path = match config {
        | Some(value) => value.to_path_buf(),
        | None => default_config,
    };
    let prepared = Config::try_from(config_path.as_path())
        .map_err(|why| {
            let exit_code = match &why {
                | ConfigReadError::UnsupportedExtension(_) => exitcode::USAGE,
                | ConfigReadError::Invalid { .. } => exitcode::DATAERR,
                | ConfigReadError::Read { .. } => exitcode::UNAVAILABLE,
            };
            RunFailure::new(exit_code, why.to_string())
        })
        .and_then(|mut preview| {
            artifact_output
                .as_ref()
                .map_or(Ok(()), |output| {
                    output.to_str().map_or_else(
                        || Err(RunFailure::new(exitcode::USAGE, "--artifact-output must be valid UTF-8")),
                        |output| {
                            preview.output = Some(output.to_string());
                            preview
                                .variables
                                .get_or_insert_default()
                                .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
                            preview.modules.iter_mut().for_each(|module| {
                                if let Some(variables) = &mut module.variables {
                                    if variables.contains_key("OUTPUT_DIRECTORY") {
                                        variables.insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
                                    }
                                }
                            });
                            Ok(())
                        },
                    )
                })
                .map(|()| {
                    let options = preview.options.take().unwrap_or_else(|| ConfigOptions::init().build());
                    let allowed = cli_allowed.clone().or(options.allowed.clone());
                    let dry_run = cli_dry_run || options.dry_run.unwrap_or_default();
                    let offline = cli_offline || options.offline.unwrap_or_default();
                    let resume = cli_resume || options.resume.unwrap_or_default();
                    let skip_verify_checksum = cli_skip_verify_checksum || options.skip_verify_checksum.unwrap_or_default();
                    let skip_verify_requirements = cli_skip_verify_requirements || options.skip_verify_requirements.unwrap_or_default();
                    preview.options = Some(
                        options
                            .with_allowed(allowed)
                            .with_dry_run(Some(dry_run))
                            .with_offline(Some(offline))
                            .with_resume(Some(resume))
                            .with_skip_verify_checksum(Some(skip_verify_checksum))
                            .with_skip_verify_requirements(Some(skip_verify_requirements)),
                    );
                    preview
                })
        })
        .and_then(|preview| {
            let options = preview.options.as_ref();
            let allowed = options.and_then(|value| value.allowed.as_deref());
            let dry_run = options.and_then(|value| value.dry_run).unwrap_or_default();
            RemotePolicy::from_hosts(allowed)
                .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid remote module allowlist — {why}")))
                .and_then(|policy| {
                    let source_root = env::temp_dir().join(EnvironmentValue::PREFIX).join("configuration-sources");
                    preview
                        .resolve_with_policy(config_path.as_path(), &policy, &source_root, dry_run)
                        .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                })
        })
        .and_then(|preview| {
            preview
                .validate()
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|()| preview)
        })
        .and_then(|preview| {
            preview
                .resolve_inputs(cli_input.as_deref())
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|inputs| (preview, inputs))
        })
        .and_then(|(preview, inputs)| {
            RemotePolicy::from_hosts(preview.options.as_ref().and_then(|options| options.allowed.as_deref()))
                .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid remote module allowlist — {why}")))
                .and_then(|policy| {
                    preview.modules.iter().try_for_each(|module| {
                        let uri = match &module.module_type {
                            | ModuleType::Binary { uri, .. } | ModuleType::Configuration { uri } | ModuleType::Script { uri, .. } => uri,
                        };
                        match ModuleUri::from(uri) {
                            | ModuleUri::Url(_) => policy
                                .ensure_allowed(uri.into())
                                .map(|_| ())
                                .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, format!("module {}: {why}", module.name))),
                            | ModuleUri::File(_) | ModuleUri::Unknown(_) => Ok(()),
                        }
                    })
                })
                .map(|()| (preview, inputs))
        })
        .and_then(|(preview, inputs)| {
            let resume = preview.options.as_ref().and_then(|options| options.resume).unwrap_or_default();
            ExecutionPlan::build(&preview, config_path.display().to_string(), &inputs, resume)
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|plan| (preview, inputs, plan))
        });
    match prepared {
        | Ok((preview, _, plan)) if preview.options.as_ref().and_then(|options| options.dry_run).unwrap_or_default() => match report_output {
            | Some(_) => Err(RunFailure::new(
                exitcode::USAGE,
                "--report-output cannot be used with a write-free dry run",
            )),
            | None => {
                let selected = deduplicate(reporter);
                selected.iter().try_for_each(|reporter| match reporter {
                    | ReporterType::Console => {
                        println!("Dry-run execution plan");
                        plan.modules.iter().for_each(|module| {
                            println!(
                                "{} · {}: {}",
                                module.module,
                                module.input.as_deref().unwrap_or("<no input>"),
                                module.decision
                            );
                        });
                        Ok(())
                    }
                    | ReporterType::Json => serde_json::to_writer_pretty(io::stdout().lock(), &plan)
                        .map_err(|why| RunFailure::new(exitcode::IOERR, format!("render dry-run plan: {why}")))
                        .map(|()| println!()),
                })
            }
        },
        | Ok((preview, inputs, plan)) => {
            let execution =
                ExecutionContext::new(limits).map_err(|why| RunFailure::new(exitcode::OSERR, format!("initialize Xylem worker pool — {why}")));
            match execution {
                | Ok(execution) => {
                    debug!(threads = execution.limits().threads(), "=> {} Rayon worker limit", Label::using());
                    let id = nanoid!(10, &nanoid::alphabet::SAFE);
                    let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
                    let root_creation = create_dir_all(&root)
                        .map_err(|why| RunFailure::new(exitcode::CANTCREAT, format!("create run directory {} — {why}", root.display())));
                    match root_creation {
                        | Ok(()) => {
                            debug!(path = %root.display(), "=> {} Root directory", Label::using());
                            let config = Ok(preview);
                            match config {
                                | Ok(cfg) => match cfg.validate().map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string())) {
                                    | Ok(()) => {
                                        let input_kind = match cli_input.as_ref().is_some_and(|values| !values.is_empty()) {
                                            | true => "CLI",
                                            | false => "Configuration",
                                        };
                                        let allowed = cfg.options.as_ref().and_then(|value| value.allowed.clone());
                                        let policy = RemotePolicy::from_hosts(allowed.as_deref())
                                            .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid remote module allowlist — {why}")));
                                        match policy {
                                            | Ok(_) => {
                                                let options = cfg
                                                    .options
                                                    .clone()
                                                    .unwrap_or_else(|| ConfigOptions::init().build())
                                                    .with_root(Some(root.clone()));
                                                debug!("=> {} Run configuration options = {}", Label::using(), format!("{options:#?}").dimmed());
                                                let required_modules = Ok(plan
                                                    .modules
                                                    .iter()
                                                    .filter(|module| module.decision == "would_run")
                                                    .map(|module| module.module_index)
                                                    .collect::<BTreeSet<_>>());
                                                let journal_directory = report_output.as_ref().unwrap_or(&root);
                                                let modules = required_modules.and_then(|required_modules| {
                                                    create_dir_all(journal_directory).map(|()| required_modules).map_err(|why| {
                                                        RunFailure::new(
                                                            exitcode::CANTCREAT,
                                                            format!("create output directory {} — {why}", journal_directory.display()),
                                                        )
                                                    })
                                                });
                                                match modules {
                                                    | Ok(required_modules) => {
                                                        let journal_path = journal_directory.join(format!("{id}.journal.jsonl"));
                                                        let journal_inputs = inputs
                                                            .iter()
                                                            .cloned()
                                                            .enumerate()
                                                            .map(|(index, value)| JournalInput::new(index, value))
                                                            .collect::<Vec<_>>();
                                                        let module_names = cfg.modules.iter().map(|module| module.name.clone()).collect::<Vec<_>>();
                                                        let run_id = id.clone();
                                                        let configuration = ConfigurationIdentity {
                                                            path: config_path.display().to_string(),
                                                            id: cfg.id.clone(),
                                                            version: cfg.version.clone(),
                                                        };
                                                        let dry_run = options.dry_run.unwrap_or_default();
                                                        let metadata = RunMetadata::now(run_id, configuration, dry_run);
                                                        let create_journal =
                                                            RunJournal::create(journal_path.clone(), metadata, journal_inputs, module_names);
                                                        let create_journal_result = create_journal.map_err(|why| {
                                                            RunFailure::new(exitcode::CANTCREAT, format!("create durable execution journal: {why}"))
                                                        });
                                                        match create_journal_result {
                                                            | Ok(journal) => {
                                                                info!(path = %journal_path.display(), run_id = id, "=> {} Execution journal", Label::using());
                                                                let progress_disabled =
                                                                    no_progress || !io::stderr().is_terminal() || env::var_os("CI").is_some();
                                                                let progress = WorkflowProgress::new(
                                                                    cfg.modules.len() as u64,
                                                                    cfg.modules.len().saturating_mul(inputs.len()) as u64,
                                                                    progress_disabled,
                                                                );
                                                                let prepared = cfg
                                                                    .with_options(options)
                                                                    .verify_requirements()
                                                                    .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
                                                                    .map(Config::verify_resources)
                                                                    .and_then(|config| {
                                                                        config
                                                                            .prepare_modules_selected(&execution, &required_modules, Some(&progress))
                                                                            .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
                                                                    })
                                                                    .map(Config::set_environment_variables);
                                                                progress.finish_preparation(prepared.is_ok());
                                                                let dispatched = match prepared {
                                                                    | Ok(prepared) => {
                                                                        let options = prepared
                                                                            .options
                                                                            .clone()
                                                                            .unwrap_or_else(|| ConfigOptions::init().root(root.clone()).build());
                                                                        let concurrency = execution.input_concurrency(options.batch);
                                                                        debug!(concurrency, "=> {} Active workflow input limit", Label::using());
                                                                        let operation = |index, input: &Option<String>| {
                                                                            match input {
                                                                                | Some(value) => info!(
                                                                                    value,
                                                                                    source = input_kind,
                                                                                    "=> {} Workflow input",
                                                                                    Label::using()
                                                                                ),
                                                                                | None => info!(
                                                                                    "=> {} Configuration with {}",
                                                                                    Label::using(),
                                                                                    Label::fmt_not_found("NO INPUTS")
                                                                                ),
                                                                            }
                                                                            let transition = journal
                                                                                .transition_input(index, ExecutionState::Pending, None)
                                                                                .map_err(|why| format!("record pending input state: {why}"))
                                                                                .and_then(|()| {
                                                                                    journal
                                                                                        .transition_input(index, ExecutionState::Running, None)
                                                                                        .map_err(|why| format!("record running input state: {why}"))
                                                                                });
                                                                            match transition {
                                                                                | Err(message) => {
                                                                                    let state = ExecutionState::Failed;
                                                                                    let _ = journal.transition_input(index, state, Some(&message));
                                                                                    WorkflowOutcome::failed(input.clone(), message)
                                                                                }
                                                                                | Ok(()) => {
                                                                                    let mut outcome = prepared.clone().run_workflow_recorded(
                                                                                        input.clone(),
                                                                                        &options,
                                                                                        Some(&progress),
                                                                                        Some(&journal),
                                                                                        index,
                                                                                    );
                                                                                    let terminal_transition = journal.transition_input(
                                                                                        index,
                                                                                        outcome.state,
                                                                                        outcome.error.as_deref(),
                                                                                    );
                                                                                    if let Err(why) = terminal_transition {
                                                                                        let message = format!("record terminal input state: {why}");
                                                                                        outcome.state = ExecutionState::Failed;
                                                                                        outcome.error = Some(match outcome.error {
                                                                                            | Some(primary) => {
                                                                                                format!("{primary}; additionally, {message}")
                                                                                            }
                                                                                            | None => message,
                                                                                        });
                                                                                    }
                                                                                    outcome
                                                                                }
                                                                            }
                                                                        };
                                                                        let failed = |outcome: &WorkflowOutcome| !outcome.is_success();
                                                                        let execute_dispatch = execution.dispatch_inputs_fail_fast(
                                                                            &inputs,
                                                                            options.batch,
                                                                            operation,
                                                                            failed,
                                                                        );
                                                                        execute_dispatch.map_err(|why| {
                                                                            RunFailure::new(
                                                                                exitcode::OSERR,
                                                                                format!("initialize Xylem input worker pool: {why}"),
                                                                            )
                                                                        })
                                                                    }
                                                                    | Err(why) => Err(why),
                                                                };
                                                                let mut primary_failure = match &dispatched {
                                                                    | Ok(outcomes) => {
                                                                        outcomes.iter().flatten().find(|outcome| !outcome.is_success()).map(
                                                                            |outcome| {
                                                                                RunFailure::new(
                                                                                    exitcode::SOFTWARE,
                                                                                    outcome
                                                                                        .error
                                                                                        .clone()
                                                                                        .unwrap_or_else(|| "workflow input failed".to_string()),
                                                                                )
                                                                            },
                                                                        )
                                                                    }
                                                                    | Err(why) => Some(RunFailure::new(why.exit_code(), why.to_string())),
                                                                };
                                                                progress.finish_execution(primary_failure.is_none());
                                                                let final_status =
                                                                    primary_failure.as_ref().map_or(FinalStatus::Succeeded, FinalStatus::from);
                                                                let final_error = primary_failure.as_ref().map(ToString::to_string);
                                                                if let Err(why) = journal.finish(final_status, final_error.as_deref()) {
                                                                    primary_failure = Some(match primary_failure {
                                                                        | Some(failure) => {
                                                                            failure.with_secondary(format!("finalize execution journal: {why}"))
                                                                        }
                                                                        | None => RunFailure::new(
                                                                            exitcode::IOERR,
                                                                            format!("finalize execution journal: {why}"),
                                                                        ),
                                                                    });
                                                                }
                                                                let summary = match RunJournal::read(&journal_path) {
                                                                    | Ok(summary) => Some(summary),
                                                                    | Err(why) => {
                                                                        primary_failure = Some(match primary_failure {
                                                                            | Some(failure) => {
                                                                                failure.with_secondary(format!("recover execution journal: {why}"))
                                                                            }
                                                                            | None => RunFailure::new(
                                                                                exitcode::IOERR,
                                                                                format!("recover execution journal: {why}"),
                                                                            ),
                                                                        });
                                                                        None
                                                                    }
                                                                };
                                                                if let Some(summary) = summary {
                                                                    let selected = deduplicate(reporter);
                                                                    if !selected.contains(&ReporterType::Console) {
                                                                        let stderr = io::stderr();
                                                                        if let Err(why) =
                                                                            writeln!(stderr.lock(), "Journal: {}", journal_path.display())
                                                                        {
                                                                            primary_failure = Some(match primary_failure {
                                                                                | Some(failure) => {
                                                                                    failure.with_secondary(format!("write journal location: {why}"))
                                                                                }
                                                                                | None => RunFailure::new(
                                                                                    exitcode::IOERR,
                                                                                    format!("write journal location: {why}"),
                                                                                ),
                                                                            });
                                                                        }
                                                                    }
                                                                    let render = summary.render(&selected, report_output.as_deref(), &journal_path);
                                                                    if let Err(why) = render {
                                                                        primary_failure = Some(match primary_failure {
                                                                            | Some(failure) => {
                                                                                failure.with_secondary(format!("render run report: {why}"))
                                                                            }
                                                                            | None => {
                                                                                RunFailure::new(exitcode::IOERR, format!("render run report: {why}"))
                                                                            }
                                                                        });
                                                                    }
                                                                }
                                                                match primary_failure {
                                                                    | Some(failure) => Err(failure),
                                                                    | None => Ok(()),
                                                                }
                                                            }
                                                            | Err(why) => Err(why),
                                                        }
                                                    }
                                                    | Err(why) => Err(why),
                                                }
                                            }
                                            | Err(why) => Err(why),
                                        }
                                    }
                                    | Err(why) => Err(why),
                                },
                                | Err(why) => Err(why),
                            }
                        }
                        | Err(why) => Err(why),
                    }
                }
                | Err(why) => Err(why),
            }
        }
        | Err(failure) => Err(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_progress_uses_hidden_bars() {
        let progress = WorkflowProgress::new(2, 4, true);
        assert!(progress.preparation.is_hidden());
        assert!(progress.execution.is_hidden());
    }
}
