//! Canonical Xylem application identity

/// Optional acquisition-cache root override for tests and embedded deployments.
pub const ACQUISITION_CACHE_ENV: &str = "XYLEM_ACQUISITION_CACHE";
/// Canonical application name.
pub const APPLICATION: &str = "xylem";
/// Default Linux image used by GitLab workflow exports.
pub const GITLAB_DEFAULT_IMAGE: &str = "savannah.ornl.gov/research-enablement/acorn/rust:latest";
/// SHA-256 digest of the Linux x86_64 Miniforge installer used by GitLab exports.
pub const MINIFORGE_SHA256: &str = "281b0ac7d550802efc81af633225a5e6116d29ae72f3ab4eae7168c3931a4c05";
/// Miniforge release used by GitLab exports.
pub const MINIFORGE_VERSION: &str = "26.7.2-0";
/// Path suffixes omitted from module source copies before traversal or symlink resolution.
pub(crate) const MODULE_SOURCE_EXCLUSIONS: [&str; 1] = [".pixi/envs"];
/// Organization name.
pub const ORGANIZATION: &str = "ornl";
/// SHA-256 digest of the Linux x86_64 Pixi binary used by GitLab exports.
pub const PIXI_SHA256: &str = "4383aed18b2d5569cf34a19638daf954aa4415cc87ad3a9da9f34059cc4a004c";
/// Pixi release used by GitLab exports.
pub const PIXI_VERSION: &str = "0.75.0";
/// First label used to store a split policy digest.
pub(crate) const POLICY_DIGEST_FIRST_LABEL: &str = "gov.ornl.xylem.policy-a";
/// Second label used to store a split policy digest.
pub(crate) const POLICY_DIGEST_SECOND_LABEL: &str = "gov.ornl.xylem.policy-b";
/// OpenShell network policy name for preparation.
pub(crate) const PREPARATION_NETWORK_POLICY: &str = "xylem-preparation";
/// Organization qualifier.
pub const QUALIFIER: &str = "org";
/// Unix-millisecond creation label for a managed sandbox resource.
pub(crate) const SANDBOX_CREATED_LABEL: &str = "gov.ornl.xylem.created-at-ms";
/// Stable input-index label for a managed sandbox resource.
pub(crate) const SANDBOX_INPUT_LABEL: &str = "gov.ornl.xylem.input-index";
/// Ownership label for resources managed by Xylem.
pub(crate) const SANDBOX_MANAGED_LABEL: &str = "gov.ornl.xylem.sandbox";
/// Lifecycle-phase label for a managed sandbox resource.
pub(crate) const SANDBOX_PHASE_LABEL: &str = "gov.ornl.xylem.phase";
/// Effective-policy label for a managed sandbox resource.
pub(crate) const SANDBOX_POLICY_LABEL: &str = "gov.ornl.xylem.policy";
/// Provider label for a managed sandbox resource.
pub(crate) const SANDBOX_PROVIDER_LABEL: &str = "gov.ornl.xylem.provider";
/// Current managed sandbox resource label version.
pub(crate) const SANDBOX_RESOURCE_VERSION: u32 = 1;
/// Stable run identifier label for a managed sandbox resource.
pub(crate) const SANDBOX_RUN_LABEL: &str = "gov.ornl.xylem.run-id";
/// Host-private artifact staging label for a managed sandbox resource.
pub(crate) const SANDBOX_STAGING_LABEL: &str = "gov.ornl.xylem.artifact-staging";
/// Lifecycle-version label for a managed sandbox resource.
pub(crate) const SANDBOX_VERSION_LABEL: &str = "gov.ornl.xylem.lifecycle-version";
/// System paths kept read-only in an OpenShell sandbox.
pub(crate) const SYSTEM_READ_ONLY_PATHS: [&str; 9] = [
    "/bin",
    "/dev/urandom",
    "/etc",
    "/lib",
    "/lib64",
    "/proc",
    "/sys/fs/cgroup",
    "/usr",
    "/var/log",
];
