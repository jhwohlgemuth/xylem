//! Bounded OCI artifact-tree acquisition through the ORAS CLI.
use super::git::remaining_timeout;
use super::*;
use std::ffi::OsString;

pub(super) fn pull_tree(
    reference: &str,
    destination: &Path,
    maximum: &Memory,
    maximum_entries: u64,
    output: &Memory,
    started: Instant,
    timeout: Seconds,
    allow_insecure_http: bool,
) -> Result<(), AcquisitionError> {
    let registry_reference = reference.strip_prefix("oci://").unwrap_or(reference);
    remaining_timeout(started, timeout).and_then(|manifest_timeout| {
        let plain_http = match allow_insecure_http {
            | true => args!["--plain-http"],
            | false => Vec::new(),
        };
        run_oras(
            "inspect OCI manifest",
            registry_reference,
            args!["manifest", "fetch", "--pretty", ..plain_http, registry_reference],
            output,
            manifest_timeout,
        )
        .and_then(|manifest| validate_manifest_size(&manifest, maximum, maximum_entries))
        .and_then(|()| remaining_timeout(started, timeout))
        .and_then(|pull_timeout| {
            let plain_http = match allow_insecure_http {
                | true => args!["--plain-http"],
                | false => Vec::new(),
            };
            run_oras(
                "pull OCI artifact tree",
                registry_reference,
                args![
                    "pull",
                    "--no-tty",
                    ..plain_http,
                    ("--output", destination.as_os_str()),
                    registry_reference
                ],
                output,
                pull_timeout,
            )
            .map(|_| ())
        })
    })
}
fn run_oras(
    action: &'static str,
    reference: &str,
    arguments: impl IntoIterator<Item = OsString>,
    output: &Memory,
    timeout: Duration,
) -> Result<String, AcquisitionError> {
    let options = ExecutionOptions::new()
        .with_capture_limits(CaptureLimits::new(
            Some(output.checked_bytes().unwrap_or_default().min(usize::MAX as u64) as usize),
            Some(output.checked_bytes().unwrap_or_default().min(usize::MAX as u64) as usize),
        ))
        .with_timeout(timeout);
    cmd!(exec checked "oras", arguments; opt: &options)
        .map(|result| String::from_utf8_lossy(result.stdout().bytes()).into_owned())
        .map_err(|source| AcquisitionError::Process {
            action,
            remote: reference.to_string(),
            reason: source.to_string(),
        })
}
fn validate_manifest_size(manifest: &str, maximum: &Memory, maximum_entries: u64) -> Result<(), AcquisitionError> {
    serde_json::from_str::<serde_json::Value>(manifest)
        .map_err(|_error| AcquisitionError::Identity {
            value: "OCI manifest".to_string(),
            reason: "registry returned an invalid OCI manifest",
        })
        .and_then(|manifest| match manifest.get("schemaVersion").and_then(serde_json::Value::as_u64) {
            | Some(2) => manifest
                .get("layers")
                .and_then(serde_json::Value::as_array)
                .filter(|layers| !layers.is_empty())
                .ok_or_else(|| AcquisitionError::Identity {
                    value: "OCI manifest".to_string(),
                    reason: "OCI module artifact must be an image manifest with file layers",
                })
                .and_then(|descriptors| {
                    let observed_entries = u64::try_from(descriptors.len()).unwrap_or(u64::MAX);
                    match observed_entries > maximum_entries {
                        | true => Err(AcquisitionError::Limit {
                            limit: "OCI artifact layers",
                            maximum: maximum_entries,
                            observed: observed_entries,
                        }),
                        | false => manifest
                            .get("config")
                            .and_then(|config| config.get("size"))
                            .and_then(serde_json::Value::as_u64)
                            .ok_or_else(|| AcquisitionError::Identity {
                                value: "OCI manifest".to_string(),
                                reason: "OCI module artifact has no valid config descriptor",
                            })
                            .and_then(|config_bytes| {
                                descriptors.iter().try_fold(config_bytes, |total, descriptor| {
                                    descriptor
                                        .get("size")
                                        .and_then(serde_json::Value::as_u64)
                                        .and_then(|size| total.checked_add(size))
                                        .ok_or_else(|| AcquisitionError::Identity {
                                            value: "OCI manifest".to_string(),
                                            reason: "OCI manifest contains an invalid or overflowing blob size",
                                        })
                                })
                            })
                            .and_then(|total| {
                                let maximum = maximum.checked_bytes().unwrap_or_default();
                                match total <= maximum {
                                    | true => Ok(()),
                                    | false => Err(AcquisitionError::Limit {
                                        limit: "OCI artifact bytes",
                                        maximum,
                                        observed: total,
                                    }),
                                }
                            }),
                    }
                }),
            | _ => Err(AcquisitionError::Identity {
                value: "OCI manifest".to_string(),
                reason: "OCI module artifact must use manifest schema version 2",
            }),
        })
}
