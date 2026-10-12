//! Remote-module allowlist, reachability, and download support
use acorn_cmd::{args, cmd};
use acorn_core::Scheme;
use core::fmt;
use core::time::Duration;
use reqwest::blocking::{Client, Response};
use reqwest::header::RANGE;
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::thread;
use std::time::Instant;
use url::Url;

/// Maximum time allowed for a remote reachability check.
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(10);
/// Failure while validating, probing, or downloading a remote module.
#[derive(Debug)]
pub enum RemoteError {
    /// A configured allowlist entry is invalid.
    AllowedHost { value: String, source: Option<url::ParseError> },
    /// A remote URL is not permitted by the configured allowlist.
    Denied { url: String },
    /// A Git probe exited unsuccessfully.
    GitStatus { url: String, status: ExitStatus },
    /// A Git probe timed out.
    GitTimeout { url: String },
    /// An HTTP response was unsuccessful.
    HttpStatus { url: String, status: StatusCode },
    /// An I/O operation failed.
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// A remote URL is invalid.
    InvalidUrl { value: String, source: url::ParseError },
    /// A remote URL has no host.
    MissingHost { url: String },
    /// An HTTP client or request failed.
    Request {
        action: &'static str,
        url: Option<String>,
        source: reqwest::Error,
    },
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
            | Self::Io { source, .. } => Some(source),
            | Self::InvalidUrl { source, .. } => Some(source),
            | Self::Request { source, .. } => Some(source),
            | Self::Denied { .. }
            | Self::GitStatus { .. }
            | Self::GitTimeout { .. }
            | Self::HttpStatus { .. }
            | Self::MissingHost { .. }
            | Self::UnsupportedScheme { .. } => None,
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
            | Self::GitStatus { url, status } => write!(formatter, "Git remote is not reachable at {url}: git exited with {status}"),
            | Self::GitTimeout { url } => write!(formatter, "Git reachability check timed out for {url}"),
            | Self::HttpStatus { url, status } => write!(formatter, "remote module is not reachable at {url}: HTTP {status}"),
            | Self::Io { action, path, source } => write!(formatter, "{action} {}: {source}", path.display()),
            | Self::InvalidUrl { value, source } => write!(formatter, "invalid remote module URL {}: {source}", redact_url(value)),
            | Self::MissingHost { url } => write!(formatter, "remote module URL has no host: {url}"),
            | Self::Request { action, url, source } => match url {
                | Some(url) if url == "[REDACTED URL]" => write!(formatter, "{action} {url}"),
                | Some(url) => write!(formatter, "{action} {url}: {source}"),
                | None => write!(formatter, "{action}: {source}"),
            },
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
    fn http_client(&self, request_timeout: Option<Duration>) -> Result<Client, RemoteError> {
        let policy = self.clone();
        let builder = Client::builder().connect_timeout(REMOTE_TIMEOUT).redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if policy.allows_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirect target is not on the remote-module allowlist")
            }
        }));
        match request_timeout {
            | Some(timeout) => builder.timeout(timeout),
            | None => builder,
        }
        .build()
        .map_err(|source| RemoteError::Request {
            action: "build HTTP client",
            url: None,
            source,
        })
    }
    /// Check that an HTTP(S) resource exists without downloading its body.
    pub fn probe_http(&self, value: &str) -> Result<(), RemoteError> {
        self.ensure_allowed(value).and_then(|url| match Scheme::from(url.scheme()) {
            | Scheme::HTTP | Scheme::HTTPS => self.http_client(Some(REMOTE_TIMEOUT)).and_then(|client| {
                client
                    .head(url.clone())
                    .send()
                    .map_err(|why| request_failure("Remote module is not reachable at", value, why))
                    .and_then(|response| match response.status() {
                        | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED => client
                            .get(url)
                            .header(RANGE, "bytes=0-0")
                            .send()
                            .map_err(|why| request_failure("Remote module is not reachable at", value, why)),
                        | _ => Ok(response),
                    })
                    .and_then(|response| require_success(response, value))
            }),
            | Scheme::File | Scheme::SSH | Scheme::Unsupported => Err(RemoteError::UnsupportedScheme {
                operation: "HTTP reachability check",
                url: redact_url(value),
            }),
        })
    }
    /// Check that a Git remote can be queried non-interactively.
    pub fn probe_git(&self, value: &str) -> Result<(), RemoteError> {
        self.ensure_allowed(value)
            .and_then(|url| match (Scheme::from(url.scheme()), url.scheme().eq_ignore_ascii_case("ssh")) {
                | (Scheme::HTTP | Scheme::HTTPS | Scheme::SSH, _) | (Scheme::Unsupported, true) => {
                    probe_git_with_command(Path::new("git"), value, REMOTE_TIMEOUT)
                }
                | (Scheme::File, _) | (Scheme::Unsupported, false) => Err(RemoteError::UnsupportedScheme {
                    operation: "Git reachability check",
                    url: redact_url(value),
                }),
            })
    }
    /// Download an allowed HTTP(S) resource while enforcing redirect policy.
    pub fn download(&self, value: &str, destination: &Path) -> Result<PathBuf, RemoteError> {
        self.ensure_allowed(value).and_then(|url| match Scheme::from(url.scheme()) {
            | Scheme::HTTP | Scheme::HTTPS => {
                let filename = url
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .filter(|segment| !segment.is_empty())
                    .unwrap_or("downloaded_file");
                let output = destination.join(filename);
                let partial = output.with_file_name(format!("{filename}.part"));
                self.http_client(None)
                    .and_then(|client| client.get(url).send().map_err(|why| request_failure("Failed to download", value, why)))
                    .and_then(|response| {
                        response
                            .error_for_status()
                            .map_err(|why| request_failure("Failed to download", value, why))
                    })
                    .and_then(|mut response| {
                        File::create(&partial)
                            .map_err(|source| RemoteError::Io {
                                action: "create partial download",
                                path: partial.clone(),
                                source,
                            })
                            .and_then(|mut file| match response.copy_to(&mut file) {
                                | Ok(_) => {
                                    drop(file);
                                    fs::rename(&partial, &output).map(|_| output.clone()).map_err(|source| {
                                        let _ = fs::remove_file(&partial);
                                        RemoteError::Io {
                                            action: "finalize download",
                                            path: output.clone(),
                                            source,
                                        }
                                    })
                                }
                                | Err(source) => {
                                    drop(file);
                                    let _ = fs::remove_file(&partial);
                                    Err(RemoteError::Request {
                                        action: "write download",
                                        url: Some(redact_url(value)),
                                        source,
                                    })
                                }
                            })
                    })
            }
            | Scheme::File | Scheme::SSH | Scheme::Unsupported => Err(RemoteError::UnsupportedScheme {
                operation: "remote binary download",
                url: redact_url(value),
            }),
        })
    }
    /// Read an allowed HTTP(S) configuration document into memory.
    pub fn read_text(&self, value: &str) -> Result<String, RemoteError> {
        self.ensure_allowed(value).and_then(|url| match Scheme::from(url.scheme()) {
            | Scheme::HTTP | Scheme::HTTPS => self
                .http_client(Some(REMOTE_TIMEOUT))
                .and_then(|client| {
                    client
                        .get(url)
                        .send()
                        .map_err(|why| request_failure("read remote configuration", value, why))
                })
                .and_then(|response| {
                    response
                        .error_for_status()
                        .map_err(|why| request_failure("read remote configuration", value, why))
                })
                .and_then(|response| response.text().map_err(|why| request_failure("read remote configuration", value, why))),
            | Scheme::File | Scheme::SSH | Scheme::Unsupported => Err(RemoteError::UnsupportedScheme {
                operation: "read remote configuration",
                url: redact_url(value),
            }),
        })
    }
}
fn normalize_host(value: &str) -> String {
    value.trim_end_matches('.').to_ascii_lowercase()
}
pub(crate) fn probe_git_with_command(command: &Path, value: &str, timeout: Duration) -> Result<(), RemoteError> {
    let arguments = args!["-c", "http.followRedirects=false", "ls-remote", value];
    cmd!(spawn command, arguments)
        .map_err(|source| RemoteError::Io {
            action: "start Git reachability check",
            path: command.to_path_buf(),
            source,
        })
        .and_then(|mut child| {
            let started = Instant::now();
            loop {
                match child.try_wait() {
                    | Ok(Some(status)) if status.success() => break Ok(()),
                    | Ok(Some(status)) => {
                        break Err(RemoteError::GitStatus {
                            url: redact_url(value),
                            status,
                        });
                    }
                    | Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
                    | Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(RemoteError::GitTimeout { url: redact_url(value) });
                    }
                    | Err(source) => {
                        break Err(RemoteError::Io {
                            action: "observe Git reachability check",
                            path: command.to_path_buf(),
                            source,
                        });
                    }
                }
            }
        })
}
/// Return a URL suitable for diagnostics without credentials or query secrets.
pub fn redact_url(value: &str) -> String {
    match Url::parse(value) {
        | Ok(url) if url_is_sensitive(&url) => "[REDACTED URL]".to_string(),
        | Ok(url) => url.to_string(),
        | Err(_) => "[INVALID URL]".to_string(),
    }
}
fn request_failure(action: &'static str, value: &str, source: reqwest::Error) -> RemoteError {
    RemoteError::Request {
        action,
        url: Some(redact_url(value)),
        source,
    }
}
fn require_success(response: Response, value: &str) -> Result<(), RemoteError> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(RemoteError::HttpStatus {
            url: redact_url(value),
            status: response.status(),
        })
    }
}
fn url_is_sensitive(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
}
