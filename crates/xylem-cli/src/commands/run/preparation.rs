use super::{RunFailure, RunRequest};
use crate::cli::DEFAULT_CONFIG_NAMES;
use acorn_host::config::resolve_config_path;
use acorn_schema::validation::Validate;
use std::env;
use std::path::{absolute, Path, PathBuf};
use xylem_lib::acquisition::{AcquisitionCache, AcquisitionLimits, AcquisitionPolicy};
use xylem_lib::config::{Config, ConfigOptions, ConfigReadError, FailurePolicy, WorkflowArgumentsError};
use xylem_lib::plan::ExecutionPlan;
use xylem_lib::run::RunContext;
use xylem_lib::sandbox::{EffectiveSandboxPolicy, SandboxProviderKind, DOCKER_SANDBOX_WARNING, OPENSHELL_WARNING};
use xylem_lib::script::EnvironmentValue;
use xylem_lib::workspace::RunWorkspace;

pub(super) struct PreparedRun {
    pub(super) config: Config,
    pub(super) config_path: PathBuf,
    pub(super) context: Option<RunContext>,
    pub(super) inputs: Vec<Option<String>>,
    pub(super) plan: ExecutionPlan,
}
struct ResolvedRun {
    config: Config,
    id: Option<String>,
    workspace: Option<RunWorkspace>,
}
fn apply_preview_artifact_output(config: &mut Config, output: &Path) -> Result<(), RunFailure> {
    output.to_str().map_or_else(
        || Err(RunFailure::new(exitcode::USAGE, "--artifact-output must be valid UTF-8")),
        |output| {
            config.output = Some(output.to_string());
            config
                .variables
                .get_or_insert_default()
                .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
            config.modules.iter_mut().for_each(|module| {
                if let Some(variables) = &mut module.variables {
                    if variables.contains_key("OUTPUT_DIRECTORY") {
                        variables.insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
                    }
                }
            });
            Ok(())
        },
    )
}
fn apply_resolved_artifact_output(config: &mut Config, output: &Path) -> Result<(), RunFailure> {
    output.to_str().map_or_else(
        || Err(RunFailure::new(exitcode::USAGE, "--artifact-output must be valid UTF-8")),
        |output| {
            config.output = Some(output.to_string());
            config
                .variables
                .get_or_insert_default()
                .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
            config.modules.iter_mut().for_each(|module| {
                module
                    .variables
                    .get_or_insert_default()
                    .insert("OUTPUT_DIRECTORY".to_string(), serde_json::Value::String(output.to_string()));
            });
            Ok(())
        },
    )
}
pub(super) fn configure_preview(mut preview: Config, request: &RunRequest<'_>, artifact_output: Option<&Path>) -> Result<Config, RunFailure> {
    let effective_sandbox = match (preview.sandbox, request.sandbox_policy.as_deref()) {
        | (false, None) => Ok(None),
        | (false, Some(_)) => Err(RunFailure::new(
            exitcode::USAGE,
            "--sandbox-policy requires workflow configuration sandbox: true",
        )),
        | (true, None) => Err(RunFailure::new(
            exitcode::USAGE,
            "workflow configuration sandbox: true requires --sandbox-policy <PATH>",
        )),
        | (true, Some(path)) => EffectiveSandboxPolicy::read(path)
            .map(|policy| {
                match policy.provider {
                    | SandboxProviderKind::DockerSandbox => eprintln!("Warning: {DOCKER_SANDBOX_WARNING}"),
                    | SandboxProviderKind::OpenShell => eprintln!("Warning: {OPENSHELL_WARNING}"),
                    | SandboxProviderKind::Oci => (),
                }
                Some(policy)
            })
            .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string())),
    };
    effective_sandbox.and_then(|effective_sandbox| {
        artifact_output
            .map_or(Ok(()), |output| apply_preview_artifact_output(&mut preview, output))
            .map(|()| {
                let options = preview.options.take().unwrap_or_else(|| ConfigOptions::init().build());
                let allowed = request.cli_allowed.clone().or(options.allowed.clone());
                let dry_run = request.cli_dry_run || options.dry_run.unwrap_or_default();
                let failure_policy = request.cli_failure_policy.unwrap_or(options.failure_policy);
                let gpu_monitor = request.cli_gpu_monitor || options.gpu_monitor.unwrap_or_default();
                let offline = request.cli_offline || options.offline.unwrap_or_default();
                let resume = request.cli_resume || options.resume.unwrap_or_default();
                let skip_verify_checksum = request.cli_skip_verify_checksum || options.skip_verify_checksum.unwrap_or_default();
                let skip_verify_requirements = request.cli_skip_verify_requirements || options.skip_verify_requirements.unwrap_or_default();
                preview.options = Some(
                    options
                        .with_allowed(allowed)
                        .with_allow_insecure_http(request.cli_allow_insecure_http)
                        .with_dry_run(Some(dry_run))
                        .with_failure_policy(failure_policy)
                        .with_gpu_monitor(Some(gpu_monitor))
                        .with_offline(Some(offline))
                        .with_resume(Some(resume))
                        .with_skip_verify_checksum(Some(skip_verify_checksum))
                        .with_skip_verify_requirements(Some(skip_verify_requirements))
                        .with_container_cleanup(request.cli_container_cleanup)
                        .with_forward_container_env(!request.cli_no_container_env)
                        .with_sandbox_policy(effective_sandbox),
                );
                preview
            })
    })
}
fn normalize_sandbox_artifact_output(config: &mut Config) -> Result<(), RunFailure> {
    match (config.sandbox, config.output.as_ref()) {
        | (true, Some(output)) => std::path::absolute(output)
            .map_err(|why| RunFailure::new(exitcode::DATAERR, format!("resolve sandbox artifact output {output} — {why}")))
            .and_then(|output| apply_resolved_artifact_output(config, &output)),
        | _ => Ok(()),
    }
}
pub(super) fn prepare(request: &RunRequest<'_>) -> Result<PreparedRun, RunFailure> {
    env::current_dir()
        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, format!("resolve current directory — {why}")))
        .and_then(|directory| {
            resolve_config_path(request.config.as_deref(), &directory, &DEFAULT_CONFIG_NAMES)
                .ok_or_else(|| {
                    RunFailure::new(
                        exitcode::USAGE,
                        format!("no workflow configuration found; expected one of {}", DEFAULT_CONFIG_NAMES.join(", ")),
                    )
                })
                .map(|config_path| (config_path, directory))
        })
        .and_then(|(config_path, directory)| prepare_config(request, config_path, directory))
}
fn prepare_config(request: &RunRequest<'_>, config_path: PathBuf, invocation_directory: PathBuf) -> Result<PreparedRun, RunFailure> {
    resolve_artifact_output(request, &invocation_directory)
        .and_then(|artifact_output| prepare_config_with_output(request, config_path, invocation_directory, artifact_output))
}
fn prepare_config_with_output(
    request: &RunRequest<'_>,
    config_path: PathBuf,
    invocation_directory: PathBuf,
    artifact_output: Option<PathBuf>,
) -> Result<PreparedRun, RunFailure> {
    Config::try_from(config_path.as_path())
        .map_err(|why| {
            let exit_code = match &why {
                | ConfigReadError::UnsupportedExtension(_) => exitcode::USAGE,
                | ConfigReadError::Invalid { .. } => exitcode::DATAERR,
                | ConfigReadError::Read { .. } => exitcode::UNAVAILABLE,
            };
            RunFailure::new(exit_code, why.to_string())
        })
        .and_then(|preview| {
            preview
                .bind_arguments(request.positional, &request.variable_overrides)
                .map_err(|why| {
                    let code = match why {
                        | WorkflowArgumentsError::InvalidDeclaration(_) | WorkflowArgumentsError::DuplicateDeclaration(_) => exitcode::DATAERR,
                        | WorkflowArgumentsError::TooManyValues { .. } => exitcode::USAGE,
                    };
                    RunFailure::new(code, why.to_string())
                })
                .and_then(|bindings| {
                    let mut preview = bindings.apply_root(preview);
                    let output = match artifact_output.as_ref() {
                        | Some(_) => Ok(None),
                        | None => preview.expanded_output(),
                    };
                    output
                        .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                        .and_then(|output| {
                            if let Some(output) = output {
                                preview.output = Some(output);
                            }
                            configure_preview(preview, request, artifact_output.as_deref())
                        })
                        .and_then(|mut preview| {
                            match (request.artifact_output.is_some(), preview.output.as_ref()) {
                                | (false, Some(output)) if Path::new(output).is_relative() => config_path
                                    .parent()
                                    .ok_or_else(|| RunFailure::new(exitcode::DATAERR, "workflow configuration has no parent directory"))
                                    .and_then(|parent| {
                                        absolute(parent.join(output))
                                            .map_err(|why| RunFailure::new(exitcode::DATAERR, format!("resolve workflow output path — {why}")))
                                    })
                                    .and_then(|output| {
                                        output
                                            .to_str()
                                            .map(str::to_string)
                                            .ok_or_else(|| RunFailure::new(exitcode::DATAERR, "workflow output path must be valid UTF-8"))
                                    })
                                    .map(|output| preview.output = Some(output)),
                                | _ => Ok(()),
                            }
                            .map(|()| preview)
                        })
                        .and_then(|preview| {
                            preview
                                .options
                                .as_ref()
                                .and_then(|options| options.sandbox_policy.as_ref())
                                .map_or(Ok(()), |policy| policy.validate_configuration(&preview))
                                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                                .map(|()| preview)
                        })
                        .and_then(|preview| {
                            let options = preview.options.as_ref();
                            let allowed = options.and_then(|value| value.allowed.as_deref());
                            let dry_run = options.and_then(|value| value.dry_run).unwrap_or_default();
                            let offline = options.and_then(|value| value.offline).unwrap_or_default();
                            let allow_insecure_http = options.is_some_and(|value| value.allow_insecure_http);
                            let skip_verify_checksum = options.and_then(|value| value.skip_verify_checksum).unwrap_or_default();
                            let limits = options
                                .and_then(|value| value.acquisition_limits.clone())
                                .unwrap_or_else(AcquisitionLimits::default);
                            AcquisitionPolicy::from_hosts(allowed, allow_insecure_http, None, offline, limits)
                                .map(|policy| policy.with_checksum_verification_disabled(skip_verify_checksum))
                                .map_err(|why| RunFailure::new(exitcode::USAGE, format!("invalid acquisition policy — {why}")))
                                .and_then(|policy| {
                                    AcquisitionCache::system()
                                        .map_err(|why| RunFailure::new(exitcode::UNAVAILABLE, why.to_string()))
                                        .map(|cache| (policy, cache))
                                })
                                .and_then(|(policy, cache)| {
                                    preview
                                        .resolve_with_policy_and_bindings(&config_path, &policy, &cache, dry_run, &bindings)
                                        .map_err(|why| {
                                            let exit_code = match why.is_availability() {
                                                | true => exitcode::UNAVAILABLE,
                                                | false => exitcode::DATAERR,
                                            };
                                            RunFailure::new(exit_code, why.to_string())
                                        })
                                        .map(|resolved| bindings.apply_resolved(resolved))
                                        .and_then(|mut resolved| {
                                            artifact_output
                                                .as_ref()
                                                .map_or(Ok(()), |output| apply_resolved_artifact_output(&mut resolved, output))
                                                .map(|()| resolved)
                                        })
                                        .and_then(|resolved| {
                                            let owned = match dry_run {
                                                | true => Ok((None, None)),
                                                | false => {
                                                    let id = nanoid::nanoid!(10, &nanoid::alphabet::SAFE);
                                                    let parent = env::temp_dir().join(EnvironmentValue::PREFIX).join("workspaces");
                                                    RunWorkspace::create(&parent, &id)
                                                        .map(|workspace| (Some(id), Some(workspace)))
                                                        .map_err(|why| RunFailure::new(exitcode::CANTCREAT, why.to_string()))
                                                }
                                            };
                                            owned.map(|(id, workspace)| ResolvedRun {
                                                config: resolved,
                                                id,
                                                workspace,
                                            })
                                        })
                                })
                        })
                })
        })
        .and_then(|mut resolved| normalize_sandbox_artifact_output(&mut resolved.config).map(|()| resolved))
        .and_then(|resolved| {
            resolved
                .config
                .validate()
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|()| resolved)
        })
        .and_then(|mut resolved| match request.module_index {
            | Some(index) => match resolved.config.modules.get(index).cloned() {
                | Some(module) => {
                    resolved.config.modules = vec![module];
                    Ok(resolved)
                }
                | None => Err(RunFailure::new(
                    exitcode::USAGE,
                    format!(
                        "module index {index} is out of range for {} resolved modules",
                        resolved.config.modules.len()
                    ),
                )),
            },
            | None => Ok(resolved),
        })
        .and_then(|resolved| {
            resolved
                .config
                .options
                .as_ref()
                .and_then(|options| options.sandbox_policy.as_ref())
                .map_or(Ok(()), |policy| {
                    policy.validate_input_discovery(&resolved.config, &config_path, request.cli_input.as_deref())
                })
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|()| resolved)
        })
        .and_then(|resolved| {
            resolved
                .config
                .resolve_inputs(request.cli_input.as_deref(), &config_path)
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|inputs| (resolved, inputs))
        })
        .and_then(|(resolved, inputs)| {
            resolved
                .config
                .options
                .as_ref()
                .and_then(|options| options.sandbox_policy.as_ref())
                .map_or(Ok(()), |policy| policy.validate_workflow(&resolved.config, &inputs.values))
                .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                .map(|()| (resolved, inputs))
        })
        .and_then(|(resolved, inputs)| {
            let resume = resolved.config.options.as_ref().and_then(|options| options.resume).unwrap_or_default();
            ExecutionPlan::build_resolved_with_limits(&resolved.config, config_path.display().to_string(), &inputs, resume, Some(request.limits))
                .map_err(|why| {
                    let exit_code = match why.is_availability() {
                        | true => exitcode::UNAVAILABLE,
                        | false => exitcode::DATAERR,
                    };
                    RunFailure::new(exit_code, why.to_string())
                })
                .and_then(|plan| {
                    let context = resolved.id.zip(resolved.workspace).map_or(Ok(None), |(id, workspace)| {
                        config_path
                            .parent()
                            .map(Path::to_path_buf)
                            .ok_or_else(|| RunFailure::new(exitcode::DATAERR, "workflow configuration has no parent directory"))
                            .and_then(|configuration_directory| {
                                let options = resolved.config.options.as_ref();
                                RunContext::init()
                                    .maybe_batch(options.and_then(|options| options.batch))
                                    .cancellation(request.cancellation.clone().unwrap_or_default())
                                    .failure_policy(options.map_or_else(FailurePolicy::default, |options| options.failure_policy))
                                    .limits(request.limits)
                                    .retain_workspace(request.retain_workspace)
                                    .run_id(id)
                                    .and_then(|builder| builder.configuration_directory(configuration_directory))
                                    .and_then(|builder| builder.invocation_directory(invocation_directory))
                                    .and_then(|builder| builder.maybe_artifact_output(artifact_output.clone()))
                                    .and_then(|builder| builder.workspace(workspace))
                                    .map(|builder| Some(builder.build()))
                                    .map_err(|why| RunFailure::new(exitcode::DATAERR, why.to_string()))
                            })
                    });
                    context.map(|context| PreparedRun {
                        config: resolved.config,
                        config_path,
                        context,
                        inputs: inputs.values,
                        plan,
                    })
                })
        })
}
fn resolve_artifact_output(request: &RunRequest<'_>, invocation_directory: &Path) -> Result<Option<PathBuf>, RunFailure> {
    request.artifact_output.as_ref().map_or(Ok(None), |output| {
        let output = match output.is_absolute() {
            | true => output.clone(),
            | false => invocation_directory.join(output),
        };
        absolute(&output)
            .map(Some)
            .map_err(|why| RunFailure::new(exitcode::USAGE, format!("resolve --artifact-output {} — {why}", output.display())))
    })
}
