use super::dispatch::{dispatch_inputs, DispatchContext};
use super::preparation::PreparedRun;
use super::progress::WorkflowProgress;
use super::reporting::{finalize, ReportContext};
use super::{RunFailure, RunRequest};
use acorn_host::terminal::Label;
use acorn_schema::hardware::Memory;
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use owo_colors::OwoColorize;
use std::env;
use std::fs::create_dir_all;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use tracing::{debug, info};
use xylem_lib::config::{Config, ConfigOptions};
use xylem_lib::execution::ExecutionContext;
use xylem_lib::gpu::GpuMonitor;
use xylem_lib::journal::{
    ConfigurationIdentity, JournalContainer, JournalInput, JournalModule, RunJournal, RunMemoryEvidence, RunMetadata, SandboxEvidence,
};
use xylem_lib::outcome::WorkflowOutcome;
use xylem_lib::plan::{ExecutionPlan, MemoryPlanMode};
use xylem_lib::remote::RemotePolicy;
use xylem_lib::run::store::{Bundle, Manifest, Store};
use xylem_lib::run::telemetry::SystemMonitor;
use xylem_lib::sandbox::{ArtifactPublication, ArtifactPublicationStatus, SandboxCapabilities, SandboxProvider, SandboxSession, WorkloadAcquisition};
use xylem_lib::script::{managed_container_name, EnvironmentValue};

