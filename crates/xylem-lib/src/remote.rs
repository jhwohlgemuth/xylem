//! Remote-module allowlist and diagnostic URL handling
use core::fmt;
use url::Url;

/// Failure while validating a remote module source.
#[derive(Debug)]
pub enum RemoteError {
    /// A configured allowlist entry is invalid.
    AllowedHost { value: String, source: Option<url::ParseError> },
    /// A remote URL is not permitted by the configured allowlist.
    Denied { url: String },
    /// A remote URL is invalid.
    InvalidUrl { value: String, source: url::ParseError },
    /// A remote URL has no host.
    MissingHost { url: String },
    /// The URL scheme cannot be used for the requested operation.
    UnsupportedScheme { operation: &'static str, url: String },
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct AllowedAuthority {
    host: String,
    port: Option<u16>,
}
/// Exact-host policy for remote module URLs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemotePolicy {
    allow_all: bool,
    authorities: Vec<AllowedAuthority>,
}
impl TryFrom<&str> for AllowedAuthority {
    type Error = RemoteError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let candidate = value.trim();
        let invalid = candidate.is_empty()
            || candidate.starts_with(':')
            || candidate.ends_with(':')
            || candidate.contains("://")
            || candidate.contains('/')
            || candidate.contains('@')
            || candidate.contains('?')
            || candidate.contains('#')
            || candidate.contains('*');
        match invalid {
            | true => Err(RemoteError::AllowedHost {
                value: value.to_string(),
                source: None,
            }),
            | false => Url::parse(&format!("http://{candidate}/"))
                .map_err(|source| RemoteError::AllowedHost {
                    value: value.to_string(),
                    source: Some(source),
                })
                .and_then(|parsed| {
                    parsed
                        .host_str()
                        .map(normalize_host)
                        .filter(|host| !host.is_empty())
                        .map(|host| Self { host, port: parsed.port() })
                        .ok_or_else(|| RemoteError::AllowedHost {
                            value: value.to_string(),
                            source: None,
                        })
                }),
        }
    }
}
impl core::error::Error for RemoteError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            | Self::AllowedHost { source, .. } => source.as_ref().map(|source| source as _),
            | Self::InvalidUrl { source, .. } => Some(source),
            | Self::Denied { .. } | Self::MissingHost { .. } | Self::UnsupportedScheme { .. } => None,
        }
    }
}
impl fmt::Display for RemoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::AllowedHost { value, source } => match source {
                | Some(source) => write!(formatter, "invalid allowed host {value:?}; expected HOST or HOST:PORT: {source}"),
                | None => write!(formatter, "invalid allowed host {value:?}; expected HOST or HOST:PORT"),
            },
            | Self::Denied { url } => write!(formatter, "remote module URL is not on the allowlist: {url}"),
            | Self::InvalidUrl { value, source } => write!(formatter, "invalid remote module URL {}: {source}", redact_url(value)),
            | Self::MissingHost { url } => write!(formatter, "remote module URL has no host: {url}"),
            | Self::UnsupportedScheme { operation, url } => write!(formatter, "{operation} requires a supported remote URL: {url}"),
        }
    }
}
impl Default for RemotePolicy {
    fn default() -> Self {
        Self {
            allow_all: true,
            authorities: Vec::new(),
        }
    }
}
impl RemotePolicy {
    /// Construct a deny-by-default policy from required exact authorities.
    pub fn from_required_hosts(hosts: Option<&[String]>) -> Result<Self, RemoteError> {
        hosts
            .unwrap_or_default()
            .iter()
            .map(|value| AllowedAuthority::try_from(value.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map(|authorities| Self {
                allow_all: false,
                authorities,
            })
    }
    /// Construct a policy from `HOST` or `HOST:PORT` entries.
    ///
    /// An absent or empty list preserves the historical allow-all behavior.
    pub fn from_hosts(hosts: Option<&[String]>) -> Result<Self, RemoteError> {
        hosts
            .unwrap_or_default()
            .iter()
            .map(|value| AllowedAuthority::try_from(value.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map(|authorities| Self {
                allow_all: authorities.is_empty(),
                authorities,
            })
    }
    /// Apply a nested workflow allowlist without permitting any authority denied by the parent policy.
    pub fn restrict(&self, hosts: Option<&[String]>) -> Result<Self, RemoteError> {
        Self::from_hosts(hosts).map(|child| match (self.allow_all, child.allow_all) {
            | (true, _) => child,
            | (_, true) => self.clone(),
            | (false, false) => Self {
                allow_all: false,
                authorities: self
                    .authorities
                    .iter()
                    .filter(|authority| child.authorities.contains(authority))
                    .cloned()
                    .collect(),
            },
        })
    }
    /// Intersect a fail-closed nested allowlist with this policy.
    pub fn restrict_required(&self, hosts: Option<&[String]>) -> Result<Self, RemoteError> {
        match hosts {
            | None => Ok(self.clone()),
            | Some(hosts) => Self::from_required_hosts(Some(hosts)).map(|child| Self {
                allow_all: false,
                authorities: self
                    .authorities
                    .iter()
                    .filter(|authority| child.authorities.contains(authority))
                    .cloned()
                    .collect(),
            }),
        }
    }
    /// Validate that a URL is permitted by this policy.
    pub fn ensure_allowed(&self, value: &str) -> Result<Url, RemoteError> {
        Url::parse(value)
            .map_err(|source| RemoteError::InvalidUrl {
                value: value.to_string(),
                source,
            })
            .and_then(|url| match (url.host_str().is_some(), self.allows_url(&url)) {
                | (false, _) => Err(RemoteError::MissingHost { url: redact_url(value) }),
                | (true, false) => Err(RemoteError::Denied { url: redact_url(value) }),
                | (true, true) => Ok(url),
            })
    }
    fn allows_url(&self, url: &Url) -> bool {
        url.host_str().map(normalize_host).is_some_and(|host| {
            self.allow_all
                || self.authorities.iter().any(|allowed| {
                    allowed.host == host
                        && match allowed.port {
                            | Some(port) => url.port_or_known_default() == Some(port),
                            | None => url.port().is_none(),
                        }
                })
        })
    }
}
fn normalize_host(value: &str) -> String {
    value.trim_end_matches('.').to_ascii_lowercase()
}
/// Return a URL suitable for diagnostics without credentials or query secrets.
pub fn redact_url(value: &str) -> String {
    match Url::parse(value) {
        | Ok(url) if url_is_sensitive(&url) => "[REDACTED URL]".to_string(),
        | Ok(url) => url.to_string(),
        | Err(_) => "[INVALID URL]".to_string(),
    }
}
fn url_is_sensitive(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
}
