use super::dispatch::{dispatch_inputs, DispatchContext};
use super::preparation::PreparedRun;
use super::progress::WorkflowProgress;
use super::reporting::{finalize, ReportContext};
use super::{RunFailure, RunRequest};
use acorn_host::terminal::Label;
use acorn_schema::validation::Validate;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use owo_colors::OwoColorize;
use std::env;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use tracing::{debug, info};
use xylem_lib::config::{Config, ConfigOptions};
use xylem_lib::execution::ExecutionContext;
use xylem_lib::gpu::GpuMonitor;
use xylem_lib::journal::{
    ConfigurationIdentity, JournalContainer, JournalInput, JournalModule, RunJournal, RunMemoryEvidence, RunMetadata, SandboxEvidence,
};
use xylem_lib::plan::{ExecutionPlan, MemoryPlanMode};
use xylem_lib::run::store::{Bundle, Manifest, Store};
use xylem_lib::run::telemetry::SystemMonitor;
use xylem_lib::run::RunContext;
use xylem_lib::sandbox::{
    ArtifactPublication, ArtifactPublicationStatus, SandboxCapabilities, SandboxLease, SandboxProvider, SandboxResourceEvidence, SandboxSession,
    WorkloadAcquisition,
};
use xylem_lib::script::managed_container_name;

