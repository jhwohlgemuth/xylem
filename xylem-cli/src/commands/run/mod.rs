use acorn::util::Label;
use clap_verbosity_flag::Verbosity;
use core::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nanoid::nanoid;
use owo_colors::OwoColorize;
use std::env;
use std::fmt;
use std::fs::create_dir_all;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use tracing::{debug, info, warn};
use xylem_lib::config::{Config, ConfigOptions, ConfigReadError};
use xylem_lib::execution::{ExecutionContext, ExecutionLimits};
use xylem_lib::journal::{ConfigurationIdentity, ExecutionState, FinalStatus, JournalInput, RunJournal, RunMetadata};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use xylem_lib::remote::RemotePolicy;
use xylem_lib::script::EnvironmentValue;
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
            | ProgressState::Finished | ProgressState::Skipped => bar.inc(1),
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
                    .expect("the progress template is static and valid")
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
    output: &Option<PathBuf>,
    cli_input: &Option<Vec<String>>,
    cli_dry_run: bool,
    cli_offline: bool,
    cli_allowed: &Option<Vec<String>>,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    _verbose: Verbosity,
    limits: ExecutionLimits,
    no_progress: bool,
) -> Result<(), RunFailure> {
    match ExecutionContext::new(limits).map_err(|why| RunFailure::new(exitcode::OSERR, format!("initialize Xylem worker pool: {why}"))) {
        | Err(why) => Err(why),
        | Ok(execution) => {
            debug!(threads = execution.limits().threads(), "=> {} Rayon worker limit", Label::using());
            let id = nanoid!(10, &nanoid::alphabet::SAFE);
            let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
            let root_creation =
                create_dir_all(&root).map_err(|why| RunFailure::new(exitcode::CANTCREAT, format!("create run directory {}: {why}", root.display())));
            match root_creation {
                | Err(why) => Err(why),
                | Ok(()) => {
                    debug!(path = %root.display(), "=> {} Root directory", Label::using());
                    let default_config = env::current_dir()
                        .map(|path| path.join(DEFAULT_CONFIG_NAME))
                        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_NAME));
                    let config_path = match config {
                        | Some(value) => value.to_path_buf(),
                        | None => {
                            warn!(path = %default_config.display(), "=> {} Default configuration", Label::using());
                            default_config
                        }
                    };
                    let config = Config::try_from(config_path.as_path()).map_err(|why| {
                        let exit_code = match &why {
                            | ConfigReadError::UnsupportedExtension(_) => exitcode::USAGE,
                            | ConfigReadError::Invalid { .. } => exitcode::DATAERR,
                            | ConfigReadError::Read { .. } => exitcode::UNAVAILABLE,
                        };
                        RunFailure::new(exit_code, why.to_string())
                    });
                    match config {
                        | Err(why) => Err(why),
                        | Ok(cfg) => {
                            let config_inputs = cfg.clone().get_inputs().filter(|values| !values.is_empty());
                            let (input_kind, inputs) = match config_inputs {
                                | Some(values) => ("Configuration", values.into_iter().map(Some).collect::<Vec<_>>()),
                                | None => match cli_input.clone().filter(|values| !values.is_empty()) {
                                    | Some(values) => ("CLI", values.into_iter().map(Some).collect::<Vec<_>>()),
                                    | None => ("Configuration", vec![None]),
                                },
                            };
                            let allowed = cli_allowed
                                .clone()
                                .or_else(|| cfg.options.as_ref().and_then(|value| value.allowed.clone()));
                            match RemotePolicy::from_hosts(allowed.as_deref())
                                .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid remote module allowlist: {why}")))
                            {
                                | Err(why) => Err(why),
                                | Ok(_) => {
                                    let options = match &cfg.options {
                                        | Some(ConfigOptions {
                                            batch,
                                            dry_run,
                                            offline,
                                            skip_verify_checksum,
                                            skip_verify_requirements,
                                            ..
                                        }) => ConfigOptions::init()
                                            .root(root)
                                            .maybe_batch(*batch)
                                            .dry_run(cli_dry_run || dry_run.unwrap_or_default())
                                            .offline(cli_offline || offline.unwrap_or_default())
                                            .maybe_allowed(allowed)
                                            .skip_verify_checksum(cli_skip_verify_checksum || skip_verify_checksum.unwrap_or_default())
                                            .skip_verify_requirements(cli_skip_verify_requirements || skip_verify_requirements.unwrap_or_default())
                                            .build(),
                                        | None => ConfigOptions::init()
                                            .root(root)
                                            .dry_run(cli_dry_run)
                                            .offline(cli_offline)
                                            .maybe_allowed(allowed)
                                            .skip_verify_checksum(cli_skip_verify_checksum)
                                            .skip_verify_requirements(cli_skip_verify_requirements)
                                            .build(),
                                    };
                                    debug!("=> {} Run configuration options = {}", Label::using(), format!("{options:#?}").dimmed());
                                    let journal_directory = output.as_ref().or(options.root.as_ref()).expect("run root is configured");
                                    match create_dir_all(journal_directory).map_err(|why| {
                                        RunFailure::new(
                                            exitcode::CANTCREAT,
                                            format!("create output directory {}: {why}", journal_directory.display()),
                                        )
                                    }) {
                                        | Err(why) => Err(why),
                                        | Ok(()) => {
                                            let journal_path = journal_directory.join(format!("{id}.journal.jsonl"));
                                            let journal_inputs = inputs
                                                .iter()
                                                .cloned()
                                                .enumerate()
                                                .map(|(index, value)| JournalInput::new(index, value))
                                                .collect::<Vec<_>>();
                                            let module_names = cfg.modules.iter().map(|module| module.name.clone()).collect::<Vec<_>>();
                                            let metadata = RunMetadata::now(
                                                id.clone(),
                                                ConfigurationIdentity {
                                                    path: config_path.display().to_string(),
                                                    id: cfg.id.clone(),
                                                    version: cfg.version.clone(),
                                                },
                                                options.dry_run.unwrap_or_default(),
                                            );
                                            let journal_creation = RunJournal::create(journal_path.clone(), metadata, journal_inputs, module_names)
                                                .map_err(|why| {
                                                    RunFailure::new(exitcode::CANTCREAT, format!("create durable execution journal: {why}"))
                                                });
                                            match journal_creation {
                                                | Err(why) => Err(why),
                                                | Ok(journal) => {
                                                    info!(path = %journal_path.display(), run_id = id, "=> {} Execution journal", Label::using());
                                                    let progress_disabled = no_progress || !io::stderr().is_terminal() || env::var_os("CI").is_some();
                                                    let progress = WorkflowProgress::new(
                                                        cfg.modules.len() as u64,
                                                        cfg.modules.len().saturating_mul(inputs.len()) as u64,
                                                        progress_disabled,
                                                    );
                                                    let prepared = cfg
                                                        .with_options(options)
                                                        .verify_requirements()
                                                        .map(Config::verify_resources)
                                                        .and_then(|config| config.prepare_modules(&execution, Some(&progress)))
                                                        .map(Config::set_environment_variables)
                                                        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why));
                                                    progress.finish_preparation(prepared.is_ok());
                                                    let dispatched = match prepared {
                                                        | Ok(prepared) => {
                                                            let options = prepared.options.as_ref().expect("prepared configuration has run options");
                                                            let input_concurrency = execution.input_concurrency(options.batch);
                                                            debug!(input_concurrency, "=> {} Active workflow input limit", Label::using());
                                                            execution
                                                                .dispatch_inputs_fail_fast(
                                                                    &inputs,
                                                                    options.batch,
                                                                    |input_index, input| {
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
                                                                            .transition_input(input_index, ExecutionState::Pending, None)
                                                                            .map_err(|why| format!("record pending input state: {why}"))
                                                                            .and_then(|()| {
                                                                                journal
                                                                                    .transition_input(input_index, ExecutionState::Running, None)
                                                                                    .map_err(|why| format!("record running input state: {why}"))
                                                                            });
                                                                        match transition {
                                                                            | Err(message) => {
                                                                                let _ = journal.transition_input(
                                                                                    input_index,
                                                                                    ExecutionState::Failed,
                                                                                    Some(&message),
                                                                                );
                                                                                WorkflowOutcome::failed(input.clone(), message)
                                                                            }
                                                                            | Ok(()) => {
                                                                                let mut outcome = prepared.clone().run_workflow_recorded(
                                                                                    input.clone(),
                                                                                    options,
                                                                                    Some(&progress),
                                                                                    Some(&journal),
                                                                                    input_index,
                                                                                );
                                                                                let terminal_transition = journal.transition_input(
                                                                                    input_index,
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
                                                                    },
                                                                    |outcome| !outcome.is_success(),
                                                                )
                                                                .map_err(|why| {
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
                                                            outcomes.iter().flatten().find(|outcome| !outcome.is_success()).map(|outcome| {
                                                                RunFailure::new(
                                                                    exitcode::SOFTWARE,
                                                                    outcome.error.clone().unwrap_or_else(|| "workflow input failed".to_string()),
                                                                )
                                                            })
                                                        }
                                                        | Err(why) => Some(RunFailure::new(why.exit_code(), why.to_string())),
                                                    };
                                                    progress.finish_execution(primary_failure.is_none());
                                                    let final_status = primary_failure.as_ref().map_or(FinalStatus::Succeeded, FinalStatus::from);
                                                    let final_error = primary_failure.as_ref().map(ToString::to_string);
                                                    if let Err(why) = journal.finish(final_status, final_error.as_deref()) {
                                                        primary_failure = Some(match primary_failure {
                                                            | Some(failure) => failure.with_secondary(format!("finalize execution journal: {why}")),
                                                            | None => RunFailure::new(exitcode::IOERR, format!("finalize execution journal: {why}")),
                                                        });
                                                    }
                                                    let summary = match RunJournal::read(&journal_path) {
                                                        | Ok(summary) => Some(summary),
                                                        | Err(why) => {
                                                            primary_failure = Some(match primary_failure {
                                                                | Some(failure) => {
                                                                    failure.with_secondary(format!("recover execution journal: {why}"))
                                                                }
                                                                | None => {
                                                                    RunFailure::new(exitcode::IOERR, format!("recover execution journal: {why}"))
                                                                }
                                                            });
                                                            None
                                                        }
                                                    };
                                                    if let Some(summary) = summary {
                                                        let selected = deduplicate(reporter);
                                                        if !selected.contains(&ReporterType::Console) {
                                                            let stderr = io::stderr();
                                                            if let Err(why) = writeln!(stderr.lock(), "Journal: {}", journal_path.display()) {
                                                                primary_failure = Some(match primary_failure {
                                                                    | Some(failure) => {
                                                                        failure.with_secondary(format!("write journal location: {why}"))
                                                                    }
                                                                    | None => {
                                                                        RunFailure::new(exitcode::IOERR, format!("write journal location: {why}"))
                                                                    }
                                                                });
                                                            }
                                                        }
                                                        if let Err(why) = summary.render(&selected, output.as_deref(), &journal_path) {
                                                            primary_failure = Some(match primary_failure {
                                                                | Some(failure) => failure.with_secondary(format!("render run report: {why}")),
                                                                | None => RunFailure::new(exitcode::IOERR, format!("render run report: {why}")),
                                                            });
                                                        }
                                                    }
                                                    match primary_failure {
                                                        | Some(failure) => Err(failure),
                                                        | None => Ok(()),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_progress_uses_hidden_bars() {
        let progress = WorkflowProgress::new(2, 4, true);
        assert!(progress.preparation.is_hidden());
        assert!(progress.execution.is_hidden());
    }
}
