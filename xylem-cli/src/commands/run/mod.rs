use crate::cli::DEFAULT_CONFIG_NAMES;
use acorn_cmd::ExecutionOptions;
use acorn_host::config::resolve_config_path;
use acorn_host::terminal::Label;
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use clap_verbosity_flag::Verbosity;
use core::fmt;
use core::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nanoid::nanoid;
use owo_colors::OwoColorize;
use std::env;
use std::fs::{copy, create_dir_all};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use tracing::{debug, info};
use xylem_lib::config::{Config, ConfigOptions, ConfigReadError, ModuleExecutor};
use xylem_lib::container::ContainerLifecycle;
use xylem_lib::execution::{ExecutionContext, ExecutionLimits};
use xylem_lib::gpu::{GpuMonitor, GpuMonitorError};
use xylem_lib::journal::{
    ConfigurationIdentity, ContainerState, ExecutionState, FinalStatus, JournalContainer, JournalInput, JournalModule, RunJournal, RunMetadata,
};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::plan::ExecutionPlan;
use xylem_lib::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use xylem_lib::remote::RemotePolicy;
use xylem_lib::run::store::{Bundle, Manifest, Store, TaskLogSink};
use xylem_lib::run::telemetry::SystemMonitor;
use xylem_lib::run::RunCancellation;
use xylem_lib::script::{managed_container_name, ContainerCleanup, EnvironmentValue, ModuleType, ModuleUri, Script};
use xylem_lib::util::deduplicate;
use xylem_lib::ReporterType;

const CANCELLED_EXIT_CODE: i32 = 130;

struct LoggedModuleExecutor {
    bundle: Bundle,
    cancellation: RunCancellation,
}

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

impl ModuleExecutor for LoggedModuleExecutor {
    fn execute(&self, script: &Script, input_index: usize, module_index: usize) -> Result<(), xylem_lib::error::ProcessError> {
        let sink = TaskLogSink::module(self.bundle.clone(), input_index, module_index);
        let options = ExecutionOptions::new()
            .with_cancellation(self.cancellation.token())
            .with_sink(sink.clone());
        let result = script.execute_checked(&options).map(|execution| {
            let stdout = String::from_utf8_lossy(execution.stdout().bytes());
            let stderr = String::from_utf8_lossy(execution.stderr().bytes());
            if !stdout.is_empty() {
                tracing::trace!(target: "xylem_cli::output", "=> {} {}", Label::output(), stdout.trim_end().dimmed());
            }
            if !stderr.is_empty() {
                tracing::trace!(target: "xylem_cli::output", "=> {} {}", Label::output(), stderr.trim_end().dimmed());
            }
        });
        let flushed = sink.flush().map_err(|source| xylem_lib::error::ProcessError::Output {
            script: script.to_string(),
            working_directory: PathBuf::from(&script.current_dir),
            source: io::Error::other(source),
        });
        match (result, flushed) {
            | (Ok(()), Ok(())) => Ok(()),
            | (Err(primary), _) => Err(primary),
            | (Ok(()), Err(output)) => Err(output),
        }
    }

