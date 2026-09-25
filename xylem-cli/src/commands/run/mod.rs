use acorn::util::Label;
use clap_verbosity_flag::Verbosity;
use color_eyre::eyre::Report;
use core::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nanoid::nanoid;
use owo_colors::OwoColorize;
use std::env;
use std::fs::create_dir_all;
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use xylem_lib::config::{Config, ConfigOptions};
use xylem_lib::execution::{ExecutionContext, ExecutionLimits};
use xylem_lib::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};
use xylem_lib::remote::RemotePolicy;
use xylem_lib::script::EnvironmentValue;

const DEFAULT_CONFIG_NAME: &str = "config.json";
struct WorkflowProgress {
    preparation: ProgressBar,
    execution: ProgressBar,
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
    fn finish_preparation(&self) {
        self.preparation.finish_with_message("Modules prepared");
    }
    fn finish_execution(&self) {
        self.execution.finish_with_message("Workflow complete");
    }
}
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
            | ProgressState::Finished => bar.inc(1),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    config: &Option<PathBuf>,
    _reporter: &Option<Vec<String>>,
    _output: &Option<PathBuf>,
    cli_input: &Option<Vec<String>>,
    cli_dry_run: bool,
    cli_offline: bool,
    cli_allowed: &Option<Vec<String>>,
    cli_skip_verify_checksum: bool,
    cli_skip_verify_requirements: bool,
    _verbose: Verbosity,
    limits: ExecutionLimits,
    no_progress: bool,
) -> Result<(), Report> {
    match ExecutionContext::new(limits) {
        | Err(why) => Err(color_eyre::eyre::eyre!("Initialize Xylem worker pool — {why}")),
        | Ok(execution) => {
            debug!(threads = execution.limits().threads(), "=> {} Rayon worker limit", Label::using());
            let id = nanoid!(10, &nanoid::alphabet::SAFE);
            let root = env::temp_dir().join(EnvironmentValue::PREFIX).join(id.clone());
            match create_dir_all(&root) {
                | Ok(_) => {
                    let path = root.clone().into_os_string().into_string().unwrap();
                    debug!(path, "=> {} Root directory", Label::using());
                }
                | Err(err) => error!("{}", err),
            }
            let default_config = match env::current_dir() {
                | Ok(path) => path.join(DEFAULT_CONFIG_NAME),
                | _ => PathBuf::from(DEFAULT_CONFIG_NAME),
            };
            let config_path = match config {
                | Some(value) => value.to_path_buf(),
                | None => {
                    let path = default_config.to_str().unwrap();
                    warn!(path, "=> {} Default configuration", Label::using());
                    default_config
                }
            };
            match Config::read(config_path) {
                | Some(cfg) => {
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
                    RemotePolicy::from_hosts(allowed.as_deref()).and_then(|_| {
                        let options = match &cfg.options {
                            | Some(ConfigOptions {
                                batch,
                                dry_run,
                                offline,
                                skip_verify_checksum,
                                skip_verify_requirements,
                                ..
                            }) => ConfigOptions::init()
                                .root(root.clone())
                                .maybe_batch(*batch)
                                .dry_run(cli_dry_run || dry_run.unwrap_or_default())
                                .offline(cli_offline || offline.unwrap_or_default())
                                .maybe_allowed(allowed.clone())
                                .skip_verify_checksum(cli_skip_verify_checksum || skip_verify_checksum.unwrap_or_default())
                                .skip_verify_requirements(cli_skip_verify_requirements || skip_verify_requirements.unwrap_or_default())
                                .build(),
                            | None => ConfigOptions::init()
                                .root(root.clone())
                                .dry_run(cli_dry_run)
                                .offline(cli_offline)
                                .maybe_allowed(allowed)
                                .skip_verify_checksum(cli_skip_verify_checksum)
                                .skip_verify_requirements(cli_skip_verify_requirements)
                                .build(),
                        };
                        debug!("=> {} Run configuration options = {}", Label::using(), format!("{options:#?}").dimmed());
                        let progress_disabled = no_progress || !std::io::stderr().is_terminal() || env::var_os("CI").is_some();
                        let progress = WorkflowProgress::new(
                            cfg.modules.len() as u64,
                            cfg.modules.len().saturating_mul(inputs.len()) as u64,
                            progress_disabled,
                        );
                        let config = cfg
                            .with_options(options.clone())
                            .verify_requirements()
                            .verify_resources()
                            .prepare_modules(&execution, Some(&progress))
                            .set_environment_variables();
                        progress.finish_preparation();
                        let input_concurrency = execution.input_concurrency(options.batch);
                        debug!(input_concurrency, "=> {} Active workflow input limit", Label::using());
                        let result = execution.dispatch_inputs(&inputs, options.batch, |input| {
                            match input {
                                | Some(value) => info!(value, source = input_kind, "=> {} Workflow input", Label::using()),
                                | None => {
                                    info!("=> {} Configuration with {}", Label::using(), Label::fmt_not_found("NO INPUTS"));
                                }
                            }
                            config.clone().run_workflow_with_progress(input.clone(), &options, Some(&progress));
                        });
                        progress.finish_execution();
                        result
                            .map(|_| ())
                            .map_err(|why| color_eyre::eyre::eyre!("Initialize Xylem input worker pool — {why}"))
                    })
                }
                | None => {
                    error!("=> {} Could not import configuration", Label::fail());
                    Ok(())
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