trait ExecutionPlanExt {
    fn finalize_unopened_sandbox(&self, journal: &RunJournal, reconciled: Vec<SandboxResourceEvidence>, failure: RunFailure) -> RunFailure;
}
struct OpenedSandbox {
    reconciled: Vec<SandboxResourceEvidence>,
    session: Option<Arc<dyn SandboxSession>>,
}
struct Runtime<'a> {
    config: Config,
    config_path: PathBuf,
    context: &'a RunContext,
    execution: ExecutionContext,
    inputs: Vec<Option<String>>,
    plan: ExecutionPlan,
    request: &'a RunRequest<'a>,
    sandbox_capabilities: Option<SandboxCapabilities>,
}
impl ExecutionPlanExt for ExecutionPlan {
    fn finalize_unopened_sandbox(&self, journal: &RunJournal, reconciled: Vec<SandboxResourceEvidence>, failure: RunFailure) -> RunFailure {
        let message = failure.to_string();
        let lifecycle = journal.finalize_sandbox(Vec::new(), reconciled, Some(&message));
        let artifacts = self.artifacts.as_ref().map_or(Ok(()), |artifacts| {
            journal.finalize_artifacts(
                ArtifactPublication::init()
                    .bytes_per_input(artifacts.bytes_per_input.clone())
                    .destination(artifacts.destination.as_str())
                    .entries_per_input(artifacts.entries_per_input)
                    .error(message.as_str())
                    .status(ArtifactPublicationStatus::Failed)
                    .build(),
            )
        });
        let finalized = lifecycle
            .and(artifacts)
            .and_then(|()| journal.finish(xylem_lib::journal::FinalStatus::Failed, Some(&message)));
        match finalized {
            | Ok(()) => failure,
            | Err(why) => failure.with_secondary(format!("finalize execution journal — {why}")),
        }
    }
}
impl Runtime<'_> {
    fn execute_bundle(
        self,
        bundle: Bundle,
        options: ConfigOptions,
        required_modules: BTreeSet<usize>,
        leases: Vec<SandboxLease>,
    ) -> Result<(), RunFailure> {
        create_journal(&self, &bundle, &options).and_then(|(journal, journal_containers)| {
            let acquisition = match options.offline.unwrap_or_default() {
                | true => WorkloadAcquisition::LocalOnly,
                | false => WorkloadAcquisition::PullIfMissing,
            };
            let reconciliation = options
                .sandbox_policy
                .as_ref()
                .map_or_else(Default::default, |policy| policy.reconcile(&leases));
            match reconciliation.error {
                | Some(error) => Err(self.plan.finalize_unopened_sandbox(
                    &journal,
                    reconciliation.resources,
                    RunFailure::new(exitcode::IOERR, format!("reconcile stale sandbox resources — {error}")),
                )),
                | None => <dyn SandboxSession>::open(self.context.run_id(), options.sandbox_policy.as_ref(), acquisition)
                    .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, format!("open sandbox provider session — {why}")))
                    .map_err(|failure| self.plan.finalize_unopened_sandbox(&journal, reconciliation.resources.clone(), failure))
                    .and_then(|sandbox| {
                        execute_opened(
                            self,
                            bundle,
                            options,
                            required_modules,
                            journal,
                            journal_containers,
                            OpenedSandbox {
                                reconciled: reconciliation.resources,
                                session: sandbox,
                            },
                        )
                    }),
            }
        })
    }
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
                | "would_create" => managed_container_name(&container.name, runtime.context.run_id()),
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
            let runtime_id = runtime.context.run_id().to_string();
            let dry_run = options.dry_run.unwrap_or_default();
            let acquisitions = runtime.config.acquisitions.clone();
            let base = RunMetadata::now(runtime_id, configuration, dry_run, sandbox).with_configuration_acquisitions(acquisitions);
            let artifact_metadata = match runtime.plan.artifacts.as_ref() {
                | Some(artifacts) => base.with_artifacts(
                    ArtifactPublication::init()
                        .bytes_per_input(artifacts.bytes_per_input.clone())
                        .destination(artifacts.destination.as_str())
                        .entries_per_input(artifacts.entries_per_input)
                        .status(ArtifactPublicationStatus::Pending)
                        .build(),
                ),
                | None => base,
            };
            let metadata = match memory {
                | Some(memory) => artifact_metadata.with_memory(memory),
                | None => artifact_metadata,
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
    let PreparedRun {
        config,
        config_path,
        context,
        inputs,
        plan,
    } = prepared;
    context
        .ok_or_else(|| RunFailure::new(exitcode::SOFTWARE, "real execution is missing a run context"))
        .and_then(|context| {
            let execution = config
                .options
                .as_ref()
                .and_then(|options| options.sandbox_policy.as_ref())
                .map_or(Ok(None), |policy| policy.discover().map(Some))
                .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
                .and_then(|sandbox_capabilities| {
                    ExecutionContext::new(context.limits())
                        .map(|execution| (execution, sandbox_capabilities))
                        .map_err(|why| RunFailure::new(exitcode::OSERR, format!("initialize Xylem worker pool — {why}")))
                })
                .and_then(|(execution, sandbox_capabilities)| {
                    debug!(target: "xylem_cli::commands::run", threads = execution.limits().threads(), "=> {} Rayon worker limit", Label::using());
                    debug!(target: "xylem_cli::commands::run", path = %context.workspace().root().display(), "=> {} Root directory", Label::using());
                    execute_runtime(Runtime {
                        config,
                        config_path,
                        context: &context,
                        execution,
                        inputs,
                        plan,
                        request,
                        sandbox_capabilities,
                    })
                });
            match context.retain_workspace() {
                | true => {
                    let path = context.retain();
                    eprintln!("Workspace: {}", path.display());
                    execution
                }
                | false => {
                    let cleanup = context
                        .close()
                        .map_err(|why| RunFailure::new(exitcode::IOERR, format!("remove run workspace — {why}")));
                    match (execution, cleanup) {
                        | (Ok(()), Ok(())) => Ok(()),
                        | (Err(primary), Ok(())) => Err(primary),
                        | (Ok(()), Err(cleanup)) => Err(cleanup),
                        | (Err(primary), Err(cleanup)) => Err(primary.with_secondary(cleanup)),
                    }
                }
            }
        })
}
fn execute_opened(
    runtime: Runtime<'_>,
    bundle: Bundle,
    options: ConfigOptions,
    required_modules: BTreeSet<usize>,
    journal: RunJournal,
    journal_containers: Vec<JournalContainer>,
    sandbox: OpenedSandbox,
) -> Result<(), RunFailure> {
    let OpenedSandbox {
        reconciled,
        session: sandbox,
    } = sandbox;
    let Runtime {
        config,
        config_path: _,
        context,
        execution,
        inputs,
        plan,
        request,
        sandbox_capabilities: _,
    } = runtime;
    let id = context.run_id();
    let root = context.workspace().root();
    let journal_path = bundle.journal_path();
    let journal = Arc::new(journal);
    let system_monitor = SystemMonitor::start(bundle.clone());
    info!(target: "xylem_cli::commands::run", path = %journal_path.display(), run_id = id, "=> {} Execution journal", Label::using());
    let progress_disabled = request.no_progress || !io::stderr().is_terminal() || env::var_os("CI").is_some();
    let preparation_total = required_modules.len().saturating_mul(inputs.len());
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
    let prepared = monitoring.and_then(|()| {
        config
            .with_options(options)
            .verify_requirements()
            .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
            .map(Config::verify_resources)
    });
    let dispatched = match prepared {
        | Err(why) => Err(why),
        | Ok(prepared) => {
            let options = prepared
                .options
                .clone()
                .unwrap_or_else(|| ConfigOptions::init().root(root.to_path_buf()).build());
            let input_kind = match request.cli_input.as_ref().is_some_and(|values| !values.is_empty()) {
                | true => "CLI",
                | false => "Configuration",
            };
            dispatch_inputs(DispatchContext {
                bundle: &bundle,
                config: &prepared,
                execution: &execution,
                input_kind,
                inputs: &inputs,
                journal: &journal,
                journal_containers: &journal_containers,
                options: &options,
                plan: &plan,
                progress: &progress,
                required_modules: &required_modules,
                run_context: context,
                sandbox: sandbox.as_ref(),
            })
        }
    };
    let sandbox_succeeded = dispatched.as_ref().is_ok_and(|report| report.is_success());
    progress.finish_preparation(sandbox_succeeded);
    let sandbox_cleanup = sandbox.as_ref().map_or(Ok(()), |sandbox| {
        let outcome = sandbox.finish(sandbox_succeeded);
        let lifecycle = journal
            .finalize_sandbox(outcome.resources, reconciled, outcome.error.as_deref())
            .map_err(|why| RunFailure::new(exitcode::IOERR, format!("record sandbox lifecycle finalization — {why}")));
        let evidence = outcome.artifacts.or_else(|| {
            plan.artifacts.as_ref().map(|artifacts| {
                ArtifactPublication::init()
                    .bytes_per_input(artifacts.bytes_per_input.clone())
                    .destination(artifacts.destination.as_str())
                    .entries_per_input(artifacts.entries_per_input)
                    .error(
                        outcome
                            .error
                            .as_deref()
                            .unwrap_or("sandbox session did not finalize configured artifacts"),
                    )
                    .status(ArtifactPublicationStatus::Failed)
                    .build()
            })
        });
        let artifacts = evidence.map_or(Ok(()), |evidence| {
            journal
                .finalize_artifacts(evidence)
                .map_err(|why| RunFailure::new(exitcode::IOERR, format!("record sandbox artifact finalization — {why}")))
        });
        let finalized = match (lifecycle, artifacts) {
            | (Ok(()), Ok(())) => Ok(()),
            | (Err(primary), Ok(())) => Err(primary),
            | (Ok(()), Err(artifacts)) => Err(artifacts),
            | (Err(primary), Err(artifacts)) => Err(primary.with_secondary(artifacts)),
        };
        match (outcome.error, finalized) {
            | (None, finalized) => finalized,
            | (Some(why), Ok(())) => Err(RunFailure::new(exitcode::IOERR, format!("finish sandbox provider session — {why}"))),
            | (Some(why), Err(finalization)) => {
                Err(RunFailure::new(exitcode::IOERR, format!("finish sandbox provider session — {why}")).with_secondary(finalization))
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
            id,
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
            let options = runtime
                .config
                .options
                .clone()
                .unwrap_or_else(|| ConfigOptions::init().build())
                .with_root(Some(runtime.context.workspace().root().to_path_buf()))
                .with_container_run_id(Some(runtime.context.run_id().to_string()));
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
                    store.sandbox_leases().and_then(|leases| {
                        Manifest::from_config(
                            runtime.context.run_id().to_string(),
                            &runtime.config_path,
                            runtime.execution.limits().threads(),
                        )
                        .and_then(|manifest| store.start(&manifest))
                        .map(|bundle| (bundle, leases))
                    })
                })
                .map_err(|why| RunFailure::new(exitcode::CANTCREAT, why.to_string()))
                .and_then(|(bundle, leases)| {
                    runtime
                        .context
                        .bind_journal_path(bundle.journal_path())
                        .map_err(|why| RunFailure::new(exitcode::CANTCREAT, why.to_string()))
                        .and_then(|()| runtime.execute_bundle(bundle, options, required_modules, leases))
                })
        })
}
