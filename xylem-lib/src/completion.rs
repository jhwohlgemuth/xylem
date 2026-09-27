//! Declared output completion and resumable execution markers
extern crate alloc;

use crate::script::{EnvironmentValue, Module, StringTemplate};
use acorn::io::PathConversion;
use acorn_core::validation::ValidationError;
use acorn_schema::validation::{Validate, ValidationReport};
use alloc::collections::BTreeMap;
use convert_case::{Case, Casing};
use core::fmt;
use core::iter::once;
use glob::{glob, Pattern};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{create_dir_all, read_to_string, remove_file, rename, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Resume decision for one module and input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionDecision {
    /// Execute the module.
    Run,
    /// Skip the module because its declared outputs and marker are current.
    SkipExisting,
}
/// Failure while validating or inspecting a completion declaration.
#[derive(Debug)]
pub enum CompletionError {
    /// A filesystem operation failed.
    Io(std::io::Error),
    /// The declaration violates a completion invariant.
    Invalid(String),
    /// Canonical marker identity serialization failed.
    Json(serde_json::Error),
}
/// Completion outputs and identity marker declared by a module.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    /// Optional exact marker path template written after successful execution.
    ///
    /// When omitted, the marker is placed under `.xylem` beneath the first output's literal directory prefix.
    pub marker: Option<String>,
    /// Output path or glob templates that must all resolve to non-empty files.
    pub outputs: Vec<String>,
}
#[derive(Serialize)]
struct CompletionIdentity<'a> {
    effective_variables: BTreeMap<&'a str, &'a str>,
    expanded_marker: &'a str,
    expanded_outputs: &'a [String],
    module: &'a Module,
    schema_version: u32,
}
/// Resolved completion information for one module and input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionPlan {
    /// Resume decision made from the current filesystem observation.
    pub decision: CompletionDecision,
    /// Canonical marker contents.
    pub identity: String,
    /// Resolved marker path.
    pub marker: PathBuf,
}
struct Containment<'a> {
    path: &'a Path,
    marker: &'a Path,
    roots: &'a [PathBuf],
    outputs: &'a [PathBuf],
}
/// Purely resolved completion declaration produced during planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCompletion {
    /// Canonical marker contents.
    pub identity: String,
    /// Resolved exact marker path.
    pub marker: PathBuf,
    /// Resolved output glob patterns in deterministic order.
    pub outputs: Vec<PathBuf>,
    /// Allowed artifact roots used for runtime symlink containment checks.
    pub roots: Vec<PathBuf>,
}
impl Completion {
    fn resolve_marker(&self, module: &Module, outputs: &[String], expand: &impl Fn(&str) -> String) -> Result<String, CompletionError> {
        self.marker
            .as_deref()
            .map_or_else(|| default_marker(module, outputs), |marker| Ok(expand(marker)))
    }
    fn validate_resolved(&self, marker: &str, outputs: &[String]) -> Result<(), CompletionError> {
        let empty_outputs = self.outputs.is_empty();
        let empty_marker = marker.is_empty();
        let marker_pattern = Pattern::new(marker).map(|pattern| pattern.as_str() == marker && !marker.contains(['*', '?', '[', ']']));
        let invalid_output = outputs.iter().find_map(|output| Pattern::new(output).err().map(|error| (output, error)));
        let unresolved_variable = once(marker).chain(outputs.iter().map(String::as_str)).find(|value| value.contains("{{"));
        match (empty_outputs, empty_marker, marker_pattern, invalid_output, unresolved_variable) {
            | (true, _, _, _, _) => Err(CompletionError::invalid("outputs must not be empty")),
            | (_, true, _, _, _) => Err(CompletionError::invalid("marker must not be empty")),
            | (_, _, Ok(false), _, _) => Err(CompletionError::invalid("marker must be an exact path without glob syntax")),
            | (_, _, Err(error), _, _) => Err(CompletionError::invalid(format!("invalid marker path: {error}"))),
            | (_, _, _, Some((output, error)), _) => Err(CompletionError::invalid(format!("invalid output glob {output}: {error}"))),
            | (_, _, _, _, Some(value)) => Err(CompletionError::invalid(format!("unresolved variable in completion path {value}"))),
            | (false, false, Ok(true), None, None) => Ok(()),
        }
    }
}
impl CompletionError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
impl core::error::Error for CompletionError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::Io(error) => Some(error),
            | Self::Json(error) => Some(error),
            | Self::Invalid(_) => None,
        }
    }
}
impl fmt::Display for CompletionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Io(error) => write!(formatter, "completion I/O failed: {error}"),
            | Self::Invalid(message) => write!(formatter, "invalid completion declaration: {message}"),
            | Self::Json(error) => write!(formatter, "completion identity serialization failed: {error}"),
        }
    }
}
impl From<serde_json::Error> for CompletionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl From<std::io::Error> for CompletionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl CompletionPlan {
    /// Resolve and validate a module completion declaration without reading the host.
    pub fn resolve(
        module: &Module,
        variables: &HashMap<String, String>,
        workflow_output: Option<&str>,
    ) -> Result<Option<ResolvedCompletion>, CompletionError> {
        module.completion.as_ref().map_or(Ok(None), |completion| {
            let plain_variables = unprefixed_variables(variables);
            let expand = |value: &str| StringTemplate::new(value).expand(plain_variables.clone());
            let mut output_patterns = completion.outputs.iter().map(|output| expand(output)).collect::<Vec<_>>();
            output_patterns.sort();
            output_patterns.dedup();
            completion.resolve_marker(module, &output_patterns, &expand).and_then(|marker_text| {
                completion.validate_resolved(&marker_text, &output_patterns).and_then(|()| {
                    allowed_roots(workflow_output, variables, &expand).and_then(|roots| {
                        let marker = resolve_path(&marker_text);
                        let outputs = output_patterns.iter().map(|output| resolve_path(output)).collect::<Result<Vec<_>, _>>();
                        marker
                            .and_then(|marker| outputs.map(|outputs| (marker, outputs)))
                            .and_then(|(marker, outputs)| {
                                Containment {
                                    path: &marker,
                                    marker: &marker,
                                    roots: &roots,
                                    outputs: &outputs,
                                }
                                .validate()
                                .map_err(|report| CompletionError::invalid(report.to_string()))
                                .and_then(|()| {
                                    let effective_variables = plain_variables
                                        .iter()
                                        .map(|(key, value)| (key.as_str(), value.as_str()))
                                        .collect::<BTreeMap<_, _>>();
                                    serde_json::to_string(&CompletionIdentity {
                                        effective_variables,
                                        expanded_marker: &marker_text,
                                        expanded_outputs: &output_patterns,
                                        module,
                                        schema_version: 1,
                                    })
                                    .map_err(CompletionError::from)
                                    .map(|identity| {
                                        Some(ResolvedCompletion {
                                            identity,
                                            marker,
                                            outputs,
                                            roots,
                                        })
                                    })
                                })
                            })
                    })
                })
            })
        })
    }
    /// Resolve and inspect a module completion declaration.
    pub fn inspect(
        module: &Module,
        variables: &HashMap<String, String>,
        resume: bool,
        workflow_output: Option<&str>,
    ) -> Result<Option<Self>, CompletionError> {
        Self::resolve(module, variables, workflow_output).and_then(|resolved| {
            resolved.map_or(Ok(None), |resolved| {
                resolved
                    .validate_optional_existing_containment()
                    .and_then(|()| resolved.outputs_complete())
                    .and_then(|complete| {
                        match read_to_string(&resolved.marker) {
                            | Ok(value) => Ok(value == format!("{}\n", resolved.identity)),
                            | Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                            | Err(error) => Err(CompletionError::from(error)),
                        }
                        .map(|marker_matches| {
                            let decision = match resume && complete && marker_matches {
                                | true => CompletionDecision::SkipExisting,
                                | false => CompletionDecision::Run,
                            };
                            Some(Self {
                                decision,
                                identity: resolved.identity,
                                marker: resolved.marker,
                            })
                        })
                    })
            })
        })
    }
    /// Atomically persist the canonical marker identity.
    pub fn write(&self) -> Result<(), CompletionError> {
        self.marker
            .parent()
            .map_or(Ok(()), create_dir_all)
            .map_err(CompletionError::from)
            .and_then(|()| {
                let file_name = self.marker.file_name().and_then(|value| value.to_str()).unwrap_or("completion");
                let temporary = self.marker.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
                let result = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)
                    .map_err(CompletionError::from)
                    .and_then(|mut file| {
                        file.write_all(format!("{}\n", self.identity).as_bytes())
                            .map_err(CompletionError::from)
                            .and_then(|()| file.flush().map_err(CompletionError::from))
                            .and_then(|()| file.sync_all().map_err(CompletionError::from))
                    })
                    .and_then(|()| rename(&temporary, &self.marker).map_err(CompletionError::from));
                match result {
                    | Ok(()) => Ok(()),
                    | Err(error) => {
                        let _ = remove_file(&temporary);
                        Err(error)
                    }
                }
            })
    }
}
impl Containment<'_> {
    fn outputs_complete(&self) -> Result<bool, CompletionError> {
        self.outputs.iter().try_fold(true, |complete, output| {
            output.to_str().map_or_else(
                || Err(CompletionError::invalid(format!("output pattern is not UTF-8: {}", output.display()))),
                |pattern| {
                    glob(pattern)
                        .map_err(|error| CompletionError::invalid(format!("invalid output glob {pattern}: {error}")))
                        .and_then(|mut entries| {
                            entries
                                .try_fold((false, complete), |(_, complete), entry| {
                                    entry
                                        .map_err(|error| CompletionError::invalid(format!("inspect output glob {pattern}: {error}")))
                                        .and_then(|path| {
                                            Containment {
                                                path: &path,
                                                marker: self.marker,
                                                roots: self.roots,
                                                outputs: self.outputs,
                                            }
                                            .validate_existing()
                                            .and_then(|()| {
                                                path.metadata()
                                                    .map_err(CompletionError::from)
                                                    .map(|metadata| (true, complete && metadata.is_file() && metadata.len() > 0))
                                            })
                                        })
                                })
                                .map(|(matched, complete)| matched && complete)
                        })
                },
            )
        })
    }
    fn validate_existing(&self) -> Result<(), CompletionError> {
        self.path.canonicalize().map_err(CompletionError::from).and_then(|path| {
            self.roots
                .iter()
                .filter_map(|root| root.canonicalize().ok())
                .any(|root| path.starts_with(root))
                .then_some(())
                .ok_or_else(|| {
                    CompletionError::invalid(format!(
                        "completion output {} resolves outside every declared output root",
                        path.display()
                    ))
                })
        })
    }
    fn validate_optional_existing(&self) -> Result<(), CompletionError> {
        self.path.try_exists().map_err(CompletionError::from).and_then(|exists| match exists {
            | true => self.validate_existing(),
            | false => {
                let canonical_roots = self.roots.iter().filter_map(|root| root.canonicalize().ok()).collect::<Vec<_>>();
                match canonical_roots.is_empty() {
                    | true => Ok(()),
                    | false => self
                        .path
                        .ancestors()
                        .skip(1)
                        .find(|ancestor| matches!(ancestor.try_exists(), Ok(true)))
                        .map_or(Ok(()), |ancestor| {
                            ancestor.canonicalize().map_err(CompletionError::from).and_then(|ancestor| {
                                canonical_roots
                                    .iter()
                                    .any(|root| ancestor.starts_with(root))
                                    .then_some(())
                                    .ok_or_else(|| {
                                        CompletionError::invalid(format!(
                                            "completion marker parent {} resolves outside every declared output root",
                                            ancestor.display()
                                        ))
                                    })
                            })
                        }),
                }
            }
        })
    }
}
impl Validate for Containment<'_> {
    fn validate(&self) -> Result<(), ValidationReport> {
        once(self.marker)
            .chain(self.outputs.iter().map(PathBuf::as_path))
            .find(|path| !self.roots.iter().any(|root| path.starts_with(root)))
            .map_or(Ok(()), |path| {
                Err(ValidationReport::from_error(
                    "path",
                    ValidationError::new("containment")
                        .with_message(format!("completion path {} is outside every declared output root", path.display())),
                ))
            })
    }
}
impl ResolvedCompletion {
    fn outputs_complete(&self) -> Result<bool, CompletionError> {
        Containment {
            path: &self.marker,
            marker: &self.marker,
            roots: &self.roots,
            outputs: &self.outputs,
        }
        .outputs_complete()
    }
    fn validate_optional_existing_containment(&self) -> Result<(), CompletionError> {
        Containment {
            path: &self.marker,
            marker: &self.marker,
            roots: &self.roots,
            outputs: &self.outputs,
        }
        .validate_optional_existing()
    }
}
fn allowed_roots(
    workflow_output: Option<&str>,
    variables: &HashMap<String, String>,
    expand: &impl Fn(&str) -> String,
) -> Result<Vec<PathBuf>, CompletionError> {
    let output_key = format!("_{}_OUTPUT_DIRECTORY", EnvironmentValue::PREFIX.to_uppercase());
    let values = workflow_output
        .map(expand)
        .into_iter()
        .chain(variables.get(&output_key).cloned())
        .filter(|value| !value.is_empty())
        .map(|value| resolve_path(&value))
        .collect::<Result<Vec<_>, _>>();
    values.and_then(|values| match values.is_empty() {
        | true => Err(CompletionError::invalid(
            "completion requires Config.output or an OUTPUT_DIRECTORY variable",
        )),
        | false => Ok(values),
    })
}
fn default_marker(module: &Module, outputs: &[String]) -> Result<String, CompletionError> {
    outputs.first().map_or_else(
        || Err(CompletionError::invalid("outputs must not be empty")),
        |output| {
            let output = PathBuf::from(output);
            let (mut prefix, contains_glob) = output
                .components()
                .fold((PathBuf::new(), false), |(mut prefix, contains_glob), component| {
                    let component_contains_glob = component.as_os_str().to_string_lossy().contains(['*', '?', '[', ']']);
                    if !contains_glob && !component_contains_glob {
                        prefix.push(component.as_os_str());
                    }
                    (prefix, contains_glob || component_contains_glob)
                });
            if !contains_glob {
                prefix.pop();
            }
            Ok(prefix
                .join(".xylem")
                .join(format!("{}.json", module.name.to_case(Case::Snake)))
                .display()
                .to_string())
        },
    )
}
fn resolve_path(value: &str) -> Result<PathBuf, CompletionError> {
    let path = PathBuf::from(value);
    let absolute = match path.is_absolute() {
        | true => path,
        | false => std::env::current_dir().unwrap_or_default().join(path),
    };
    absolute
        .lexically_normalized()
        .map_err(|error| CompletionError::invalid(error.to_string()))
}
fn unprefixed_variables(variables: &HashMap<String, String>) -> Vec<(String, String)> {
    let prefix = format!("_{}_", EnvironmentValue::PREFIX.to_uppercase());
    variables
        .iter()
        .map(|(key, value)| (key.strip_prefix(&prefix).unwrap_or(key).to_string(), value.clone()))
        .collect()
}
