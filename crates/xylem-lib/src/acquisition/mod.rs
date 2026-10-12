//! Policy-driven, reproducible remote-source acquisition.
extern crate alloc;

use crate::remote::{redact_url, RemoteError, RemotePolicy};
use crate::util::constants::app::{ACQUISITION_CACHE_ENV, APPLICATION, ORGANIZATION, QUALIFIER};
use acorn_cmd::{args, cmd, command_string, CancellationToken, CaptureLimits, ExecutionOptions};
use acorn_core::time::{Seconds, UnitOfTime};
use acorn_host::fs::{project_cache_directory, SafePath};
use acorn_schema::hardware::Memory;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::time::Duration;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{
    canonicalize, create_dir, create_dir_all, read, read_dir, read_link, remove_dir_all, remove_file, rename, symlink_metadata, File, OpenOptions,
};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, System};
use url::Url;

mod cache;
mod executor;
mod git;
mod http;
mod policy;
mod util;
/// Current immutable acquisition-cache format.
pub const ACQUISITION_CACHE_VERSION: u32 = 1;
const LOCK_STALE_AFTER: Duration = Duration::from_secs(300);
const MONITOR_INTERVAL: Duration = Duration::from_millis(25);
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The operation a pure acquisition plan intends to perform.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionAction {
    /// Fetch an HTTP(S) resource and record its digest.
    Download,
    /// Fetch and verify an immutable Git commit.
    FetchGit,
    /// Use a caller-provided local source without remote acquisition.
    UseLocal,
}
/// Actual cache behavior observed while executing an acquisition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionCacheStatus {
    /// A verified immutable cache entry was reused.
    Hit,
    /// No cache entry existed and a verified entry was published.
    Miss,
}
/// Failure while planning, caching, or executing an acquisition.
#[derive(Debug, thiserror::Error)]
pub enum AcquisitionError {
    /// The configured cache root could not be resolved.
    #[error("resolve acquisition cache: {0}")]
    CacheRoot(acorn_host::fs::FsError),
    /// Cached content no longer matches its immutable manifest.
    #[error("acquisition cache entry is corrupt at {path}: {reason}")]
    Corrupt {
        /// Invalid cache entry.
        path: PathBuf,
        /// Integrity failure.
        reason: String,
    },
    /// An expected cryptographic or Git identity is invalid.
    #[error("invalid acquisition identity {value:?}: {reason}")]
    Identity {
        /// Rejected identity value.
        value: String,
        /// Required identity shape.
        reason: &'static str,
    },
    /// A filesystem operation failed.
    #[error("{action} {}: {source}", path.display())]
    Io {
        /// Failed operation.
        action: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Operating-system failure.
        #[source]
        source: io::Error,
    },
    /// A configured or observed resource limit was exceeded.
    #[error("acquisition exceeded {limit}: observed {observed}, maximum {maximum}")]
    Limit {
        /// Exceeded limit name.
        limit: &'static str,
        /// Configured maximum.
        maximum: u64,
        /// Observed value.
        observed: u64,
    },
    /// A module could not be acquired or prepared.
    #[error("prepare module {module}: {source}")]
    Module {
        /// Configured module name.
        module: String,
        /// Underlying preparation failure.
        #[source]
        source: io::Error,
    },
    /// Offline policy prohibited a cache miss.
    #[error("offline acquisition cache miss for {remote}")]
    Offline {
        /// Redacted remote source.
        remote: String,
    },
    /// A child process failed or was cancelled.
    #[error("{action} failed for {remote}: {reason}")]
    Process {
        /// Failed process operation.
        action: &'static str,
        /// Redacted remote source.
        remote: String,
        /// Bounded process diagnostic.
        reason: String,
    },
    /// Remote source policy rejected the request.
    #[error("remote acquisition policy rejected {remote}: {reason}")]
    Policy {
        /// Redacted remote source.
        remote: String,
        /// Policy rejection reason.
        reason: String,
    },
    /// An HTTP request or response failed.
    #[error("{action} {remote}: {reason}")]
    Request {
        /// Failed HTTP operation.
        action: &'static str,
        /// Redacted remote source.
        remote: String,
        /// Bounded request diagnostic.
        reason: String,
    },
    /// Downloaded bytes did not match their declared SHA-256 identity.
    #[error("SHA-256 mismatch for {remote}: expected {expected}, calculated {calculated}")]
    Sha256Mismatch {
        /// Calculated lowercase digest.
        calculated: String,
        /// Declared lowercase digest.
        expected: String,
        /// Redacted remote source.
        remote: String,
    },
}
/// Immutable identity required before remote acquisition begins.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcquisitionIdentity {
    /// Git origin and full commit object ID.
    GitCommit {
        /// Full commit object ID.
        commit: String,
        /// Normalized repository origin.
        origin: String,
    },
    /// Algorithm-qualified expected content digest.
    Sha256 {
        /// Algorithm-qualified lowercase SHA-256 digest.
        digest: String,
    },
}
/// Explicit source categories with distinct materialization contracts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcquisitionSource {
    /// Commit-pinned Git repository.
    Git {
        /// Full commit object ID.
        commit: String,
        /// Repository URL.
        url: String,
    },
    /// Digest-pinned HTTP(S) executable bytes.
    HttpBinary {
        /// Expected algorithm-qualified lowercase SHA-256 digest.
        sha256: String,
        /// HTTP(S) source URL.
        url: String,
    },
    /// HTTP(S) executable bytes acquired through the explicit checksum escape hatch.
    HttpBinaryUnverified {
        /// HTTP(S) source URL.
        url: String,
    },
    /// Digest-pinned HTTP(S) configuration bytes.
    HttpDocument {
        /// Expected algorithm-qualified lowercase SHA-256 digest.
        sha256: String,
        /// HTTP(S) source URL.
        url: String,
    },
    /// Caller-owned local filesystem source.
    Local {
        /// Caller-owned local path.
        path: PathBuf,
    },
}
/// Immutable, content-addressed acquisition cache.
#[derive(Clone, Debug)]
pub struct AcquisitionCache {
    root: PathBuf,
}
/// Actual, non-secret provenance emitted by an acquisition executor.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AcquisitionEvidence {
    /// Unix timestamp when the immutable cache entry was first acquired.
    pub acquired_at_unix_seconds: u64,
    /// Number of verified payload bytes.
    pub size: Memory,
    /// Whether an immutable cache entry was reused or populated.
    pub cache: AcquisitionCacheStatus,
    /// Number of verified payload entries.
    pub entries: u64,
    /// Immutable identity that was verified.
    pub identity: AcquisitionIdentity,
    /// Effective resource limits.
    pub limits: AcquisitionLimits,
    /// Stable module ancestry.
    pub module_path: Vec<String>,
    /// Operator capabilities and execution mode applied to this acquisition.
    pub policy: AcquisitionPolicyEvidence,
    /// Redacted source suitable for durable reports.
    pub source: String,
    /// Canonical tree digest for Git content.
    pub tree_digest: Option<String>,
}
/// Bounded executor for pure acquisition plans.
#[derive(Clone, Debug)]
pub struct AcquisitionExecutor {
    cache: AcquisitionCache,
    policy: AcquisitionPolicy,
}
/// Resource ceilings enforced by every acquisition backend.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AcquisitionLimits {
    /// Maximum HTTP response bytes for a remote binary.
    pub binary: Memory,
    /// HTTP connection timeout in seconds.
    pub connect_timeout: Seconds,
    /// Maximum HTTP response bytes for a configuration document.
    pub document: Memory,
    /// Maximum checked-out Git tree entries.
    pub git_entries: u64,
    /// Maximum Git staging bytes, including the object database.
    pub git_staging: Memory,
    /// Shared deadline for the complete Git acquisition.
    pub git_timeout: Seconds,
    /// Maximum checked-out Git tree bytes.
    pub git_tree: Memory,
    /// Total HTTP request timeout in seconds.
    pub http_timeout: Seconds,
    /// Maximum HTTP redirects.
    pub redirects: u64,
    /// Maximum retained bytes for each Git output stream.
    pub process_output: Memory,
}
/// Pure, deterministic acquisition intent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AcquisitionPlan {
    /// Operation required when the cache does not contain this identity.
    pub action: AcquisitionAction,
    /// Deterministic immutable cache key.
    pub cache_key: Option<String>,
    /// Expected immutable identity.
    pub identity: Option<AcquisitionIdentity>,
    /// Effective limits.
    pub limits: AcquisitionLimits,
    /// Stable module ancestry.
    pub module_path: Vec<String>,
    /// Fully validated typed source.
    pub source: AcquisitionSource,
}
/// Trusted operator policy resolved before remote configuration loading.
#[derive(Clone, Debug, PartialEq)]
pub struct AcquisitionPolicy {
    allow_insecure_http: bool,
    git_ssh: Option<GitSshPolicy>,
    limits: AcquisitionLimits,
    offline: bool,
    remote: RemotePolicy,
    skip_checksum_verification: bool,
}
/// Non-secret policy decisions retained with acquisition provenance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcquisitionPolicyEvidence {
    /// Whether the operator allowed cleartext HTTP.
    pub allow_insecure_http: bool,
    /// Whether the operator supplied an isolated SSH identity and known-hosts file.
    pub git_ssh_enabled: bool,
    /// Whether execution prohibited cache misses from reaching the network.
    pub offline: bool,
    /// Whether the operator disabled pre-download checksum verification.
    pub skip_checksum_verification: bool,
}
/// Verified materialization and its auditable provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct AcquisitionResult {
    /// Durable, non-secret provenance for reporters and journals.
    pub evidence: AcquisitionEvidence,
    /// Immutable file or directory containing the verified content.
    pub path: PathBuf,
}
/// Operator-provided SSH identity and host-verification material for Git.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitSshPolicy {
    /// Private key passed directly to SSH with agent use disabled.
    pub identity: PathBuf,
    /// Dedicated known-hosts file used with strict host-key checking.
    pub known_hosts: PathBuf,
}

#[cfg(test)]
mod tests;
