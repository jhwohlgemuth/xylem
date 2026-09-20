//! Remote-module allowlist, reachability, and download support
use acorn::{args, cmd};
use acorn_core::Scheme;
use color_eyre::eyre::{eyre, Report, Result};
use core::fmt::Display;
use core::time::Duration;
use reqwest::blocking::{Client, Response};
use reqwest::header::RANGE;
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;
use url::Url;

/// Maximum time allowed for a remote reachability check.
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(10);
#[derive(Clone, Debug, Eq, PartialEq)]
struct AllowedAuthority {
    host: String,
    port: Option<u16>,
}
/// Exact-host policy for remote module URLs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RemotePolicy {
    authorities: Vec<AllowedAuthority>,
}
impl RemotePolicy {
    /// Construct a policy from `HOST` or `HOST:PORT` entries.
    ///
    /// An absent or empty list preserves the historical allow-all behavior.
    pub fn from_hosts(hosts: Option<&[String]>) -> Result<Self> {
        hosts
            .unwrap_or_default()
            .iter()
            .map(|value| AllowedAuthority::try_from(value.as_str()))
            .collect::<Result<Vec<_>>>()
            .map(|authorities| Self { authorities })
    }
    /// Validate that a URL is permitted by this policy.
    pub fn ensure_allowed(&self, value: &str) -> Result<Url> {
        Url::parse(value)
            .map_err(|why| eyre!("Invalid remote module URL {} — {why}", redact_url(value)))
            .and_then(|url| match (url.host_str().is_some(), self.allows_url(&url)) {
                | (false, _) => Err(eyre!("Remote module URL has no host: {}", redact_url(value))),
                | (true, false) => Err(eyre!("Remote module URL is not on the allowlist: {}", redact_url(value))),
                | (true, true) => Ok(url),
            })
    }
    fn allows_url(&self, url: &Url) -> bool {
        url.host_str().map(normalize_host).is_some_and(|host| {
            self.authorities.is_empty()
                || self.authorities.iter().any(|allowed| {
                    allowed.host == host
                        && match allowed.port {
                            | Some(port) => url.port_or_known_default() == Some(port),
                            | None => url.port().is_none(),
                        }
                })
        })
    }
    fn http_client(&self, request_timeout: Option<Duration>) -> Result<Client> {
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
        .map_err(Report::from)
    }
    /// Check that an HTTP(S) resource exists without downloading its body.
    pub fn probe_http(&self, value: &str) -> Result<()> {
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
            | Scheme::File | Scheme::Unsupported => Err(eyre!("HTTP reachability checks require an HTTP(S) URL: {}", redact_url(value))),
        })
    }
    /// Check that a Git remote can be queried non-interactively.
    pub fn probe_git(&self, value: &str) -> Result<()> {
        self.ensure_allowed(value)
            .and_then(|url| match (Scheme::from(url.scheme()), url.scheme().eq_ignore_ascii_case("ssh")) {
                | (Scheme::HTTP | Scheme::HTTPS, _) | (Scheme::Unsupported, true) => probe_git_with_command(Path::new("git"), value, REMOTE_TIMEOUT),
                | (Scheme::File, _) | (Scheme::Unsupported, false) => {
                    Err(eyre!("Git reachability checks require an HTTP(S) or SSH URL: {}", redact_url(value)))
                }
            })
    }
    /// Download an allowed HTTP(S) resource while enforcing redirect policy.
    pub fn download(&self, value: &str, destination: &Path) -> Result<PathBuf> {
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
                            .map_err(|why| eyre!("Failed to create {} — {why}", partial.display()))
                            .and_then(|mut file| match response.copy_to(&mut file) {
                                | Ok(_) => {
                                    drop(file);
                                    fs::rename(&partial, &output).map(|_| output.clone()).map_err(|why| {
                                        let _ = fs::remove_file(&partial);
                                        eyre!("Failed to finalize {} — {why}", output.display())
                                    })
                                }
                                | Err(why) => {
                                    drop(file);
                                    let _ = fs::remove_file(&partial);
                                    Err(eyre!("Failed to write {} — {why}", output.display()))
                                }
                            })
                    })
            }
            | Scheme::File | Scheme::Unsupported => Err(eyre!("Remote binary downloads require an HTTP(S) URL: {}", redact_url(value))),
        })
    }
}
fn normalize_host(value: &str) -> String {
    value.trim_end_matches('.').to_ascii_lowercase()
}
impl TryFrom<&str> for AllowedAuthority {
    type Error = Report;

    fn try_from(value: &str) -> Result<Self> {
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
            | true => Err(eyre!("Invalid allowed host {value:?}; expected HOST or HOST:PORT")),
            | false => Url::parse(&format!("http://{candidate}/"))
                .map_err(|why| eyre!("Invalid allowed host {value:?} — {why}"))
                .and_then(|parsed| {
                    parsed
                        .host_str()
                        .map(normalize_host)
                        .filter(|host| !host.is_empty())
                        .map(|host| Self { host, port: parsed.port() })
                        .ok_or_else(|| eyre!("Invalid allowed host {value:?}"))
                }),
        }
    }
}
fn require_success(response: Response, value: &str) -> Result<()> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(eyre!(
            "Remote module is not reachable at {} — HTTP {}",
            redact_url(value),
            response.status()
        ))
    }
}
pub(crate) fn probe_git_with_command(command: &Path, value: &str, timeout: Duration) -> Result<()> {
    let arguments = args!["-c", "http.followRedirects=false", "ls-remote", value];
    cmd!(spawn command, arguments)
        .map_err(|why| eyre!("Failed to start Git reachability check — {why}"))
        .and_then(|mut child| {
            let started = Instant::now();
            loop {
                match child.try_wait() {
                    | Ok(Some(status)) if status.success() => break Ok(()),
                    | Ok(Some(status)) => {
                        break Err(eyre!("Git remote is not reachable at {} — git exited with {status}", redact_url(value)));
                    }
                    | Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
                    | Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(eyre!("Git reachability check timed out for {}", redact_url(value)));
                    }
                    | Err(why) => break Err(eyre!("Git reachability check failed for {} — {why}", redact_url(value))),
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
fn url_is_sensitive(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
}
fn request_failure(action: &str, value: &str, why: impl Display) -> Report {
    match Url::parse(value).is_ok_and(|url| url_is_sensitive(&url)) {
        | true => eyre!("{action} [REDACTED URL]"),
        | false => eyre!("{action} {} — {why}", redact_url(value)),
    }
}
