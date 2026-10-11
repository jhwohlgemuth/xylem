//! Canonical Xylem application identity

/// Canonical application name.
pub const APPLICATION: &str = "xylem";
/// Organization name.
pub const ORGANIZATION: &str = "ornl";
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
