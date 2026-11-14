//! Run-scoped container lifecycle management
extern crate alloc;

use crate::config::Config;
use crate::error::{ModuleLoadError, TemplateError};
use crate::script::{managed_container_name, ContainerCleanup, ContainerEnvironment, ContainerRuntime, ContainerStartup, Script};
use acorn_cmd::args;
use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use core::time::Duration;
use serde::Deserialize;
use std::ffi::OsString;
use std::fs::{canonicalize, metadata};
use std::path::Path;
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Instant;

const DEFAULT_STARTUP_TIMEOUT_SECONDS: u64 = 30;
#[derive(Debug)]
struct ActiveContainer {
    cleanup: ContainerCleanup,
    id: Option<String>,
    name: String,
    runtime: ContainerRuntime,
}
/// Failure while resolving or managing a container.
#[derive(Debug)]
pub struct ContainerError {
    message: String,
    module_load: Option<ModuleLoadError>,
}
/// Final lifecycle disposition for journaling and reporting.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ContainerFinish {
    /// External containers verified and left untouched.
    pub external: Vec<String>,
    /// Managed containers removed by policy.
    pub removed: Vec<String>,
    /// Managed containers retained by policy.
    pub retained: Vec<String>,
}
/// Containers prepared for one workflow run.
#[derive(Debug, Default)]
pub struct ContainerLifecycle {
    containers: Vec<ActiveContainer>,
}
#[derive(Debug, Deserialize)]
struct InspectContainer {
    #[serde(rename = "State")]
    state: InspectState,
}
#[derive(Debug, Deserialize)]
struct InspectHealth {
    #[serde(rename = "Status")]
    status: String,
}
#[derive(Debug, Deserialize)]
struct InspectState {
    #[serde(rename = "Health")]
    health: Option<InspectHealth>,
    #[serde(rename = "Running")]
    running: bool,
}
impl Config {
    fn container_declarations(
        &self,
        selected: &BTreeSet<usize>,
        root: &Path,
    ) -> Result<BTreeMap<(ContainerRuntime, String), ContainerEnvironment>, ContainerError> {
        self.modules
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.contains(index))
            .try_fold(BTreeMap::new(), |mut declarations, (_, module)| {
                Script::from_module(module.clone(), Some(root.to_path_buf()))
                    .map_err(ContainerError::from_module_load)
                    .and_then(|script| match script.command.container {
                        | None => Ok(declarations),
                        | Some(container) => {
                            let key = (container.runtime, container.name.clone());
                            match declarations.get(&key) {
                                | Some(existing) if existing != &container => Err(ContainerError::new(format!(
                                    "conflicting container declarations for {}:{}",
                                    container.runtime, container.name
                                ))),
                                | Some(_) => Ok(declarations),
                                | None => {
                                    declarations.insert(key, container);
                                    Ok(declarations)
                                }
                            }
                        }
                    })
            })
    }
}
impl ContainerEnvironment {
    fn create_managed(&self, name: &str, run_id: &str, offline: bool) -> Result<String, ContainerError> {
        self.image.as_deref().map_or_else(
            || Err(ContainerError::new(format!("managed container {} has no image", self.name))),
            |image| {
                self.mounts
                    .iter()
                    .try_fold(
                        args![
                            "run",
                            "--detach",
                            ("--name", name),
                            ("--label", "gov.ornl.xylem.managed=true"),
                            ("--label", format!("gov.ornl.xylem.run-id={run_id}")),
                            ("--label", format!("gov.ornl.xylem.logical-name={}", self.name)),
                            format!("--pull={}", if offline { "never" } else { "missing" })
                        ],
                        |arguments, mount| {
                            canonicalize(&mount.source)
                                .map_err(|error| ContainerError::new(format!("resolve mount source {} — {error}", mount.source)))
                                .and_then(|source| {
                                    metadata(&source)
                                        .map_err(|error| ContainerError::new(format!("inspect mount source {} — {error}", source.display())))
                                        .and_then(|metadata| match metadata.is_file() || metadata.is_dir() {
                                            | true => Ok(source),
                                            | false => Err(ContainerError::new(format!(
                                                "mount source {} is not a regular file or directory",
                                                source.display()
                                            ))),
                                        })
                                })
                                .map(|source| {
                                    let mode = if mount.read_only { ",readonly" } else { "" };
                                    args![
                                        ..arguments,
                                        "--mount",
                                        format!("type=bind,source={},target={}{}", source.display(), mount.target, mode)
                                    ]
                                })
                        },
                    )
                    .and_then(|arguments| {
                        let (_, startup_arguments) = self.resolved_startup();
                        self.runtime
                            .run(args![..arguments, image, ..startup_arguments])
                            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                    })
            },
        )
    }
}
impl ContainerError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            module_load: None,
        }
    }
    fn from_module_load(source: ModuleLoadError) -> Self {
        Self {
            message: source.to_string(),
            module_load: Some(source),
        }
    }
    /// Whether a loaded template command has an invalid static binding.
    pub fn is_invalid_configuration(&self) -> bool {
        matches!(
            self.module_load.as_ref(),
            Some(ModuleLoadError::Template {
                source: TemplateError::Interpolation { .. },
                ..
            })
        )
    }
}
impl core::error::Error for ContainerError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.module_load.as_ref().map(|source| source as &(dyn core::error::Error + 'static))
    }
}
impl fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}
impl ContainerLifecycle {
    /// Create managed containers and verify external containers needed by selected modules.
    pub fn start(
        config: &Config,
        selected: &BTreeSet<usize>,
        root: &Path,
        run_id: &str,
        offline: bool,
        cleanup_override: Option<ContainerCleanup>,
    ) -> Result<Self, ContainerError> {
        config.container_declarations(selected, root).and_then(|declarations| {
            declarations.into_values().try_fold(Self::default(), |mut lifecycle, declaration| {
                let result = lifecycle.start_one(declaration, run_id, offline, cleanup_override);
                match result {
                    | Ok(()) => Ok(lifecycle),
                    | Err(primary) => match lifecycle.finish(false) {
                        | Ok(_) => Err(primary),
                        | Err(cleanup) => Err(ContainerError::new(format!("{primary}; additionally, {cleanup}"))),
                    },
                }
            })
        })
    }
    /// Apply cleanup policy and return the runtime names of retained containers.
    pub fn finish(mut self, succeeded: bool) -> Result<ContainerFinish, ContainerError> {
        let (mut result, errors) =
            self.containers
                .drain(..)
                .rev()
                .fold((ContainerFinish::default(), Vec::new()), |(mut result, mut errors), container| {
                    let policy_removes = matches!(container.cleanup, ContainerCleanup::Always)
                        || matches!(container.cleanup, ContainerCleanup::OnSuccess) && succeeded;
                    let remove = container.id.is_some() && policy_removes;
                    match (remove, container.id.as_deref()) {
                        | (true, Some(id)) => {
                            if let Err(error) = container.runtime.remove(id) {
                                errors.push(format!("remove managed container {} — {error}", container.name));
                            } else {
                                result.removed.push(container.name);
                            }
                        }
                        | (false, Some(_)) => result.retained.push(container.name),
                        | (_, None) => result.external.push(container.name),
                    }
                    (result, errors)
                });
        result.external.reverse();
        result.removed.reverse();
        result.retained.reverse();
        match errors.is_empty() {
            | true => Ok(result),
            | false => Err(ContainerError::new(errors.join("; "))),
        }
    }
    fn start_one(
        &mut self,
        declaration: ContainerEnvironment,
        run_id: &str,
        offline: bool,
        cleanup_override: Option<ContainerCleanup>,
    ) -> Result<(), ContainerError> {
        let timeout = Duration::from_secs(declaration.startup_timeout_seconds.unwrap_or(DEFAULT_STARTUP_TIMEOUT_SECONDS));
        match declaration.create {
            | false => match declaration.runtime {
                | ContainerRuntime::Docker | ContainerRuntime::Podman => declaration.runtime.wait_ready(&declaration.name, timeout, None),
                | ContainerRuntime::Apptainer => Ok(()),
                | ContainerRuntime::Unknown => Err(ContainerError::new("unsupported container runtime")),
            }
            .map(|()| {
                self.containers.push(ActiveContainer {
                    cleanup: ContainerCleanup::Never,
                    id: None,
                    name: declaration.name,
                    runtime: declaration.runtime,
                });
            }),
            | true => {
                let name = managed_container_name(&declaration.name, run_id);
                let startup = declaration.resolved_startup().0;
                declaration.create_managed(&name, run_id, offline).and_then(|id| {
                    self.containers.push(ActiveContainer {
                        cleanup: cleanup_override.or(declaration.cleanup).unwrap_or_default(),
                        id: Some(id),
                        name: name.clone(),
                        runtime: declaration.runtime,
                    });
                    declaration.runtime.wait_ready(&name, timeout, Some(startup))
                })
            }
        }
    }
}
impl ContainerRuntime {
    fn inspect(self, name: &str) -> Result<InspectContainer, ContainerError> {
        self.run(args!["inspect", name]).and_then(|output| {
            serde_json::from_slice::<Vec<InspectContainer>>(&output.stdout)
                .map_err(|error| ContainerError::new(format!("parse {self} inspect for {name}: {error}")))
                .and_then(|values| {
                    values
                        .into_iter()
                        .next()
                        .ok_or_else(|| ContainerError::new(format!("{self} inspect returned no container for {name}")))
                })
        })
    }
    fn remove(self, id: &str) -> Result<(), ContainerError> {
        self.run(args!["rm", "--force", id]).map(|_| ())
    }
    fn run(self, arguments: Vec<OsString>) -> Result<Output, ContainerError> {
        let display = arguments.iter().map(|argument| argument.to_string_lossy()).collect::<Vec<_>>().join(" ");
        Command::new(self.to_string())
            .args(arguments)
            .output()
            .map_err(|error| ContainerError::new(format!("run {self} {display} — {error}")))
            .and_then(|output| match output.status.success() {
                | true => Ok(output),
                | false => Err(ContainerError::new(format!(
                    "run {self} {display}: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))),
            })
    }
    fn wait_ready(self, name: &str, timeout: Duration, startup: Option<ContainerStartup>) -> Result<(), ContainerError> {
        let started = Instant::now();
        loop {
            let readiness = self.inspect(name).and_then(|container| match container.state.running {
                | false => Err(ContainerError::new(match startup {
                    | Some(ContainerStartup::KeepAlive) => format!(
                        "container {name} is not running with startup strategy keep_alive; the image may not provide sleep with infinity support"
                    ),
                    | Some(startup) => format!("container {name} is not running with startup strategy {startup}"),
                    | None => format!("container {name} is not running"),
                })),
                | true => match container.state.health.map(|health| health.status) {
                    | None => Ok(true),
                    | Some(status) if status == "healthy" => Ok(true),
                    | Some(status) if status == "unhealthy" => Err(ContainerError::new(format!("container {name} is unhealthy"))),
                    | Some(_) => Ok(false),
                },
            });
            match readiness {
                | Ok(true) => break Ok(()),
                | Err(error) => break Err(error),
                | Ok(false) if started.elapsed() >= timeout => {
                    break Err(ContainerError::new(format!(
                        "container {name} did not become healthy within {}s",
                        timeout.as_secs()
                    )))
                }
                | Ok(false) => sleep(Duration::from_millis(250)),
            }
        }
    }
}