struct Runtime<'a> {
    config: Config,
    config_path: PathBuf,
    execution: ExecutionContext,
    id: String,
    inputs: Vec<Option<String>>,
    plan: ExecutionPlan,
    request: &'a RunRequest<'a>,
    root: PathBuf,
    sandbox_capabilities: Option<SandboxCapabilities>,
}
fn create_journal(runtime: &Runtime<'_>, bundle: &Bundle, options: &ConfigOptions) -> Result<(RunJournal, Vec<JournalContainer>), RunFailure> {
    let journal_path = bundle.journal_path();
    let journal_inputs = runtime
        .inputs
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, value)| JournalInput::new(index, value))
        .collect::<Vec<_>>();
    let journal_modules = runtime
        .config
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| JournalModule::new(index, module.name.clone(), module.module_path.clone()))
        .collect::<Vec<_>>();
    let configuration = ConfigurationIdentity {
        path: runtime.config_path.display().to_string(),
        id: runtime.config.id.clone(),
        version: runtime.config.version.clone(),
    };
    let sandbox = match (options.sandbox_policy.as_ref(), runtime.sandbox_capabilities.clone()) {
        | (Some(policy), Some(capabilities)) => SandboxEvidence::required(policy, capabilities),
        | _ => SandboxEvidence::off(),
    };
    let journal_containers = runtime
        .plan
        .containers
        .iter()
        .enumerate()
        .map(|(index, container)| JournalContainer {
            index,
            name: match container.action.as_str() {
                | "would_create" => managed_container_name(&container.name, &runtime.id),
                | _ => container.name.clone(),
            },
            owned: container.action == "would_create",
            runtime: container.runtime.to_string(),
        })
        .collect::<Vec<_>>();
    runtime
        .plan
        .memory
        .as_ref()
        .map(RunMemoryEvidence::try_from)
        .transpose()
        .map_err(|why| RunFailure::new(exitcode::SOFTWARE, format!("derive durable memory evidence — {why}")))
        .and_then(|memory| {
            let metadata = RunMetadata::now(runtime.id.clone(), configuration, options.dry_run.unwrap_or_default(), sandbox);
            let metadata = match runtime.plan.artifacts.as_ref() {
                | Some(artifacts) => metadata.with_artifacts(ArtifactPublication {
                    bytes: Memory::kb(0.0),
                    bytes_per_input: artifacts.bytes_per_input.clone(),
                    destination: artifacts.destination.clone(),
                    entries: 0,
                    entries_per_input: artifacts.entries_per_input,
                    error: None,
                    status: ArtifactPublicationStatus::Pending,
                }),
                | None => metadata,
            };
            let metadata = match memory {
                | Some(memory) => metadata.with_memory(memory),
                | None => metadata,
            };
            RunJournal::create_with_containers(journal_path, metadata, journal_inputs, journal_modules, journal_containers.clone())
                .map(|journal| (journal, journal_containers))
                .map_err(|why| RunFailure::new(exitcode::CANTCREAT, format!("create durable execution journal — {why}")))
        })
}
pub(super) fn execute(request: &RunRequest<'_>, prepared: PreparedRun) -> Result<(), RunFailure> {
    if let Some((memory, fallback)) = prepared.plan.memory.as_ref().and_then(|memory| {
        (memory.mode == MemoryPlanMode::FixedFallback)
            .then(|| {
                memory
                    .inputs
                    .iter()
                    .find_map(|input| input.fallback.as_ref())
                    .map(|fallback| (memory, fallback))
            })
            .flatten()
    }) {
        eprintln!(
            "Warning: Memory-aware admission for estimator {} fell back to fixed input batching: {fallback}",
            memory.estimator.identity
        );
    }
    prepared
        .config
        .options
        .as_ref()
        .and_then(|options| options.sandbox_policy.as_ref())
        .map_or(Ok(None), |policy| policy.discover().map(Some))
        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
        .and_then(|sandbox_capabilities| {
            ExecutionContext::new(request.limits)
                .map(|execution| (execution, sandbox_capabilities))
                .map_err(|why| RunFailure::new(exitcode::OSERR, format!("initialize Xylem worker pool — {why}")))
        })
        .and_then(|(execution, sandbox_capabilities)| {
            debug!(target: "xylem_cli::commands::run", threads = execution.limits().threads(), "=> {} Rayon worker limit", Label::using());
            let id = nanoid::nanoid!(10, &nanoid::alphabet::SAFE);
            let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
            create_dir_all(&root)
                .map_err(|why| RunFailure::new(exitcode::CANTCREAT, format!("create run directory {} — {why}", root.display())))
                .and_then(|()| {
                    debug!(target: "xylem_cli::commands::run", path = %root.display(), "=> {} Root directory", Label::using());
                    execute_runtime(Runtime {
                        config: prepared.config,
                        config_path: prepared.config_path,
                        execution,
                        id,
                        inputs: prepared.inputs,
                        plan: prepared.plan,
                        request,
                        root,
                        sandbox_capabilities,
                    })
                })
        })
}
fn execute_bundle(runtime: Runtime<'_>, bundle: Bundle, options: ConfigOptions, required_modules: BTreeSet<usize>) -> Result<(), RunFailure> {
    create_journal(&runtime, &bundle, &options).and_then(|(journal, journal_containers)| {
        let acquisition = match options.offline.unwrap_or_default() {
            | true => WorkloadAcquisition::LocalOnly,
            | false => WorkloadAcquisition::PullIfMissing,
        };
        <dyn SandboxSession>::open(&runtime.id, options.sandbox_policy.as_ref(), acquisition)
            .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, format!("open sandbox provider session — {why}")))
            .map_err(|failure| {
                let message = failure.to_string();
                let artifacts = runtime.plan.artifacts.as_ref().map_or(Ok(()), |artifacts| {
                    journal.finalize_artifacts(ArtifactPublication {
                        bytes: Memory::kb(0.0),
                        bytes_per_input: artifacts.bytes_per_input.clone(),
                        destination: artifacts.destination.clone(),
                        entries: 0,
                        entries_per_input: artifacts.entries_per_input,
                        error: Some(message.clone()),
                        status: ArtifactPublicationStatus::Failed,
                    })
                });
                let finalized = artifacts.and_then(|()| journal.finish(xylem_lib::journal::FinalStatus::Failed, Some(&message)));
                match finalized {
                    | Ok(()) => failure,
                    | Err(why) => failure.with_secondary(format!("finalize execution journal — {why}")),
                }
            })
            .and_then(|sandbox| execute_opened(runtime, bundle, options, required_modules, journal, journal_containers, sandbox))
    })
}
fn execute_opened(
    runtime: Runtime<'_>,
    bundle: Bundle,
    options: ConfigOptions,
    required_modules: BTreeSet<usize>,
    journal: RunJournal,
    journal_containers: Vec<JournalContainer>,
    sandbox: Option<Arc<dyn SandboxSession>>,
) -> Result<(), RunFailure> {
    let Runtime {
        config,
        config_path: _,
        execution,
        id,
        inputs,
        plan,
        request,
        root,
        sandbox_capabilities: _,
    } = runtime;
    let journal_path = bundle.journal_path();
    let journal = Arc::new(journal);
    let system_monitor = SystemMonitor::start(bundle.clone());
    let run_cancellation = request.cancellation.clone().unwrap_or_default();
    info!(target: "xylem_cli::commands::run", path = %journal_path.display(), run_id = id, "=> {} Execution journal", Label::using());
    let progress_disabled = request.no_progress || !io::stderr().is_terminal() || env::var_os("CI").is_some();
    let preparation_total = match sandbox.is_some() {
        | true => required_modules.len().saturating_mul(inputs.len()),
        | false => config.modules.len(),
    };
    let progress = WorkflowProgress::new(
        preparation_total as u64,
        config.modules.len().saturating_mul(inputs.len()) as u64,
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
    let prepared = monitoring.and_then(|()| match sandbox.is_some() {
        | true => Ok(config.with_options(options)),
        | false => config
            .with_options(options)
            .verify_requirements()
            .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
            .map(Config::verify_resources)
            .and_then(|config| {
                config
                    .prepare_modules_selected(&execution, &required_modules, Some(&progress))
                    .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
            }),
    });
    if sandbox.is_none() {
        progress.finish_preparation(prepared.is_ok());
    }
    let dispatched = match prepared {
        | Err(why) => Err(why),
        | Ok(prepared) => {
            let options = prepared
                .options
                .clone()
                .unwrap_or_else(|| ConfigOptions::init().root(root.clone()).build());
            let input_kind = match request.cli_input.as_ref().is_some_and(|values| !values.is_empty()) {
                | true => "CLI",
                | false => "Configuration",
            };
            dispatch_inputs(DispatchContext {
                bundle: &bundle,
                config: &prepared,
                execution: &execution,
                id: &id,
                input_kind,
                inputs: &inputs,
                journal: &journal,
                journal_containers: &journal_containers,
                options: &options,
                plan: &plan,
                progress: &progress,
                required_modules: &required_modules,
                root: &root,
                run_cancellation: &run_cancellation,
                sandbox: sandbox.as_ref(),
            })
        }
    };
    let sandbox_succeeded = dispatched
        .as_ref()
        .is_ok_and(|outcomes| outcomes.iter().flatten().all(WorkflowOutcome::is_success));
    if sandbox.is_some() {
        progress.finish_preparation(sandbox_succeeded);
    }
    let sandbox_cleanup = sandbox.as_ref().map_or(Ok(()), |sandbox| match sandbox.finish(sandbox_succeeded) {
        | Ok(publication) => publication.map_or(Ok(()), |publication| {
            journal
                .finalize_artifacts(publication)
                .map_err(|why| RunFailure::new(exitcode::IOERR, format!("record sandbox artifact finalization — {why}")))
        }),
        | Err(why) => {
            let failure = RunFailure::new(exitcode::IOERR, format!("finish sandbox provider session — {why}"));
            let evidence = plan.artifacts.as_ref().map(|artifacts| ArtifactPublication {
                bytes: Memory::kb(0.0),
                bytes_per_input: artifacts.bytes_per_input.clone(),
                destination: artifacts.destination.clone(),
                entries: 0,
                entries_per_input: artifacts.entries_per_input,
                error: Some(why.to_string()),
                status: ArtifactPublicationStatus::Failed,
            });
            match evidence.map(|evidence| journal.finalize_artifacts(evidence)).transpose() {
                | Ok(_) => Err(failure),
                | Err(journal_error) => Err(failure.with_secondary(format!("record sandbox artifact failure — {journal_error}"))),
            }
        }
    });
    let dispatched = match (dispatched, sandbox_cleanup) {
        | (Ok(outcomes), Ok(())) => Ok(outcomes),
        | (Err(primary), Ok(())) => Err(primary),
        | (Ok(_), Err(cleanup)) => Err(cleanup),
        | (Err(primary), Err(cleanup)) => Err(primary.with_secondary(cleanup)),
    };
    let monitoring = gpu_monitor.map(GpuMonitor::stop).transpose().map(|_| ()).map_err(RunFailure::from);
    let system_monitoring = system_monitor.stop().map_err(|why| RunFailure::new(exitcode::IOERR, why.to_string()));
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
    finalize(
        ReportContext {
            bundle: &bundle,
            id: &id,
            journal: &journal,
            journal_path: &journal_path,
            progress: &progress,
            report_output: request.report_output,
            reporters: request.reporter,
        },
        dispatched,
    )
}
fn execute_runtime(runtime: Runtime<'_>) -> Result<(), RunFailure> {
    runtime
        .config
        .validate()
        .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
        .and_then(|()| {
            let allowed = runtime.config.options.as_ref().and_then(|value| value.allowed.clone());
            RemotePolicy::from_hosts(allowed.as_deref())
                .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid remote module allowlist — {why}")))
        })
        .and_then(|_| {
            let options = runtime
                .config
                .options
                .clone()
                .unwrap_or_else(|| ConfigOptions::init().build())
                .with_root(Some(runtime.root.clone()))
                .with_container_run_id(Some(runtime.id.clone()));
            debug!(target: "xylem_cli::commands::run", "=> {} Run configuration options = {}", Label::using(), format!("{options:#?}").dimmed());
            let required_modules = runtime
                .plan
                .modules
                .iter()
                .filter(|module| module.decision == "would_run")
                .map(|module| module.module_index)
                .collect::<BTreeSet<_>>();
            Store::system()
                .and_then(|store| {
                    Manifest::from_config(runtime.id.clone(), &runtime.config_path, runtime.execution.limits().threads())
                        .and_then(|manifest| store.start(&manifest))
                })
                .map_err(|why| RunFailure::new(exitcode::CANTCREAT, why.to_string()))
                .and_then(|bundle| execute_bundle(runtime, bundle, options, required_modules))
        })
}
