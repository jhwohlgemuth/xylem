//! Owns disposable filesystem state for workflow runs and their inputs.
//!
//! A [`RunWorkspace`] creates a private run directory with separate configuration
//! and input trees. Each [`InputWorkspace`] then isolates module sources,
//! environment prefixes, and scratch data for one input. Dropping the run owner
//! removes the complete tree unless [`RunWorkspace::retain`] transfers cleanup
//! responsibility to the caller.
//!
//! ```no_run
//! use std::env;
//! use xylem_lib::workspace::RunWorkspace;
//!
//! let parent = env::temp_dir().join("xylem-workspaces");
//! let workspace = RunWorkspace::create(&parent, "example-run").expect("create workspace");
//! let input = workspace.input(0).expect("create input workspace");
//!
//! assert!(workspace.configuration().is_dir());
//! assert!(input.modules().is_dir());
//! workspace.close().expect("remove workspace");
//! ```
extern crate alloc;

use crate::util::constants::app::MODULE_SOURCE_EXCLUSIONS;
use acorn_host::fs::{FsError, SafePath};
use alloc::collections::BTreeSet;
use core::fmt;
use core::iter::once;
use std::fs::{canonicalize, copy, create_dir, create_dir_all, read_dir, remove_dir_all, remove_file, set_permissions, symlink_metadata};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Failure while creating, materializing, or cleaning a workflow workspace.
#[derive(Debug)]
pub enum WorkspaceError {
    /// A filesystem operation failed.
    Io {
        /// Failed operation.
        action: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// A materialized source violates the workspace boundary.
    Materialization {
        /// Rejected source entry.
        path: PathBuf,
        /// Reason the entry was rejected.
        reason: String,
    },
    /// A relative workspace path was unsafe or crossed an unsafe component.
    Path {
        /// Rejected relative path.
        path: PathBuf,
        /// Path validation or resolution failure.
        source: FsError,
    },
    /// A run identifier is not a single safe path component.
    RunId(String),
}
/// Paths owned by one workflow input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputWorkspace {
    environments: PathBuf,
    modules: PathBuf,
    root: PathBuf,
    scratch: PathBuf,
}
/// RAII owner for one workflow run's disposable files.
#[derive(Debug)]
pub struct RunWorkspace {
    cleanup: bool,
    configuration: PathBuf,
    inputs: PathBuf,
    root: PathBuf,
}
impl InputWorkspace {
    /// Return this input's environment-prefix directory.
    pub fn environments(&self) -> &Path {
        &self.environments
    }
    /// Return the isolated materialization path for a module index.
    pub fn module(&self, module_index: usize) -> Result<PathBuf, WorkspaceError> {
        workspace_path(&self.modules, format!("module-{module_index:06}"))
    }
    /// Return this input's module directory.
    pub fn modules(&self) -> &Path {
        &self.modules
    }
    /// Return this input's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Return this input's scratch directory.
    pub fn scratch(&self) -> &Path {
        &self.scratch
    }
}
impl RunWorkspace {
    /// Create a workspace under an injected parent directory.
    pub fn create(parent: &Path, run_id: &str) -> Result<Self, WorkspaceError> {
        SafePath::new(run_id)
            .map_err(|_| WorkspaceError::RunId(run_id.to_string()))
            .and_then(|run_id| match run_id.as_path().components().count() == 1 {
                | true => Ok(run_id),
                | false => Err(WorkspaceError::RunId(run_id.as_path().display().to_string())),
            })
            .and_then(|run_id| {
                create_dir_all(parent)
                    .map_err(|source| io_error("create workspace parent", parent, source))
                    .and_then(|()| {
                        run_id.materialize_under(parent).map_err(|source| WorkspaceError::Path {
                            path: run_id.as_path().to_path_buf(),
                            source,
                        })
                    })
            })
            .and_then(|root| {
                create_dir(&root)
                    .map_err(|source| io_error("create run workspace", &root, source))
                    .and_then(|()| {
                        #[cfg(unix)]
                        let private = set_permissions(&root, std::fs::Permissions::from_mode(0o700))
                            .map_err(|source| io_error("make run workspace private", &root, source));
                        #[cfg(not(unix))]
                        let private = Ok(());
                        private
                            .inspect_err(|_| {
                                let _ = remove_dir_all(&root);
                            })
                            .map(|()| root)
                    })
                    .and_then(|root| {
                        workspace_path(&root, "configuration")
                            .and_then(|configuration| workspace_path(&root, "inputs").map(|inputs| (configuration, inputs)))
                            .and_then(|(configuration, inputs)| {
                                [configuration.clone(), inputs.clone()]
                                    .into_iter()
                                    .try_for_each(|path| create_dir(&path).map_err(|source| io_error("create workspace directory", &path, source)))
                                    .map(|()| (configuration, inputs))
                            })
                            .inspect_err(|_| {
                                let _ = remove_dir_all(&root);
                            })
                            .map(|(configuration, inputs)| Self {
                                cleanup: true,
                                configuration,
                                inputs,
                                root,
                            })
                    })
            })
    }
    /// Remove this workspace immediately.
    pub fn close(mut self) -> Result<(), WorkspaceError> {
        remove_dir_all(&self.root)
            .map_err(|source| io_error("remove run workspace", &self.root, source))
            .map(|()| {
                self.cleanup = false;
            })
    }
    /// Return the run-scoped configuration source directory.
    pub fn configuration(&self) -> &Path {
        &self.configuration
    }
    /// Create one input workspace.
    pub fn input(&self, index: usize) -> Result<InputWorkspace, WorkspaceError> {
        workspace_path(&self.inputs, format!("input-{index:06}"))
            .and_then(|root| {
                create_dir(&root)
                    .map_err(|source| io_error("create input workspace", &root, source))
                    .map(|()| root)
            })
            .and_then(|root| {
                workspace_path(&root, "environments")
                    .and_then(|environments| {
                        workspace_path(&root, "modules")
                            .and_then(|modules| workspace_path(&root, "scratch").map(|scratch| (environments, modules, scratch)))
                    })
                    .and_then(|(environments, modules, scratch)| {
                        [environments.clone(), modules.clone(), scratch.clone()]
                            .into_iter()
                            .try_for_each(|path| create_dir(&path).map_err(|source| io_error("create input workspace directory", &path, source)))
                            .map(|()| (environments, modules, scratch))
                    })
                    .inspect_err(|_| {
                        let _ = remove_dir_all(&root);
                    })
                    .map(|(environments, modules, scratch)| InputWorkspace {
                        environments,
                        modules,
                        root,
                        scratch,
                    })
            })
    }
    /// Keep this workspace and transfer cleanup responsibility to the caller.
    pub fn retain(mut self) -> PathBuf {
        self.cleanup = false;
        self.root.clone()
    }
    /// Return this workspace's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
impl Drop for RunWorkspace {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = remove_dir_all(&self.root);
        }
    }
}
impl core::error::Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Io { source, .. } => Some(source),
            | Self::Path { source, .. } => Some(source),
            | Self::Materialization { .. } | Self::RunId(_) => None,
        }
    }
}
impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Io { action, path, source } => write!(formatter, "{action} {}: {source}", path.display()),
            | Self::Materialization { path, reason } => write!(formatter, "materialize workspace source {}: {reason}", path.display()),
            | Self::Path { path, source } => write!(formatter, "resolve safe workspace path {}: {source}", path.display()),
            | Self::RunId(run_id) => write!(formatter, "workspace run ID must be one safe path component: {run_id}"),
        }
    }
}
fn copy_entry(
    source_root: &Path,
    source: &Path,
    destination_root: &Path,
    destination: &SafePath,
    ancestors: &BTreeSet<PathBuf>,
) -> Result<(), WorkspaceError> {
    destination
        .materialize_under(destination_root)
        .map_err(|source| WorkspaceError::Path {
            path: destination.as_path().to_path_buf(),
            source,
        })
        .and_then(|destination_path| {
            symlink_metadata(source)
                .map_err(|source_error| io_error("inspect materialization source", source, source_error))
                .and_then(|metadata| match metadata.file_type().is_symlink() {
                    | true => canonicalize(source)
                        .map_err(|source_error| io_error("resolve materialization symlink", source, source_error))
                        .and_then(|target| match target.starts_with(source_root) {
                            | true => copy_entry(source_root, &target, destination_root, destination, ancestors),
                            | false => Err(WorkspaceError::Materialization {
                                path: source.to_path_buf(),
                                reason: format!("symlink target {} escapes source root {}", target.display(), source_root.display()),
                            }),
                        }),
                    | false if metadata.is_dir() => canonicalize(source)
                        .map_err(|source_error| io_error("canonicalize materialization directory", source, source_error))
                        .and_then(|canonical| match ancestors.contains(&canonical) {
                            | true => Err(WorkspaceError::Materialization {
                                path: source.to_path_buf(),
                                reason: "directory cycle detected".to_string(),
                            }),
                            | false => {
                                let next = ancestors.iter().cloned().chain(once(canonical)).collect::<BTreeSet<_>>();
                                create_dir(&destination_path)
                                    .map_err(|source_error| io_error("create materialized directory", &destination_path, source_error))
                                    .and_then(|()| {
                                        read_dir(source)
                                            .map_err(|source_error| io_error("read materialization directory", source, source_error))
                                            .and_then(|entries| {
                                                entries
                                                    .map(|entry| {
                                                        entry.map_err(|source_error| io_error("read materialization entry", source, source_error))
                                                    })
                                                    .collect::<Result<Vec<_>, _>>()
                                            })
                                            .map(|mut entries| {
                                                entries.sort_by_key(std::fs::DirEntry::file_name);
                                                entries
                                            })
                                            .and_then(|entries| {
                                                entries
                                                    .into_iter()
                                                    .filter(|entry| {
                                                        let path = entry.path();
                                                        !MODULE_SOURCE_EXCLUSIONS.iter().any(|excluded| path.ends_with(excluded))
                                                    })
                                                    .try_for_each(|entry| {
                                                        let relative = destination.as_path().join(entry.file_name());
                                                        SafePath::new(&relative)
                                                            .map_err(|source| WorkspaceError::Path { path: relative, source })
                                                            .and_then(|child| copy_entry(source_root, &entry.path(), destination_root, &child, &next))
                                                    })
                                            })
                                    })
                            }
                        }),
                    | false if metadata.is_file() => destination_path
                        .parent()
                        .map_or(Ok(()), |parent| {
                            create_dir_all(parent).map_err(|source_error| io_error("create materialized parent", parent, source_error))
                        })
                        .and_then(|()| {
                            copy(source, &destination_path)
                                .map_err(|source_error| io_error("copy materialized file", &destination_path, source_error))
                        })
                        .and_then(|_| {
                            set_permissions(&destination_path, metadata.permissions())
                                .map_err(|source_error| io_error("preserve materialized permissions", &destination_path, source_error))
                        }),
                    | false => Err(WorkspaceError::Materialization {
                        path: source.to_path_buf(),
                        reason: "special files are not supported".to_string(),
                    }),
                })
        })
}
fn io_error(action: &'static str, path: &Path, source: io::Error) -> WorkspaceError {
    WorkspaceError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}
