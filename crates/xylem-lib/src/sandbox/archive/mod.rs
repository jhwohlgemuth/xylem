//! Validation and extraction for bounded archives exported from sandbox preparation.
extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use std::fs::{create_dir_all, metadata, File, Metadata};
#[cfg(unix)]
use std::fs::{set_permissions, Permissions};
use std::io::{self, copy};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use tar::{Archive, EntryType};

#[derive(Debug)]
pub(super) enum PreparedArchiveError {
    Io(io::Error),
    Invalid(String),
}
#[derive(Clone, Debug)]
struct Inspection {
    links: Vec<Link>,
    regular_bytes: u64,
}
#[derive(Clone, Debug)]
struct Link {
    path: PathBuf,
    target: PathBuf,
}
impl core::error::Error for PreparedArchiveError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Io(error) => Some(error),
            | Self::Invalid(_) => None,
        }
    }
}
impl fmt::Display for PreparedArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Io(error) => error.fmt(formatter),
            | Self::Invalid(message) => formatter.write_str(message),
        }
    }
}
impl From<io::Error> for PreparedArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
pub(super) fn create_export(path: &Path) -> Result<(), PreparedArchiveError> {
    File::create(path)
        .map_err(PreparedArchiveError::from)
        .and_then(|_| set_export_permissions(path))
}
pub(super) fn extract(archive_path: &Path, destination: &Path, maximum_bytes: u64, maximum_entries: u64) -> Result<(), PreparedArchiveError> {
    validate_archive_size(archive_path, maximum_bytes, maximum_entries)
        .and_then(|()| inspect(archive_path, maximum_bytes, maximum_entries))
        .and_then(|inspection| {
            create_dir_all(destination)
                .map_err(PreparedArchiveError::from)
                .and_then(|()| extract_regular_entries(archive_path, destination))
                .and_then(|()| materialize_links(destination, &inspection.links, inspection.regular_bytes, maximum_bytes))
        })
}
fn extract_regular_entries(archive_path: &Path, destination: &Path) -> Result<(), PreparedArchiveError> {
    File::open(archive_path).map_err(PreparedArchiveError::from).and_then(|file| {
        let mut archive = Archive::new(file);
        archive.entries().map_err(PreparedArchiveError::from).and_then(|entries| {
            entries.into_iter().try_for_each(|entry| {
                entry.map_err(PreparedArchiveError::from).and_then(|mut entry| {
                    entry
                        .path()
                        .map_err(PreparedArchiveError::from)
                        .and_then(|path| normalize_entry(&path, entry.header().entry_type()))
                        .and_then(|path| match path {
                            | None => Ok(()),
                            | Some(path) => {
                                let output = destination.join(path);
                                match entry.header().entry_type() {
                                    | kind if kind.is_dir() => create_dir_all(&output).map_err(PreparedArchiveError::from),
                                    | kind if kind.is_file() => output
                                        .parent()
                                        .map_or(Ok(()), |parent| create_dir_all(parent).map_err(PreparedArchiveError::from))
                                        .and_then(|()| File::create(&output).map_err(PreparedArchiveError::from))
                                        .and_then(|mut file| copy(&mut entry, &mut file).map(|_| ()).map_err(PreparedArchiveError::from))
                                        .and_then(|()| {
                                            entry
                                                .header()
                                                .mode()
                                                .map_err(PreparedArchiveError::from)
                                                .and_then(|mode| set_mode(output, mode))
                                        }),
                                    | kind if kind.is_symlink() || kind.is_hard_link() => Ok(()),
                                    | _ => Err(PreparedArchiveError::Invalid(
                                        "prepared archive contains an unsupported entry type".to_string(),
                                    )),
                                }
                            }
                        })
                })
            })
        })
    })
}
fn inspect(archive_path: &Path, maximum_bytes: u64, maximum_entries: u64) -> Result<Inspection, PreparedArchiveError> {
    File::open(archive_path).map_err(PreparedArchiveError::from).and_then(|file| {
        let mut archive = Archive::new(file);
        archive.entries().map_err(PreparedArchiveError::from).and_then(|entries| {
            entries
                .into_iter()
                .try_fold(
                    (BTreeSet::new(), Vec::new(), 0u64, 0u64),
                    |(mut paths, mut links, bytes, count), entry| {
                        entry.map_err(PreparedArchiveError::from).and_then(|entry| {
                            entry
                                .path()
                                .map_err(PreparedArchiveError::from)
                                .and_then(|path| normalize_entry(&path, entry.header().entry_type()))
                                .and_then(|path| match path {
                                    | None => Ok((paths, links, bytes, count)),
                                    | Some(path) => {
                                        let kind = entry.header().entry_type();
                                        count
                                            .checked_add(1)
                                            .ok_or_else(|| PreparedArchiveError::Invalid("prepared archive entry count overflowed".to_string()))
                                            .and_then(|next_count| {
                                                match kind.is_file() {
                                                    | true => bytes.checked_add(entry.size()),
                                                    | false => Some(bytes),
                                                }
                                                .ok_or_else(|| PreparedArchiveError::Invalid("prepared archive size overflowed".to_string()))
                                                .map(|next_bytes| (next_count, next_bytes))
                                            })
                                            .and_then(|(next_count, next_bytes)| {
                                                if next_count > maximum_entries {
                                                    Err(PreparedArchiveError::Invalid(format!(
                                                        "prepared archive exceeds the {maximum_entries}-entry limit"
                                                    )))
                                                } else if next_bytes > maximum_bytes {
                                                    Err(PreparedArchiveError::Invalid(format!(
                                                        "prepared archive exceeds the {maximum_bytes}-byte limit"
                                                    )))
                                                } else if !paths.insert(path.clone()) {
                                                    Err(PreparedArchiveError::Invalid(format!(
                                                        "prepared archive contains duplicate path {}",
                                                        path.display()
                                                    )))
                                                } else if kind.is_dir() || kind.is_file() {
                                                    Ok((paths, links, next_bytes, next_count))
                                                } else if kind.is_symlink() || kind.is_hard_link() {
                                                    entry
                                                        .link_name()
                                                        .map_err(PreparedArchiveError::from)
                                                        .and_then(|target| {
                                                            target.ok_or_else(|| {
                                                                PreparedArchiveError::Invalid(format!(
                                                                    "prepared archive link {} has no target",
                                                                    path.display()
                                                                ))
                                                            })
                                                        })
                                                        .and_then(|target| link_target(&path, &target, kind))
                                                        .map(|target| {
                                                            links.push(Link { path, target });
                                                            (paths, links, next_bytes, next_count)
                                                        })
                                                } else {
                                                    Err(PreparedArchiveError::Invalid(format!(
                                                        "prepared archive path {} has unsupported entry type",
                                                        path.display()
                                                    )))
                                                }
                                            })
                                    }
                                })
                        })
                    },
                )
                .and_then(|(paths, links, regular_bytes, _)| resolve_links(&paths, &links).map(|links| Inspection { links, regular_bytes }))
        })
    })
}
fn link_target(path: &Path, target: &Path, kind: EntryType) -> Result<PathBuf, PreparedArchiveError> {
    let target = match kind.is_symlink() {
        | true => path.parent().unwrap_or_else(|| Path::new("")).join(target),
        | false => target.to_path_buf(),
    };
    normalize(&target)
}
fn materialize_links(destination: &Path, links: &[Link], regular_bytes: u64, maximum_bytes: u64) -> Result<(), PreparedArchiveError> {
    links
        .iter()
        .try_fold(regular_bytes, |bytes, link| {
            let source = destination.join(&link.target);
            let output = destination.join(&link.path);
            metadata(&source)
                .map_err(PreparedArchiveError::from)
                .and_then(|metadata| match metadata.is_file() {
                    | true => bytes
                        .checked_add(metadata.len())
                        .filter(|total| *total <= maximum_bytes)
                        .ok_or_else(|| PreparedArchiveError::Invalid(format!("materialized links exceed the {maximum_bytes}-byte limit")))
                        .map(|total| (metadata_mode(&metadata), total)),
                    | false => Err(PreparedArchiveError::Invalid(format!(
                        "prepared archive link {} does not resolve to a regular file",
                        link.path.display()
                    ))),
                })
                .and_then(|(mode, total)| {
                    output
                        .parent()
                        .map_or(Ok(()), |parent| create_dir_all(parent).map_err(PreparedArchiveError::from))
                        .and_then(|()| std::fs::copy(source, &output).map(|_| ()).map_err(PreparedArchiveError::from))
                        .and_then(|()| set_mode(output, mode))
                        .map(|()| total)
                })
        })
        .map(|_| ())
}
#[cfg(unix)]
fn metadata_mode(metadata: &Metadata) -> u32 {
    metadata.permissions().mode()
}
#[cfg(not(unix))]
fn metadata_mode(_metadata: &Metadata) -> u32 {
    0o644
}
fn normalize(path: &Path) -> Result<PathBuf, PreparedArchiveError> {
    path.components()
        .try_fold(PathBuf::new(), |mut normalized, component| match component {
            | Component::CurDir => Ok(normalized),
            | Component::Normal(value) => {
                normalized.push(value);
                Ok(normalized)
            }
            | Component::ParentDir => match normalized.pop() {
                | true => Ok(normalized),
                | false => Err(PreparedArchiveError::Invalid(format!(
                    "prepared archive path {} escapes its root",
                    path.display()
                ))),
            },
            | Component::Prefix(_) | Component::RootDir => Err(PreparedArchiveError::Invalid(format!(
                "prepared archive path {} is absolute",
                path.display()
            ))),
        })
        .and_then(|normalized| match normalized.as_os_str().is_empty() {
            | true => Err(PreparedArchiveError::Invalid("prepared archive contains an empty path".to_string())),
            | false => Ok(normalized),
        })
}
fn normalize_entry(path: &Path, kind: EntryType) -> Result<Option<PathBuf>, PreparedArchiveError> {
    match path.components().all(|component| matches!(component, Component::CurDir)) {
        | true if kind.is_dir() => Ok(None),
        | true => Err(PreparedArchiveError::Invalid(
            "prepared archive root entry is not a directory".to_string(),
        )),
        | false => normalize(path).map(Some),
    }
}
fn resolve_links(paths: &BTreeSet<PathBuf>, links: &[Link]) -> Result<Vec<Link>, PreparedArchiveError> {
    let targets = links
        .iter()
        .map(|link| (link.path.clone(), link.target.clone()))
        .collect::<BTreeMap<_, _>>();
    links
        .iter()
        .map(|link| {
            (0..=links.len())
                .try_fold(link.target.clone(), |target, _| match targets.get(&target) {
                    | Some(next) if next == &target => Err(PreparedArchiveError::Invalid(format!(
                        "prepared archive link {} forms a cycle",
                        link.path.display()
                    ))),
                    | Some(next) => Ok(next.clone()),
                    | None if paths.contains(&target) => Ok(target),
                    | None => Err(PreparedArchiveError::Invalid(format!(
                        "prepared archive link {} targets missing path {}",
                        link.path.display(),
                        target.display()
                    ))),
                })
                .and_then(|target| match targets.contains_key(&target) {
                    | true => Err(PreparedArchiveError::Invalid(format!(
                        "prepared archive link {} forms a cycle",
                        link.path.display()
                    ))),
                    | false => Ok(Link {
                        path: link.path.clone(),
                        target,
                    }),
                })
        })
        .collect()
}
#[cfg(unix)]
fn set_export_permissions(path: &Path) -> Result<(), PreparedArchiveError> {
    set_permissions(path, Permissions::from_mode(0o666)).map_err(PreparedArchiveError::from)
}
#[cfg(not(unix))]
fn set_export_permissions(_path: &Path) -> Result<(), PreparedArchiveError> {
    Ok(())
}
#[cfg(unix)]
fn set_mode(path: PathBuf, mode: u32) -> Result<(), PreparedArchiveError> {
    set_permissions(path, Permissions::from_mode(mode & 0o0777)).map_err(PreparedArchiveError::from)
}
#[cfg(not(unix))]
fn set_mode(_path: PathBuf, _mode: u32) -> Result<(), PreparedArchiveError> {
    Ok(())
}
fn validate_archive_size(archive_path: &Path, maximum_bytes: u64, maximum_entries: u64) -> Result<(), PreparedArchiveError> {
    maximum_entries
        .checked_mul(4096)
        .and_then(|value| maximum_bytes.checked_add(value))
        .ok_or_else(|| PreparedArchiveError::Invalid("prepared archive limit overflowed".to_string()))
        .and_then(|overhead| {
            metadata(archive_path)
                .map_err(PreparedArchiveError::from)
                .and_then(|metadata| match metadata.len() <= overhead {
                    | true => Ok(()),
                    | false => Err(PreparedArchiveError::Invalid(format!(
                        "prepared archive exceeds the {overhead}-byte transfer limit"
                    ))),
                })
        })
}

#[cfg(test)]
mod tests;
