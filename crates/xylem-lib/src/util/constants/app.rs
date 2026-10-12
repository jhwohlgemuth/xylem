//! Canonical Xylem application identity

/// Optional acquisition-cache root override for tests and embedded deployments.
pub const ACQUISITION_CACHE_ENV: &str = "XYLEM_ACQUISITION_CACHE";
/// Canonical application name.
pub const APPLICATION: &str = "xylem";
/// Organization name.
pub const ORGANIZATION: &str = "ornl";
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