/// Copy a local file or directory beneath an owned workspace root.
/// ### Note
/// Directory copies omit installed `.pixi/envs` entries, including in nested projects, before inspecting their contents or resolving their symlinks.
/// Other source files, including Pixi manifests, lockfiles, configuration, and Git-ignored data, are copied.
pub fn materialize(source: &Path, destination_root: &Path, destination: &SafePath) -> Result<PathBuf, WorkspaceError> {
    canonicalize(source)
        .map_err(|source_error| io_error("canonicalize materialization source", source, source_error))
        .and_then(|source| {
            destination
                .materialize_under(destination_root)
                .map_err(|source| WorkspaceError::Path {
                    path: destination.as_path().to_path_buf(),
                    source,
                })
                .map(|destination_path| (source, destination_path))
        })
        .and_then(|(source, destination_path)| match destination_path.try_exists() {
            | Ok(true) => Err(WorkspaceError::Materialization {
                path: destination_path,
                reason: "destination already exists".to_string(),
            }),
            | Ok(false) => copy_entry(&source, &source, destination_root, destination, &BTreeSet::new())
                .inspect_err(|_| match symlink_metadata(&destination_path) {
                    | Ok(metadata) if metadata.is_dir() => {
                        let _ = remove_dir_all(&destination_path);
                    }
                    | Ok(_) => {
                        let _ = remove_file(&destination_path);
                    }
                    | Err(_) => (),
                })
                .map(|()| destination_path),
            | Err(source_error) => Err(io_error("inspect materialization destination", &destination_path, source_error)),
        })
}
fn workspace_path(root: &Path, relative: impl AsRef<Path>) -> Result<PathBuf, WorkspaceError> {
    let relative = relative.as_ref().to_path_buf();
    SafePath::new(&relative)
        .map_err(|source| WorkspaceError::Path {
            path: relative.clone(),
            source,
        })
        .and_then(|safe| {
            safe.materialize_under(root)
                .map_err(|source| WorkspaceError::Path { path: relative, source })
        })
}

#[cfg(test)]
mod tests;