    fn is_cancellation_requested(&self) -> bool {
        self.cancellation.is_requested()
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

impl core::error::Error for RunFailure {}

impl fmt::Display for RunFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl From<GpuMonitorError> for RunFailure {
    fn from(error: GpuMonitorError) -> Self {
        match error {
            | GpuMonitorError::Journal(error) => Self::new(exitcode::IOERR, error.to_string()),
            | GpuMonitorError::Runtime(message) => Self::new(exitcode::UNAVAILABLE, message),
            | GpuMonitorError::Thread(message) => Self::new(exitcode::OSERR, message),
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

impl ProgressObserver for WorkflowProgress {
    fn update(&self, update: ProgressUpdate) {
        let bar = match update.phase {
            | ProgressPhase::Preparation => &self.preparation,
            | ProgressPhase::Execution => &self.execution,
        };
        match update.state {
            | ProgressState::Cancelled => {
                bar.set_position(bar.position().saturating_add(1));
                bar.set_message(format!("{} cancelled", update.module_path.join("::")));
            }
            | ProgressState::Started => {
                let module = update.module_path.join("::");
                let message = match update.input {
                    | Some(input) => format!("{module} · {input}"),
                    | None => module,
                };
                bar.set_message(message);
            }
            | ProgressState::Finished | ProgressState::Skipped | ProgressState::SkippedExisting => {
                bar.set_position(bar.position().saturating_add(1));
            }
            | ProgressState::Failed => {
                bar.set_position(bar.position().saturating_add(1));
                bar.set_message(format!("{} failed", update.module_path.join("::")));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Resolve, validate, plan, execute, journal, and report one workflow run.
pub fn run(
    config: &Option<PathBuf>,
    reporter: &[ReporterType],
    report_output: &Option<PathBuf>,
    artifact_output: &Option<PathBuf>,
    cli_input: &Option<Vec<String>>,
    cli_dry_run: bool,
    cli_gpu_monitor: bool,
    cli_offline: bool,
    cli_resume: bool,
    cli_allowed: &Option<Vec<String>>,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    cli_container_cleanup: Option<ContainerCleanup>,
    cli_no_container_env: bool,
    _verbose: Verbosity,
    limits: ExecutionLimits,
    no_progress: bool,
    cancellation: Option<RunCancellation>,
) -> Result<(), RunFailure> {
    let config_path = env::current_dir()
        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, format!("resolve current directory — {why}")))
        .and_then(|directory| {
            resolve_config_path(config.as_deref(), &directory, &DEFAULT_CONFIG_NAMES).ok_or_else(|| {
                RunFailure::new(
                    exitcode::USAGE,
                    format!("no workflow configuration found; expected one of {}", DEFAULT_CONFIG_NAMES.join(", ")),
                )
            })
        })?;
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
                    let gpu_monitor = cli_gpu_monitor || options.gpu_monitor.unwrap_or_default();
                    let offline = cli_offline || options.offline.unwrap_or_default();
                    let resume = cli_resume || options.resume.unwrap_or_default();
                    let skip_verify_checksum = cli_skip_verify_checksum || options.skip_verify_checksum.unwrap_or_default();
                    let skip_verify_requirements = cli_skip_verify_requirements || options.skip_verify_requirements.unwrap_or_default();
                    preview.options = Some(
                        options
                            .with_allowed(allowed)
                            .with_dry_run(Some(dry_run))
                            .with_gpu_monitor(Some(gpu_monitor))
                            .with_offline(Some(offline))
                            .with_resume(Some(resume))
                            .with_skip_verify_checksum(Some(skip_verify_checksum))
                            .with_skip_verify_requirements(Some(skip_verify_requirements))
                            .with_container_cleanup(cli_container_cleanup)
                            .with_forward_container_env(!cli_no_container_env),
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
                        .map(|mut resolved| {
                            if let Some(output) = artifact_output.as_ref().and_then(|path| path.to_str()) {
                                resolved.output = Some(output.to_string());
                                resolved
                                    .variables
                                    .get_or_insert_default()
                                    .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
                                resolved.modules.iter_mut().for_each(|module| {
                                    module
                                        .variables
                                        .get_or_insert_default()
                                        .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
                                });
                            }
                            resolved
                        })
                        .map_err(|why| {
                            let exit_code = match why.is_availability() {
                                | true => exitcode::UNAVAILABLE,
                                | false => exitcode::DATAERR,
                            };
                            RunFailure::new(exit_code, why.to_string())
                        })
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
                .resolve_inputs(cli_input.as_deref(), &config_path)
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
            ExecutionPlan::build_resolved(&preview, config_path.display().to_string(), &inputs, resume)
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|plan| (preview, inputs.values, plan))
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
                                module.module_path.join("::"),
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
                                                    .with_root(Some(root.clone()))
                                                    .with_container_run_id(Some(id.clone()));
                                                debug!("=> {} Run configuration options = {}", Label::using(), format!("{options:#?}").dimmed());
                                                let required_modules = Ok(plan
                                                    .modules
                                                    .iter()
                                                    .filter(|module| module.decision == "would_run")
                                                    .map(|module| module.module_index)
                                                    .collect::<BTreeSet<_>>());
                                                let modules = required_modules.and_then(|required_modules| {
                                                    Store::system()
                                                        .and_then(|store| {
                                                            Manifest::from_config(id.clone(), &config_path, execution.limits().threads())
                                                                .and_then(|manifest| store.start(&manifest))
                                                        })
                                                        .map(|bundle| (required_modules, bundle))
                                                        .map_err(|why| RunFailure::new(exitcode::CANTCREAT, why.to_string()))
                                                });
                                                match modules {
                                                    | Ok((required_modules, bundle)) => {
                                                        let journal_path = bundle.journal_path();
                                                        let journal_inputs = inputs
                                                            .iter()
                                                            .cloned()
                                                            .enumerate()
                                                            .map(|(index, value)| JournalInput::new(index, value))
                                                            .collect::<Vec<_>>();
                                                        let journal_modules = cfg
                                                            .modules
                                                            .iter()
                                                            .enumerate()
                                                            .map(|(index, module)| {
                                                                JournalModule::new(index, module.name.clone(), module.module_path.clone())
                                                            })
                                                            .collect::<Vec<_>>();
                                                        let run_id = id.clone();
                                                        let configuration = ConfigurationIdentity {
                                                            path: config_path.display().to_string(),
                                                            id: cfg.id.clone(),
                                                            version: cfg.version.clone(),
                                                        };
                                                        let dry_run = options.dry_run.unwrap_or_default();
                                                        let metadata = RunMetadata::now(run_id, configuration, dry_run);
                                                        let journal_containers = plan
                                                            .containers
                                                            .iter()
                                                            .enumerate()
                                                            .map(|(index, container)| JournalContainer {
                                                                index,
                                                                name: match container.action.as_str() {
                                                                    | "would_create" => managed_container_name(&container.name, &id),
                                                                    | _ => container.name.clone(),
                                                                },
                                                                owned: container.action == "would_create",
                                                                runtime: container.runtime.to_string(),
                                                            })
                                                            .collect::<Vec<_>>();
                                                        let create_journal = RunJournal::create_with_containers(
                                                            journal_path.clone(),
                                                            metadata,
                                                            journal_inputs,
                                                            journal_modules,
                                                            journal_containers.clone(),
                                                        );
                                                        let create_journal_result = create_journal.map_err(|why| {
                                                            RunFailure::new(exitcode::CANTCREAT, format!("create durable execution journal — {why}"))
                                                        });
                                                        match create_journal_result {
                                                            | Ok(journal) => {
                                                                let journal = Arc::new(journal);
                                                                let system_monitor = SystemMonitor::start(bundle.clone());
                                                                let module_executor = LoggedModuleExecutor {
                                                                    bundle: bundle.clone(),
                                                                    cancellation: cancellation.clone().unwrap_or_default(),
                                                                };
                                                                info!(path = %journal_path.display(), run_id = id, "=> {} Execution journal", Label::using());
                                                                let progress_disabled =
                                                                    no_progress || !io::stderr().is_terminal() || env::var_os("CI").is_some();
                                                                let progress = WorkflowProgress::new(
                                                                    cfg.modules.len() as u64,
                                                                    cfg.modules.len().saturating_mul(inputs.len()) as u64,
                                                                    progress_disabled,
                                                                );
                                                                let gpu_monitor_enabled = options.gpu_monitor.unwrap_or_default();
                                                                let mut gpu_monitor = None;
                                                                let monitoring = match gpu_monitor_enabled {
                                                                    | true => GpuMonitor::start(Arc::clone(&journal))
                                                                        .map(|monitor| gpu_monitor = Some(monitor))
                                                                        .map_err(RunFailure::from),
                                                                    | false => Ok(()),
                                                                };
                                                                let prepared = monitoring.and_then(|()| {
                                                                    cfg.with_options(options)
                                                                        .verify_requirements()
                                                                        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
                                                                        .map(Config::verify_resources)
                                                                        .and_then(|config| {
                                                                            config
                                                                                .prepare_modules_selected(
                                                                                    &execution,
                                                                                    &required_modules,
                                                                                    Some(&progress),
                                                                                )
                                                                                .map_err(|why| {
                                                                                    RunFailure::new(exitcode::UNAVAILABLE, why.to_string())
                                                                                })
                                                                        })
                                                                });
                                                                progress.finish_preparation(prepared.is_ok());
                                                                let dispatched = match prepared {
                                                                    | Ok(prepared) => {
                                                                        let options = prepared
                                                                            .options
                                                                            .clone()
                                                                            .unwrap_or_else(|| ConfigOptions::init().root(root.clone()).build());
                                                                        let concurrency = execution.input_concurrency(options.batch);
                                                                        debug!(concurrency, "=> {} Active workflow input limit", Label::using());
                                                                        let lifecycle = ContainerLifecycle::start(
                                                                            &prepared,
                                                                            &required_modules,
                                                                            options.root.as_deref().unwrap_or(root.as_path()),
                                                                            &id,
                                                                            options.offline.unwrap_or_default(),
                                                                            options.container_cleanup,
                                                                        )
                                                                        .map_err(|why| {
                                                                            journal_containers.iter().for_each(|container| {
                                                                                let _ = journal.transition_container(
                                                                                    container.index,
                                                                                    ContainerState::Failed,
                                                                                    Some(&why.to_string()),
                                                                                );
                                                                            });
                                                                            RunFailure::new(
                                                                                exitcode::UNAVAILABLE,
                                                                                format!("prepare containers — {why}"),
                                                                            )
                                                                        });
                                                                        lifecycle.and_then(|lifecycle| {
                                                                            journal_containers.iter().for_each(|container| {
                                                                                let state = match container.owned {
                                                                                    | true => ContainerState::Ready,
                                                                                    | false => ContainerState::External,
                                                                                };
                                                                                let _ = journal.transition_container(container.index, state, None);
                                                                            });
                                                                            let operation = |index, input: &Option<String>| {
                                                                                if module_executor.cancellation.is_requested() {
                                                                                    let message = "workflow cancellation requested";
                                                                                    let _ = journal.transition_input(
                                                                                        index,
                                                                                        ExecutionState::Cancelled,
                                                                                        Some(message),
                                                                                    );
                                                                                    return WorkflowOutcome::cancelled(input.clone(), message);
                                                                                }
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
                                                                                    .map_err(|why| format!("record pending input state — {why}"))
                                                                                    .and_then(|()| {
                                                                                        journal
                                                                                            .transition_input(index, ExecutionState::Running, None)
                                                                                            .map_err(|why| {
                                                                                                format!("record running input state — {why}")
                                                                                            })
                                                                                    });
                                                                                match transition {
                                                                                    | Err(message) => {
                                                                                        let state = ExecutionState::Failed;
                                                                                        let _ =
                                                                                            journal.transition_input(index, state, Some(&message));
                                                                                        WorkflowOutcome::failed(input.clone(), message)
                                                                                    }
                                                                                    | Ok(()) => {
                                                                                        let mut outcome = prepared.clone().run_workflow_recorded(
                                                                                            input.clone(),
                                                                                            &options,
                                                                                            Some(&progress),
                                                                                            Some(&module_executor),
                                                                                            Some(&journal),
                                                                                            index,
                                                                                        );
                                                                                        let terminal_transition = journal.transition_input(
                                                                                            index,
                                                                                            outcome.state,
                                                                                            outcome.error.as_deref(),
                                                                                        );
                                                                                        if let Err(why) = terminal_transition {
                                                                                            let message =
                                                                                                format!("record terminal input state — {why}");
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
                                                                            let dispatched = execution
                                                                                .dispatch_inputs_fail_fast(&inputs, options.batch, operation, failed)
                                                                                .map_err(|why| {
                                                                                    RunFailure::new(
                                                                                        exitcode::OSERR,
                                                                                        format!("initialize Xylem input worker pool — {why}"),
                                                                                    )
                                                                                });
                                                                            let succeeded = dispatched.as_ref().is_ok_and(|outcomes| {
                                                                                outcomes.iter().flatten().all(WorkflowOutcome::is_success)
                                                                            });
                                                                            let cleanup = lifecycle.finish(succeeded);
                                                                            match (dispatched, cleanup) {
                                                                                | (Ok(outcomes), Ok(finished)) => {
                                                                                    journal_containers
                                                                                        .iter()
                                                                                        .filter(|container| container.owned)
                                                                                        .for_each(|container| {
                                                                                            let state =
                                                                                                match finished.retained.contains(&container.name) {
                                                                                                    | true => ContainerState::Retained,
                                                                                                    | false => ContainerState::Removed,
                                                                                                };
                                                                                            let _ = journal.transition_container(
                                                                                                container.index,
                                                                                                state,
                                                                                                None,
                                                                                            );
                                                                                        });
                                                                                    finished.retained.iter().for_each(|name| {
                                                                                        info!(name, "=> Retained managed container")
                                                                                    });
                                                                                    Ok(outcomes)
                                                                                }
                                                                                | (Err(primary), Ok(_)) => Err(primary),
                                                                                | (Ok(_), Err(cleanup)) => {
                                                                                    journal_containers
                                                                                        .iter()
                                                                                        .filter(|container| container.owned)
                                                                                        .for_each(|container| {
                                                                                            let _ = journal.transition_container(
                                                                                                container.index,
                                                                                                ContainerState::Failed,
                                                                                                Some(&cleanup.to_string()),
                                                                                            );
                                                                                        });
                                                                                    Err(RunFailure::new(
                                                                                        exitcode::IOERR,
                                                                                        format!("clean up containers — {cleanup}"),
                                                                                    ))
                                                                                }
                                                                                | (Err(primary), Err(cleanup)) => {
                                                                                    journal_containers
                                                                                        .iter()
                                                                                        .filter(|container| container.owned)
                                                                                        .for_each(|container| {
                                                                                            let _ = journal.transition_container(
                                                                                                container.index,
                                                                                                ContainerState::Failed,
                                                                                                Some(&cleanup.to_string()),
                                                                                            );
                                                                                        });
                                                                                    Err(primary
                                                                                        .with_secondary(format!("clean up containers — {cleanup}")))
                                                                                }
                                                                            }
                                                                        })
                                                                    }
                                                                    | Err(why) => Err(why),
                                                                };
                                                                let monitoring = gpu_monitor
                                                                    .map(GpuMonitor::stop)
                                                                    .transpose()
                                                                    .map(|_| ())
                                                                    .map_err(RunFailure::from);
                                                                let system_monitoring = system_monitor
                                                                    .stop()
                                                                    .map_err(|why| RunFailure::new(exitcode::IOERR, why.to_string()));
                                                                let monitoring = match (monitoring, system_monitoring) {
                                                                    | (Ok(()), Ok(())) => Ok(()),
                                                                    | (Err(primary), Ok(())) => Err(primary),
                                                                    | (Ok(()), Err(system)) => Err(system),
                                                                    | (Err(primary), Err(system)) => Err(primary.with_secondary(system)),
                                                                };
                                                                let dispatched = match (dispatched, monitoring) {
                                                                    | (Ok(outcomes), Ok(())) => Ok(outcomes),
                                                                    | (Err(primary), Ok(())) => Err(primary),
                                                                    | (Ok(_), Err(monitoring)) => Err(monitoring),
                                                                    | (Err(primary), Err(monitoring)) => Err(primary.with_secondary(monitoring)),
                                                                };
                                                                let mut primary_failure = match &dispatched {
                                                                    | Ok(outcomes) => {
                                                                        outcomes.iter().flatten().find(|outcome| !outcome.is_success()).map(
                                                                            |outcome| {
                                                                                let exit_code = match outcome.state.is_cancelled() {
                                                                                    | true => CANCELLED_EXIT_CODE,
                                                                                    | false => exitcode::SOFTWARE,
                                                                                };
                                                                                RunFailure::new(
                                                                                    exit_code,
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
                                                                let cancelled = dispatched.as_ref().is_ok_and(|outcomes| {
                                                                    outcomes.iter().flatten().any(|outcome| outcome.state.is_cancelled())
                                                                });
                                                                let final_status = match (cancelled, primary_failure.as_ref()) {
                                                                    | (true, _) => FinalStatus::Cancelled,
                                                                    | (false, Some(failure)) => FinalStatus::from(failure),
                                                                    | (false, None) => FinalStatus::Succeeded,
                                                                };
                                                                let final_error = primary_failure.as_ref().map(ToString::to_string);
                                                                if let Err(why) = journal.finish(final_status, final_error.as_deref()) {
                                                                    primary_failure = Some(match primary_failure {
                                                                        | Some(failure) => {
                                                                            failure.with_secondary(format!("finalize execution journal — {why}"))
                                                                        }
                                                                        | None => RunFailure::new(
                                                                            exitcode::IOERR,
                                                                            format!("finalize execution journal — {why}"),
                                                                        ),
                                                                    });
                                                                }
                                                                let summary = match RunJournal::read(&journal_path) {
                                                                    | Ok(summary) => Some(summary),
                                                                    | Err(why) => {
                                                                        primary_failure = Some(match primary_failure {
                                                                            | Some(failure) => {
                                                                                failure.with_secondary(format!("recover execution journal — {why}"))
                                                                            }
                                                                            | None => RunFailure::new(
                                                                                exitcode::IOERR,
                                                                                format!("recover execution journal — {why}"),
                                                                            ),
                                                                        });
                                                                        None
                                                                    }
                                                                };
                                                                if let Some(summary) = summary {
                                                                    if let Err(why) = bundle.write_summary(&summary) {
                                                                        primary_failure = Some(match primary_failure {
                                                                            | Some(failure) => {
                                                                                failure.with_secondary(format!("write canonical run summary — {why}"))
                                                                            }
                                                                            | None => RunFailure::new(
                                                                                exitcode::IOERR,
                                                                                format!("write canonical run summary — {why}"),
                                                                            ),
                                                                        });
                                                                    }
                                                                    let selected = deduplicate(reporter);
                                                                    if let Some(directory) = report_output {
                                                                        let copied = create_dir_all(directory)
                                                                            .and_then(|()| {
                                                                                copy(&journal_path, directory.join(format!("{id}.journal.jsonl")))
                                                                            })
                                                                            .map(|_| ());
                                                                        if let Err(why) = copied {
                                                                            primary_failure = Some(match primary_failure {
                                                                                | Some(failure) => failure
                                                                                    .with_secondary(format!("export execution journal — {why}")),
                                                                                | None => RunFailure::new(
                                                                                    exitcode::IOERR,
                                                                                    format!("export execution journal — {why}"),
                                                                                ),
                                                                            });
                                                                        }
                                                                    }
                                                                    if should_write_journal_location(&selected) {
                                                                        let stderr = io::stderr();
                                                                        if let Err(why) =
                                                                            writeln!(stderr.lock(), "Journal: {}", journal_path.display())
                                                                        {
                                                                            primary_failure = Some(match primary_failure {
                                                                                | Some(failure) => {
                                                                                    failure.with_secondary(format!("write journal location — {why}"))
                                                                                }
                                                                                | None => RunFailure::new(
                                                                                    exitcode::IOERR,
                                                                                    format!("write journal location — {why}"),
                                                                                ),
                                                                            });
                                                                        }
                                                                    }
                                                                    let render = summary.render(&selected, report_output.as_deref(), &journal_path);
                                                                    if let Err(why) = render {
                                                                        primary_failure = Some(match primary_failure {
                                                                            | Some(failure) => {
                                                                                failure.with_secondary(format!("render run report — {why}"))
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

fn should_write_journal_location(reporters: &[ReporterType]) -> bool {
    !reporters.is_empty() && !reporters.contains(&ReporterType::Console)
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

    #[test]
    fn test_embedded_run_does_not_write_journal_location() {
        assert!(!should_write_journal_location(&[]));
        assert!(!should_write_journal_location(&[ReporterType::Console]));
        assert!(should_write_journal_location(&[ReporterType::Json]));
    }
}
