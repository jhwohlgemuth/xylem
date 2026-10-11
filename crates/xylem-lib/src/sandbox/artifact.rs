//! Private staging and atomic publication for sandbox artifacts.
extern crate alloc;

use super::{
    archive, validate_granted_destination, ArtifactPublication, ArtifactPublicationStatus, SandboxError, SandboxResources, SandboxSessionOutcome,
};
use crate::completion::CompletionPlan;
use crate::config::Config;
use acorn_schema::hardware::Memory;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use glob::glob;
use std::fs::{copy, create_dir, create_dir_all, read_dir, remove_dir_all, rename, symlink_metadata, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sysinfo::Disks;

#[derive(Clone, Debug)]
pub(super) struct ArtifactInput {
    pub archive: PathBuf,
    pub index: usize,
    pub raw: PathBuf,
    pub validated: PathBuf,
}
#[derive(Debug)]
pub(super) struct ArtifactManager {
    artifact_roots: Vec<PathBuf>,
    resources: SandboxResources,
    run_id: String,
    state: Mutex<Option<ArtifactRun>>,
}
#[derive(Clone, Debug)]
struct ArtifactRecord {
    config: Config,
    input: Option<String>,
    staged: Option<(u64, u64)>,
}
#[derive(Debug)]
struct ArtifactRun {
    destination: PathBuf,
    inputs: BTreeMap<usize, ArtifactRecord>,
    input_count: usize,
    staging: PathBuf,
}
impl ArtifactManager {
    pub(super) fn new(run_id: String, artifact_roots: Vec<PathBuf>, resources: SandboxResources) -> Arc<Self> {
        Arc::new(Self {
            artifact_roots,
            resources,
            run_id,
            state: Mutex::new(None),
        })
    }
    pub(super) fn finish(&self, succeeded: bool) -> Result<SandboxSessionOutcome, SandboxError> {
        self.state
            .lock()
            .map_err(|_| SandboxError::Runtime("sandbox artifact state lock was poisoned".to_string()))
            .and_then(|mut state| match state.take() {
                | None => Ok(None),
                | Some(run) if !succeeded => cleanup(&run.staging).map(|()| {
                    Some(ArtifactPublication {
                        bytes: Memory::kb(0.0),
                        bytes_per_input: Memory::kb(self.resources.artifact_bytes as f64 / 1024.0),
                        destination: run.destination.display().to_string(),
                        entries: 0,
                        entries_per_input: self.resources.artifact_files,
                        error: None,
                        status: ArtifactPublicationStatus::Discarded,
                    })
                }),
                | Some(run) => self.publish(run).map(Some),
            })
    }
    pub(super) fn prepare(
        &self,
        config: &Config,
        input: Option<&str>,
        input_index: usize,
        input_count: usize,
    ) -> Result<Option<ArtifactInput>, SandboxError> {
        config.output.as_ref().map_or(Ok(None), |output| {
            self.state
                .lock()
                .map_err(|_| SandboxError::Runtime("sandbox artifact state lock was poisoned".to_string()))
                .and_then(|mut state| {
                    match state.is_none() {
                        | true => self.initialize(output, input_count).map(|run| *state = Some(run)),
                        | false => Ok(()),
                    }
                    .and_then(|()| {
                        state
                            .as_mut()
                            .ok_or_else(|| SandboxError::Runtime("sandbox artifact state was not initialized".to_string()))
                    })
                    .and_then(|run| {
                        validate_granted_destination(Path::new(output), &self.artifact_roots).and_then(|destination| {
                            match (
                                run.destination == destination,
                                run.input_count == input_count,
                                run.inputs.contains_key(&input_index),
                            ) {
                                | (false, _, _) => Err(SandboxError::Validation(
                                    "sandbox inputs resolved different artifact destinations".to_string(),
                                )),
                                | (_, false, _) => Err(SandboxError::Validation("sandbox inputs declared inconsistent input counts".to_string())),
                                | (_, _, true) => Err(SandboxError::Validation(format!(
                                    "sandbox artifact input {input_index} was registered more than once"
                                ))),
                                | (true, true, false) => {
                                    let archive = run.staging.join("archives").join(format!("input-{input_index:06}.tar"));
                                    let raw = run.staging.join("raw").join(format!("input-{input_index:06}"));
                                    let validated = run.staging.join("validated").join(format!("input-{input_index:06}"));
                                    create_dir_all(&raw)
                                        .map_err(|why| SandboxError::Runtime(format!("create private artifact directory {}: {why}", raw.display())))
                                        .and_then(|()| make_writable(&raw))
                                        .and_then(|()| {
                                            archive::create_export(&archive).map_err(|why| {
                                                SandboxError::Runtime(format!("create private artifact archive {}: {why}", archive.display()))
                                            })
                                        })
                                        .map(|()| {
                                            run.inputs.insert(
                                                input_index,
                                                ArtifactRecord {
                                                    config: config.clone(),
                                                    input: input.map(str::to_string),
                                                    staged: None,
                                                },
                                            );
                                            Some(ArtifactInput {
                                                archive,
                                                index: input_index,
                                                raw,
                                                validated,
                                            })
                                        })
                                }
                            }
                        })
                    })
                })
        })
    }
    pub(super) fn stage(&self, input: &ArtifactInput) -> Result<(), SandboxError> {
        archive::extract(
            &input.archive,
            &input.validated,
            self.resources.artifact_bytes,
            self.resources.artifact_files,
        )
        .map_err(|why| SandboxError::Runtime(format!("validate sandbox artifact archive: {why}")))
        .and_then(|()| inspect_tree(&input.validated, self.resources.artifact_bytes, self.resources.artifact_files))
        .and_then(|totals| {
            self.state
                .lock()
                .map_err(|_| SandboxError::Runtime("sandbox artifact state lock was poisoned".to_string()))
                .and_then(|mut state| {
                    state
                        .as_mut()
                        .and_then(|run| run.inputs.get_mut(&input.index))
                        .ok_or_else(|| SandboxError::Runtime(format!("sandbox artifact input {} is not registered", input.index)))
                        .map(|record| record.staged = Some(totals))
                })
        })
    }
    fn initialize(&self, output: &str, input_count: usize) -> Result<ArtifactRun, SandboxError> {
        validate_granted_destination(Path::new(output), &self.artifact_roots).and_then(|destination| {
            destination
                .parent()
                .ok_or_else(|| SandboxError::Validation("sandbox artifact destination must have a parent".to_string()))
                .and_then(|parent| {
                    self.resources
                        .artifact_files
                        .checked_mul(4096)
                        .and_then(|overhead| self.resources.artifact_bytes.checked_add(overhead))
                        .and_then(|per_input| per_input.checked_mul(input_count as u64))
                        .and_then(|all_inputs| all_inputs.checked_mul(2))
                        .ok_or_else(|| SandboxError::Validation("sandbox artifact capacity requirement overflowed".to_string()))
                        .and_then(|required| ensure_capacity(parent, required))
                        .map(|()| parent.join(format!(".xylem-{}-artifacts", self.run_id)))
                })
                .and_then(|staging| {
                    create_dir(&staging)
                        .map_err(|why| SandboxError::Runtime(format!("create private artifact staging {}: {why}", staging.display())))
                        .and_then(|()| make_private(&staging))
                        .and_then(|()| {
                            ["archives", "raw", "validated"]
                                .iter()
                                .try_for_each(|name| create_dir(staging.join(name)).map_err(|why| SandboxError::Runtime(why.to_string())))
                        })
                        .inspect_err(|_| {
                            let _ = cleanup(&staging);
                        })
                        .map(|()| ArtifactRun {
                            destination,
                            inputs: BTreeMap::new(),
                            input_count,
                            staging,
                        })
                })
        })
    }
    fn publish(&self, run: ArtifactRun) -> Result<ArtifactPublication, SandboxError> {
        let result = match (
            run.inputs.len() == run.input_count,
            run.inputs.values().all(|record| record.staged.is_some()),
        ) {
            | (false, _) => Err(SandboxError::Runtime(format!(
                "sandbox artifact session registered {} of {} inputs",
                run.inputs.len(),
                run.input_count
            ))),
            | (true, false) => Err(SandboxError::Runtime("sandbox artifact session contains an unstaged input".to_string())),
            | (true, true) => {
                let merged = run.staging.join("merged");
                create_dir(&merged)
                    .map_err(|why| SandboxError::Runtime(format!("create merged artifact staging {}: {why}", merged.display())))
                    .and_then(|()| {
                        run.inputs
                            .keys()
                            .try_for_each(|index| merge_tree(&run.staging.join("validated").join(format!("input-{index:06}")), &merged))
                    })
                    .and_then(|()| run.synthesize_markers(&merged))
                    .and_then(|()| inspect_tree(&merged, u64::MAX, u64::MAX))
                    .and_then(|(bytes, entries)| {
                        cleanup_inputs(&run.staging).and_then(|()| {
                            rename(&merged, &run.destination)
                                .map_err(|why| SandboxError::Runtime(format!("publish sandbox artifacts to {}: {why}", run.destination.display())))
                                .map(|()| ArtifactPublication {
                                    bytes: Memory::kb(bytes as f64 / 1024.0),
                                    bytes_per_input: Memory::kb(self.resources.artifact_bytes as f64 / 1024.0),
                                    destination: run.destination.display().to_string(),
                                    entries,
                                    entries_per_input: self.resources.artifact_files,
                                    error: None,
                                    status: ArtifactPublicationStatus::Published,
                                })
                        })
                    })
            }
        };
        let cleaned = cleanup(&run.staging);
        match (result, cleaned) {
            | (Ok(outcome), _) => Ok(outcome),
            | (Err(primary), _) => Err(primary),
        }
    }
}
impl ArtifactRun {
    fn synthesize_markers(&self, merged: &Path) -> Result<(), SandboxError> {
        self.inputs.iter().try_for_each(|(_, record)| {
            record.config.modules.iter().try_for_each(|module| {
                let variables = record
                    .config
                    .clone()
                    .get_environment_variables(Some(module.clone()), record.input.clone());
                CompletionPlan::resolve(module, &variables, record.config.output.as_deref())
                    .map_err(|why| SandboxError::Validation(why.to_string()))
                    .and_then(|completion| {
                        completion.map_or(Ok(()), |completion| {
                            validate_completion_outputs(&completion.outputs, &self.destination, merged).and_then(|()| {
                                completion
                                    .marker
                                    .strip_prefix(&self.destination)
                                    .map_err(|_| {
                                        SandboxError::Validation(format!(
                                            "completion marker {} is outside artifact destination {}",
                                            completion.marker.display(),
                                            self.destination.display()
                                        ))
                                    })
                                    .and_then(|relative| {
                                        let marker = merged.join(relative);
                                        marker
                                            .parent()
                                            .map_or(Ok(()), |parent| {
                                                create_dir_all(parent)
                                                    .map_err(|why| SandboxError::Runtime(format!("create completion marker directory: {why}")))
                                            })
                                            .and_then(|()| match std::fs::read_to_string(&marker) {
                                                | Ok(existing) if existing == format!("{}\n", completion.identity) => Ok(()),
                                                | Ok(_) => {
                                                    Err(SandboxError::Validation(format!("completion marker conflict at {}", relative.display())))
                                                }
                                                | Err(why) if why.kind() == std::io::ErrorKind::NotFound => OpenOptions::new()
                                                    .write(true)
                                                    .create_new(true)
                                                    .open(&marker)
                                                    .map_err(|why| {
                                                        SandboxError::Runtime(format!("create completion marker {}: {why}", marker.display()))
                                                    })
                                                    .and_then(|mut file| {
                                                        file.write_all(format!("{}\n", completion.identity).as_bytes())
                                                            .and_then(|()| file.sync_all())
                                                            .map_err(|why| {
                                                                SandboxError::Runtime(format!("write completion marker {}: {why}", marker.display()))
                                                            })
                                                    }),
                                                | Err(why) => {
                                                    Err(SandboxError::Runtime(format!("read completion marker {}: {why}", marker.display())))
                                                }
                                            })
                                    })
                            })
                        })
                    })
            })
        })
    }
}
fn cleanup(path: &Path) -> Result<(), SandboxError> {
    match remove_dir_all(path) {
        | Ok(()) => Ok(()),
        | Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
        | Err(why) => Err(SandboxError::Runtime(format!(
            "remove private artifact staging {}: {why}",
            path.display()
        ))),
    }
}
fn cleanup_inputs(staging: &Path) -> Result<(), SandboxError> {
    ["archives", "raw", "validated"].iter().try_for_each(|name| cleanup(&staging.join(name)))
}
fn ensure_capacity(path: &Path, required: u64) -> Result<(), SandboxError> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())
        .ok_or_else(|| SandboxError::Runtime(format!("determine available space for {}", path.display())))
        .and_then(|disk| match disk.available_space() >= required {
            | true => Ok(()),
            | false => Err(SandboxError::Validation(format!(
                "sandbox artifact staging requires {required} available bytes under {}, found {}",
                path.display(),
                disk.available_space()
            ))),
        })
}
fn inspect_tree(root: &Path, maximum_bytes: u64, maximum_entries: u64) -> Result<(u64, u64), SandboxError> {
    walk(root).and_then(|paths| {
        paths.into_iter().try_fold((0u64, 0u64), |(bytes, entries), path| {
            symlink_metadata(&path)
                .map_err(|why| SandboxError::Runtime(format!("inspect artifact path {}: {why}", path.display())))
                .and_then(|metadata| {
                    if metadata.file_type().is_symlink() || !(metadata.is_dir() || metadata.is_file()) {
                        Err(SandboxError::Validation(format!(
                            "artifact path {} has an unsupported type",
                            path.display()
                        )))
                    } else {
                        entries
                            .checked_add(1)
                            .filter(|value| *value <= maximum_entries)
                            .ok_or_else(|| SandboxError::Validation(format!("sandbox artifacts exceed the {maximum_entries}-entry limit")))
                            .and_then(|entries| {
                                let file_bytes = match metadata.is_file() {
                                    | true => metadata.len(),
                                    | false => 0,
                                };
                                bytes
                                    .checked_add(file_bytes)
                                    .filter(|value| *value <= maximum_bytes)
                                    .ok_or_else(|| SandboxError::Validation(format!("sandbox artifacts exceed the {maximum_bytes}-byte limit")))
                                    .map(|bytes| (bytes, entries))
                            })
                    }
                })
        })
    })
}
fn make_private(path: &Path) -> Result<(), SandboxError> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|why| SandboxError::Runtime(format!("make artifact staging private {}: {why}", path.display())))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
fn make_writable(path: &Path) -> Result<(), SandboxError> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o777))
            .map_err(|why| SandboxError::Runtime(format!("make private artifact directory writable {}: {why}", path.display())))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
