use super::util::sha256;
use super::*;
use acorn_core::{
    validation::{digest, ValidationError},
    Location,
};
use acorn_schema::validation::{rules, Validate, ValidationReport};

impl AcquisitionLimits {
    /// Intersect these ceilings with a child configuration's ceilings.
    pub fn restrict(&self, child: &Self) -> Self {
        Self {
            binary: minimum_memory(&self.binary, &child.binary),
            connect_timeout: self.connect_timeout.min(child.connect_timeout),
            document: minimum_memory(&self.document, &child.document),
            git_entries: self.git_entries.min(child.git_entries),
            git_staging: minimum_memory(&self.git_staging, &child.git_staging),
            git_timeout: self.git_timeout.min(child.git_timeout),
            git_tree: minimum_memory(&self.git_tree, &child.git_tree),
            http_timeout: self.http_timeout.min(child.http_timeout),
            redirects: self.redirects.min(child.redirects),
            process_output: minimum_memory(&self.process_output, &child.process_output),
        }
    }
    pub(crate) fn validate(&self) -> Result<(), AcquisitionError> {
        let invalid_memory = [
            ("binary", &self.binary),
            ("document", &self.document),
            ("git_staging", &self.git_staging),
            ("git_tree", &self.git_tree),
            ("process_output", &self.process_output),
        ]
        .into_iter()
        .find(|(_, value)| value.checked_bytes().is_none_or(|bytes| bytes == 0))
        .map(|(name, _)| name);
        let invalid_scalar = [
            ("connect_timeout", self.connect_timeout.get()),
            ("git_entries", self.git_entries),
            ("git_timeout", self.git_timeout.get()),
            ("http_timeout", self.http_timeout.get()),
        ]
        .into_iter()
        .find(|(_, value)| *value == 0)
        .map(|(name, _)| name);
        invalid_memory.or(invalid_scalar).map_or(Ok(()), |name| {
            Err(AcquisitionError::Identity {
                value: name.to_string(),
                reason: "acquisition limits must be greater than zero",
            })
        })
    }
}
impl Default for AcquisitionLimits {
    fn default() -> Self {
        Self {
            binary: Memory::mb(512),
            connect_timeout: Seconds::new(10),
            document: Memory::mb(1),
            git_entries: 100_000,
            git_staging: Memory::gb(2),
            git_timeout: Seconds::new(300),
            git_tree: Memory::gb(1),
            http_timeout: Seconds::new(300),
            redirects: 5,
            process_output: Memory::kb(256),
        }
    }
}
impl AcquisitionPlan {
    /// Construct a side-effect-free acquisition plan after validating source policy and identity.
    pub fn build(module_path: Vec<String>, source: AcquisitionSource, policy: &AcquisitionPolicy) -> Result<Self, AcquisitionError> {
        policy.validate_source(&source).and_then(|()| {
            let planned = match &source {
                | AcquisitionSource::Git { commit, url } => normalize_origin(url).map(|origin| {
                    let identity = AcquisitionIdentity::GitCommit {
                        commit: commit.clone(),
                        origin: origin.clone(),
                    };
                    let digest = sha256(format!("{origin}\0{commit}").as_bytes());
                    let key = digest.strip_prefix("sha256:").unwrap_or(&digest).to_string();
                    (AcquisitionAction::FetchGit, Some(identity), Some(key))
                }),
                | AcquisitionSource::Oci { digest, .. } => Ok((
                    AcquisitionAction::FetchOci,
                    Some(AcquisitionIdentity::OciDigest { digest: digest.clone() }),
                    Some(digest.strip_prefix("sha256:").unwrap_or(digest).to_string()),
                )),
                | AcquisitionSource::HttpBinary { sha256, .. } | AcquisitionSource::HttpDocument { sha256, .. } => Ok((
                    AcquisitionAction::Download,
                    Some(AcquisitionIdentity::Sha256 { digest: sha256.clone() }),
                    Some(sha256.strip_prefix("sha256:").unwrap_or(sha256).to_string()),
                )),
                | AcquisitionSource::HttpBinaryUnverified { .. } => Ok((AcquisitionAction::Download, None, None)),
                | AcquisitionSource::Local { .. } => Ok((AcquisitionAction::UseLocal, None, None)),
            };
            planned.map(|(action, identity, cache_key)| Self {
                action,
                cache_key,
                identity,
                limits: policy.limits.clone(),
                module_path,
                source,
            })
        })
    }
}
impl AcquisitionPolicy {
    pub(super) fn validate_oci_request(&self, url: &Url, previous: Option<&Url>) -> Result<(), AcquisitionError> {
        let insecure = url.scheme() == "http" && self.allow_insecure_http;
        let downgrade = previous.is_some_and(|previous| previous.scheme() == "https" && url.scheme() == "http");
        if !matches!(url.scheme(), "https" | "http") || (url.scheme() == "http" && !insecure) || downgrade {
            Err(policy_error(url.as_str(), "OCI request requires HTTPS without a downgrade"))
        } else if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            Err(policy_error(url.as_str(), "OCI request URL contains credentials or a fragment"))
        } else {
            self.ensure_allowed(url.as_str())
        }
    }
    /// Resolve a fail-closed policy from exact hosts and operator-only capabilities.
    pub fn from_hosts(
        hosts: Option<&[String]>,
        allow_insecure_http: bool,
        git_ssh: Option<GitSshPolicy>,
        offline: bool,
        limits: AcquisitionLimits,
    ) -> Result<Self, AcquisitionError> {
        limits.validate().and_then(|()| {
            RemotePolicy::from_required_hosts(hosts)
                .map_err(|source| AcquisitionError::Policy {
                    remote: "configured allowlist".to_string(),
                    reason: source.to_string(),
                })
                .and_then(|remote| {
                    git_ssh
                        .as_ref()
                        .map_or(Ok(()), Validate::validate)
                        .map_err(|report| AcquisitionError::Policy {
                            remote: "SSH Git".to_string(),
                            reason: report.to_string(),
                        })
                        .map(|()| Self {
                            allow_insecure_http,
                            git_ssh,
                            limits,
                            offline,
                            remote,
                            skip_checksum_verification: false,
                        })
                })
        })
    }
    /// Enable the explicit operator escape hatch for HTTP binary checksums.
    pub fn with_checksum_verification_disabled(self, disabled: bool) -> Self {
        Self {
            skip_checksum_verification: self.skip_checksum_verification || disabled,
            ..self
        }
    }
    /// Return whether the operator disabled pre-download checksum verification.
    pub fn checksum_verification_disabled(&self) -> bool {
        self.skip_checksum_verification
    }
    /// Return the effective resource ceilings.
    pub fn limits(&self) -> &AcquisitionLimits {
        &self.limits
    }
    /// Return whether cache misses are prohibited from reaching the network.
    pub fn offline(&self) -> bool {
        self.offline
    }
    /// Derive a nested policy that can only narrow hosts and resource ceilings.
    pub fn restrict(&self, hosts: Option<&[String]>, limits: &AcquisitionLimits) -> Result<Self, AcquisitionError> {
        limits.validate().and_then(|()| {
            self.remote
                .restrict_required(hosts)
                .map(|remote| Self {
                    allow_insecure_http: self.allow_insecure_http,
                    git_ssh: self.git_ssh.clone(),
                    limits: self.limits.restrict(limits),
                    offline: self.offline,
                    remote,
                    skip_checksum_verification: self.skip_checksum_verification,
                })
                .map_err(|source| AcquisitionError::Policy {
                    remote: "nested allowlist".to_string(),
                    reason: source.to_string(),
                })
        })
    }
    /// Preserve every capability while tightening offline behavior.
    pub fn restrict_offline(&self, offline: bool) -> Self {
        Self {
            allow_insecure_http: self.allow_insecure_http,
            git_ssh: self.git_ssh.clone(),
            limits: self.limits.clone(),
            offline: self.offline || offline,
            remote: self.remote.clone(),
            skip_checksum_verification: self.skip_checksum_verification,
        }
    }
    pub(super) fn http_client(&self) -> Result<Client, AcquisitionError> {
        let remote = self.remote.clone();
        let allow_insecure_http = self.allow_insecure_http;
        let redirects = self.limits.redirects as usize;
        Client::builder()
            .connect_timeout(Duration::from(self.limits.connect_timeout))
            .timeout(Duration::from(self.limits.http_timeout))
            .redirect(Policy::custom(move |attempt| {
                let source_scheme = attempt.previous().first().map(Url::scheme);
                let downgrade = source_scheme == Some("https") && attempt.url().scheme() == "http";
                if attempt.previous().len() >= redirects {
                    attempt.error("too many redirects")
                } else if downgrade {
                    attempt.error("HTTPS redirect cannot downgrade to HTTP")
                } else if validate_http_url(&remote, attempt.url(), allow_insecure_http).is_err() {
                    attempt.error("redirect target violates remote acquisition policy")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|source| AcquisitionError::Request {
                action: "build HTTP client",
                remote: "HTTP client".to_string(),
                reason: source.to_string(),
            })
    }
    pub(super) fn validate_plan(&self, plan: &AcquisitionPlan) -> Result<(), AcquisitionError> {
        AcquisitionPlan::build(plan.module_path.clone(), plan.source.clone(), self).and_then(|expected| match expected == *plan {
            | true => Ok(()),
            | false => Err(AcquisitionError::Policy {
                remote: plan.source.to_string(),
                reason: "acquisition plan does not match its source identity or effective policy".to_string(),
            }),
        })
    }
    fn validate_source(&self, source: &AcquisitionSource) -> Result<(), AcquisitionError> {
        match source {
            | AcquisitionSource::Git { commit, url } => rules::commit(commit)
                .map_err(|_| AcquisitionError::Identity {
                    value: commit.clone(),
                    reason: "Git revision must be a full lowercase 40- or 64-hex commit ID",
                })
                .and_then(|()| {
                    match matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
                        | true => Ok(()),
                        | false => Err(AcquisitionError::Identity {
                            value: commit.clone(),
                            reason: "Git revision must be a full lowercase 40- or 64-hex commit ID",
                        }),
                    }
                })
                .and_then(|()| {
                    parse_url(url).and_then(|parsed| match parsed.scheme() {
                        | "https" => validate_url_shape(&parsed, true).and_then(|()| self.ensure_allowed(url)),
                        | "http" if self.allow_insecure_http => validate_url_shape(&parsed, true).and_then(|()| self.ensure_allowed(url)),
                        | "ssh" if self.git_ssh.is_some() => validate_url_shape(&parsed, false).and_then(|()| self.ensure_allowed(url)),
                        | "http" => Err(policy_error(url, "HTTP requires the operator-only insecure HTTP capability")),
                        | "ssh" => Err(policy_error(url, "SSH requires the operator-only Git SSH capability")),
                        | _ => Err(policy_error(
                            url,
                            "Git source must use HTTPS, explicitly allowed HTTP, or explicitly allowed SSH",
                        )),
                    })
                }),
            | AcquisitionSource::Oci { digest, reference } => rules::digest(digest)
                .map_err(|_| AcquisitionError::Identity {
                    value: digest.clone(),
                    reason: "OCI artifact identity must be a sha256 digest",
                })
                .and_then(|()| validate_oci_reference(reference, digest))
                .and_then(|registry| {
                    registry.map_or(Ok(()), |registry| {
                        let scheme = match self.allow_insecure_http {
                            | true => "http",
                            | false => "https",
                        };
                        self.remote
                            .ensure_allowed(&format!("{scheme}://{registry}/"))
                            .map(|_| ())
                            .map_err(|error| policy_error(reference, &error.to_string()))
                    })
                }),
            | AcquisitionSource::HttpBinary { sha256, url } | AcquisitionSource::HttpDocument { sha256, url } => digest(sha256)
                .map_err(|_| AcquisitionError::Identity {
                    value: sha256.clone(),
                    reason: "SHA-256 must use the sha256: prefix followed by 64 lowercase hexadecimal characters",
                })
                .and_then(|()| {
                    parse_url(url).and_then(|parsed| {
                        validate_http_url(&self.remote, &parsed, self.allow_insecure_http).map_err(|source| AcquisitionError::Policy {
                            remote: redact_url(url),
                            reason: source.to_string(),
                        })
                    })
                }),
            | AcquisitionSource::HttpBinaryUnverified { url } => match self.skip_checksum_verification {
                | true => parse_url(url).and_then(|parsed| {
                    validate_http_url(&self.remote, &parsed, self.allow_insecure_http).map_err(|source| AcquisitionError::Policy {
                        remote: redact_url(url),
                        reason: source.to_string(),
                    })
                }),
                | false => Err(policy_error(url, "unverified HTTP requires that checksum-verification be disabled")),
            },
            | AcquisitionSource::Local { .. } => Ok(()),
        }
    }
    fn ensure_allowed(&self, url: &str) -> Result<(), AcquisitionError> {
        self.remote.ensure_allowed(url).map(|_| ()).map_err(|source| AcquisitionError::Policy {
            remote: redact_url(url),
            reason: source.to_string(),
        })
    }
}
impl AcquisitionSource {
    pub(super) fn namespace(&self) -> &'static str {
        match self {
            | Self::Git { .. } => "git",
            | Self::Oci { reference, .. } if reference.starts_with("oci-layout:") => "oci-layout",
            | Self::Oci { .. } => "oci",
            | Self::HttpBinary { .. } | Self::HttpBinaryUnverified { .. } => "binary",
            | Self::HttpDocument { .. } => "document",
            | Self::Local { .. } => "local",
        }
    }
}
impl fmt::Display for AcquisitionSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Oci { reference, .. } => formatter.write_str(reference),
            | Self::Git { url, .. } | Self::HttpBinary { url, .. } | Self::HttpBinaryUnverified { url } | Self::HttpDocument { url, .. } => {
                formatter.write_str(&redact_url(url))
            }
            | Self::Local { path } => path.display().fmt(formatter),
        }
    }
}
impl Validate for GitSshPolicy {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        [(&self.identity, "identity"), (&self.known_hosts, "known_hosts")]
            .into_iter()
            .filter(|(path, _)| !path.is_file())
            .for_each(|(path, name)| {
                report.add(
                    name,
                    ValidationError::new("regular_file").with_message(format!("Path is not a regular file: {}", path.display())),
                );
            });
        report.finish()
    }
}
fn minimum_memory(parent: &Memory, child: &Memory) -> Memory {
    match parent.checked_bytes() <= child.checked_bytes() {
        | true => parent.clone(),
        | false => child.clone(),
    }
}
pub(super) fn normalize_origin(value: &str) -> Result<String, AcquisitionError> {
    parse_url(value).map(|mut url| {
        url.set_fragment(None);
        url.set_query(None);
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&path);
        url.to_string()
    })
}
fn parse_url(value: &str) -> Result<Url, AcquisitionError> {
    Url::parse(value).map_err(|source| AcquisitionError::Policy {
        remote: redact_url(value),
        reason: source.to_string(),
    })
}
fn policy_error(source: &str, reason: &str) -> AcquisitionError {
    AcquisitionError::Policy {
        remote: redact_url(source),
        reason: reason.to_string(),
    }
}
fn validate_http_url(remote: &RemotePolicy, url: &Url, allow_insecure_http: bool) -> Result<(), RemoteError> {
    let scheme_allowed = url.scheme() == "https" || (url.scheme() == "http" && allow_insecure_http);
    match scheme_allowed {
        | false => Err(RemoteError::UnsupportedScheme {
            operation: "HTTP acquisition",
            url: redact_url(url.as_str()),
        }),
        | true => validate_url_shape(url, true)
            .map_err(|error| RemoteError::Denied { url: error.to_string() })
            .and_then(|()| remote.ensure_allowed(url.as_str()).map(|_| ())),
    }
}
fn validate_oci_reference(reference: &str, expected_digest: &str) -> Result<Option<String>, AcquisitionError> {
    match reference.strip_prefix("oci-layout:") {
        | Some(layout) => layout
            .rsplit_once('@')
            .filter(|(path, digest)| !path.is_empty() && *digest == expected_digest && !path.contains(['?', '#']))
            .map(|_| None)
            .ok_or_else(|| AcquisitionError::Identity {
                value: reference.to_string(),
                reason: "OCI layout references must use oci-layout:<path>@sha256:<64 lowercase hex>",
            }),
        | None => validate_oci_registry_reference(reference, expected_digest).map(Some),
    }
}
fn validate_oci_registry_reference(reference: &str, expected_digest: &str) -> Result<String, AcquisitionError> {
    let registry_reference = reference.strip_prefix("oci://").unwrap_or(reference);
    let location = Location::from(registry_reference);
    let invalid = || AcquisitionError::Identity {
        value: reference.to_string(),
        reason: "OCI module references must use oci://registry/repository@sha256:<64 lowercase hex>",
    };
    rules::nonempty(reference)
        .ok()
        .filter(|_| reference.starts_with("oci://") && location.is_oci_reference() && !registry_reference.contains(['?', '#']))
        .and_then(|_| registry_reference.rsplit_once('@'))
        .filter(|(_, digest)| *digest == expected_digest)
        .and_then(|_| registry_reference.split_once('/').map(|(registry, _)| registry))
        .and_then(|registry| Url::parse(&format!("https://{registry}/")).ok())
        .filter(|url| url.username().is_empty() && url.password().is_none() && url.path() == "/" && url.query().is_none() && url.fragment().is_none())
        .and_then(|url| {
            url.host_str().map(|host| match url.port() {
                | Some(port) => format!("{host}:{port}"),
                | None => host.to_string(),
            })
        })
        .ok_or_else(invalid)
}
fn validate_url_shape(url: &Url, reject_username: bool) -> Result<(), AcquisitionError> {
    let username_invalid = reject_username && !url.username().is_empty();
    match (
        username_invalid,
        url.password().is_some(),
        url.query().is_some(),
        url.fragment().is_some(),
    ) {
        | (false, false, false, false) => Ok(()),
        | _ => Err(policy_error(
            url.as_str(),
            "source URLs cannot contain credentials, query parameters, or fragments",
        )),
    }
}