fn merge_tree(source: &Path, destination: &Path) -> Result<(), SandboxError> {
    walk(source).and_then(|paths| {
        paths.into_iter().try_for_each(|path| {
            path.strip_prefix(source)
                .map_err(|why| SandboxError::Runtime(format!("resolve artifact path {}: {why}", path.display())))
                .and_then(|relative| {
                    let output = destination.join(relative);
                    symlink_metadata(&path)
                        .map_err(|why| SandboxError::Runtime(format!("inspect artifact path {}: {why}", path.display())))
                        .and_then(|metadata| match (metadata.is_dir(), output.exists()) {
                            | (true, false) => create_dir(&output)
                                .map_err(|why| SandboxError::Runtime(format!("create merged artifact directory {}: {why}", output.display()))),
                            | (true, true) if output.is_dir() => Ok(()),
                            | (true, true) => Err(SandboxError::Validation(format!("artifact type conflict at {}", relative.display()))),
                            | (false, false) => copy(&path, &output)
                                .map(|_| ())
                                .map_err(|why| SandboxError::Runtime(format!("merge artifact file {}: {why}", relative.display()))),
                            | (false, true) => Err(SandboxError::Validation(format!("artifact file conflict at {}", relative.display()))),
                        })
                })
        })
    })
}
fn validate_completion_outputs(outputs: &[PathBuf], destination: &Path, merged: &Path) -> Result<(), SandboxError> {
    outputs.iter().try_for_each(|output| {
        output
            .strip_prefix(destination)
            .map_err(|_| {
                SandboxError::Validation(format!(
                    "completion output {} is outside artifact destination {}",
                    output.display(),
                    destination.display()
                ))
            })
            .map(|relative| merged.join(relative))
            .and_then(|pattern| {
                pattern
                    .to_str()
                    .ok_or_else(|| SandboxError::Validation(format!("completion output pattern is not UTF-8: {}", pattern.display())))
                    .and_then(|pattern| {
                        glob(pattern)
                            .map_err(|why| SandboxError::Validation(format!("invalid completion output glob {pattern}: {why}")))
                            .and_then(|entries| {
                                entries.into_iter().try_fold(false, |matched, entry| {
                                    entry
                                        .map_err(|why| SandboxError::Validation(format!("inspect completion output glob {pattern}: {why}")))
                                        .and_then(|path| {
                                            symlink_metadata(&path)
                                                .map_err(|why| SandboxError::Runtime(format!("inspect completion output {}: {why}", path.display())))
                                                .and_then(|metadata| match metadata.is_file() && metadata.len() > 0 {
                                                    | true => Ok(true),
                                                    | false => Err(SandboxError::Validation(format!(
                                                        "completion output {} is not a non-empty regular file",
                                                        path.display()
                                                    ))),
                                                })
                                        })
                                        .map(|current| matched || current)
                                })
                            })
                            .and_then(|matched| match matched {
                                | true => Ok(()),
                                | false => Err(SandboxError::Validation(format!("completion output pattern {pattern} matched no files"))),
                            })
                    })
            })
    })
}
fn walk(root: &Path) -> Result<Vec<PathBuf>, SandboxError> {
    read_dir(root)
        .map_err(|why| SandboxError::Runtime(format!("read artifact directory {}: {why}", root.display())))
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()).map_err(|why| SandboxError::Runtime(why.to_string())))
                .collect::<Result<Vec<_>, _>>()
        })
        .and_then(|mut paths| {
            paths.sort();
            paths.into_iter().try_fold(Vec::new(), |mut all, path| {
                match symlink_metadata(&path) {
                    | Ok(metadata) if metadata.is_dir() => walk(&path),
                    | Ok(_) => Ok(Vec::new()),
                    | Err(why) => Err(SandboxError::Runtime(format!("inspect artifact path {}: {why}", path.display()))),
                }
                .map(|children| {
                    all.push(path);
                    all.extend(children);
                    all
                })
            })
        })
}
