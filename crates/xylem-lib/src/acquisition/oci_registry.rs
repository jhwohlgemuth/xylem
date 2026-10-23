use super::util::{io_error, sha256};
use super::*;
use acorn_core::validation::digest as validate_sha256_digest;
use alloc::collections::BTreeSet;
use base64::{engine::general_purpose::STANDARD, Engine};
use core::iter::once;
use reqwest::blocking::Response;
use reqwest::header::{ACCEPT, AUTHORIZATION, LOCATION, WWW_AUTHENTICATE};
use serde::Deserialize;
use serde_json::json;
use std::fs::write;
use std::io::ErrorKind;
use std::process::{Command, Stdio};

enum Credentials {
    Basic { username: String, secret: String },
    IdentityToken(String),
}
#[derive(Deserialize)]
struct Descriptor {
    #[serde(default)]
    data: Option<String>,
    digest: String,
    #[serde(default, rename = "mediaType")]
    media_type: String,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
    size: u64,
}
pub(super) struct Layout<'a> {
    root: &'a Path,
}
#[derive(Deserialize)]
struct Manifest {
    config: Descriptor,
    layers: Vec<Descriptor>,
    #[serde(rename = "mediaType")]
    media_type: String,
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
}
struct RegistryClient<'a> {
    client: Client,
    credentials: Option<Credentials>,
    policy: &'a AcquisitionPolicy,
    registry: Url,
    started: Instant,
    token: Option<String>,
}
impl Credentials {
    fn load(registry: &str, started: Instant, timeout: Seconds) -> Result<Option<Self>, AcquisitionError> {
        let directory = env::var_os("DOCKER_CONFIG")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker")));
        directory.map(|directory| directory.join("config.json")).map_or(Ok(None), |path| {
            let content = match read(&path) {
                | Ok(content) => Ok(Some(content)),
                | Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
                | Err(source) => Err(io_error("read OCI registry credentials", path, source)),
            };
            content.and_then(|content| {
                content.map_or(Ok(None), |content| {
                    serde_json::from_slice::<serde_json::Value>(&content)
                        .map_err(|_| invalid_manifest("invalid Docker credential configuration"))
                        .and_then(|config| Self::from_config(&config, registry, started, timeout))
                })
            })
        })
    }
    fn from_config(config: &serde_json::Value, registry: &str, started: Instant, timeout: Seconds) -> Result<Option<Credentials>, AcquisitionError> {
        let helper = config
            .get("credHelpers")
            .and_then(|helpers| helpers.get(registry))
            .or_else(|| config.get("credsStore"))
            .and_then(serde_json::Value::as_str);
        match helper {
            | Some(helper) => Self::from_helper(helper, registry, started, timeout).map(Some),
            | None => {
                let entry = config
                    .get("auths")
                    .and_then(|auths| auths.get(registry).or_else(|| auths.get(format!("https://{registry}").as_str())));
                match entry.and_then(|entry| entry.get("identitytoken")).and_then(serde_json::Value::as_str) {
                    | Some(token) => Ok(Some(Credentials::IdentityToken(token.to_string()))),
                    | None => entry
                        .and_then(|entry| entry.get("auth"))
                        .and_then(serde_json::Value::as_str)
                        .map_or(Ok(None), |encoded| {
                            STANDARD
                                .decode(encoded)
                                .ok()
                                .and_then(|bytes| String::from_utf8(bytes).ok())
                                .and_then(|decoded| {
                                    decoded.split_once(':').map(|(username, secret)| Credentials::Basic {
                                        username: username.to_string(),
                                        secret: secret.to_string(),
                                    })
                                })
                                .map(Some)
                                .ok_or_else(|| invalid_manifest("invalid Docker registry auth value"))
                        }),
                }
            }
        }
    }
    fn from_helper(helper: &str, registry: &str, started: Instant, timeout: Seconds) -> Result<Self, AcquisitionError> {
        let valid = !helper.is_empty() && helper.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid
            .then_some(())
            .ok_or_else(|| invalid_manifest("invalid Docker credential helper name"))
            .and_then(|()| {
                Command::new(format!("docker-credential-{helper}"))
                    .arg("get")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|_| invalid_manifest("Docker credential helper could not start"))
            })
            .and_then(|mut child| {
                let input = child
                    .stdin
                    .take()
                    .ok_or_else(|| invalid_manifest("Docker credential helper has no stdin"))
                    .and_then(|mut stdin| {
                        stdin
                            .write_all(registry.as_bytes())
                            .map_err(|_| invalid_manifest("Docker credential helper input failed"))
                    });
                let output = input.and_then(|()| {
                    child
                        .stdout
                        .take()
                        .ok_or_else(|| invalid_manifest("Docker credential helper has no stdout"))
                });
                match output {
                    | Ok(stdout) => {
                        let reader = thread::spawn(move || {
                            let mut bytes = Vec::new();
                            stdout.take(16 * 1024 + 1).read_to_end(&mut bytes).map(|_| bytes)
                        });
                        let deadline = Duration::from(timeout)
                            .checked_sub(started.elapsed())
                            .unwrap_or_default()
                            .min(Duration::from_secs(10));
                        let helper_started = Instant::now();
                        let status = loop {
                            match child.try_wait() {
                                | Ok(Some(status)) => break Ok(status),
                                | Ok(None) if helper_started.elapsed() >= deadline => break Err(()),
                                | Ok(None) => thread::sleep(MONITOR_INTERVAL),
                                | Err(_) => break Err(()),
                            }
                        };
                        if status.is_err() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                        reader
                            .join()
                            .ok()
                            .and_then(Result::ok)
                            .ok_or_else(|| invalid_manifest("Docker credential helper output failed"))
                            .and_then(|bytes| {
                                if !status.is_ok_and(|status| status.success()) || bytes.len() > 16 * 1024 {
                                    Err(invalid_manifest("Docker credential helper failed or exceeded its output limit"))
                                } else {
                                    serde_json::from_slice::<serde_json::Value>(&bytes)
                                        .map_err(|_| invalid_manifest("invalid Docker credential helper output"))
                                }
                            })
                            .and_then(|value| {
                                value
                                    .get("Username")
                                    .and_then(serde_json::Value::as_str)
                                    .zip(value.get("Secret").and_then(serde_json::Value::as_str))
                                    .map(|(username, secret)| match username {
                                        | "<token>" => Credentials::IdentityToken(secret.to_string()),
                                        | _ => Credentials::Basic {
                                            username: username.to_string(),
                                            secret: secret.to_string(),
                                        },
                                    })
                                    .ok_or_else(|| invalid_manifest("Docker credential helper returned no credentials"))
                            })
                    }
                    | Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        Err(error)
                    }
                }
            })
    }
    fn authorization(&self) -> String {
        match self {
            | Self::Basic { username, secret } => format!("Basic {}", STANDARD.encode(format!("{username}:{secret}"))),
            | Self::IdentityToken(token) => format!("Bearer {token}"),
        }
    }
    fn basic_header(&self) -> Option<String> {
        match self {
            | Self::Basic { .. } => Some(self.authorization()),
            | Self::IdentityToken(_) => None,
        }
    }
}
impl Layout<'_> {
    fn blob_path(&self, digest: &str) -> Result<PathBuf, AcquisitionError> {
        validate_digest(digest).map(|name| self.root.join("blobs/sha256").join(name))
    }
    fn write_manifest(&self, digest: &str, manifest_bytes: &[u8], manifest: &Manifest) -> Result<(), AcquisitionError> {
        let index = json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [{
                "mediaType": manifest.media_type,
                "digest": digest,
                "size": manifest_bytes.len(),
            }],
        });
        create_dir_all(self.root.join("blobs/sha256"))
            .map_err(|source| io_error("create OCI layout", self.root, source))
            .and_then(|()| {
                write(self.root.join("oci-layout"), b"{\"imageLayoutVersion\":\"1.0.0\"}\n")
                    .map_err(|source| io_error("write OCI layout marker", self.root, source))
            })
            .and_then(|()| {
                write(self.root.join("index.json"), index.to_string()).map_err(|source| io_error("write OCI layout index", self.root, source))
            })
            .and_then(|()| self.write_verified_blob(digest, manifest_bytes))
    }
    fn write_verified_blob(&self, digest: &str, bytes: &[u8]) -> Result<(), AcquisitionError> {
        self.blob_path(digest).and_then(|path| {
            File::create(&path)
                .map_err(|source| io_error("stage verified OCI blob", &path, source))
                .and_then(|mut file| {
                    file.write_all(bytes)
                        .map_err(|source| io_error("write verified OCI blob", &path, source))
                        .and_then(|()| file.sync_all().map_err(|source| io_error("sync verified OCI blob", &path, source)))
                })
        })
    }
}
impl<'a> Layout<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self { root }
    }
    pub(super) fn fetch(&self, reference: &str, digest: &str, policy: &AcquisitionPolicy, started: Instant) -> Result<(), AcquisitionError> {
        reference
            .strip_prefix("oci://")
            .and_then(|value| value.rsplit_once('@'))
            .and_then(|(name, _)| name.split_once('/'))
            .ok_or_else(|| invalid_manifest("invalid OCI registry reference"))
            .and_then(|(registry, repository)| {
                let scheme = if policy.allow_insecure_http { "http" } else { "https" };
                Url::parse(&format!("{scheme}://{registry}/"))
                    .map_err(|_| invalid_manifest("invalid OCI registry host"))
                    .and_then(|origin| {
                        policy
                            .validate_oci_request(&origin, None)
                            .and_then(|()| {
                                Client::builder()
                                    .connect_timeout(Duration::from(policy.limits.connect_timeout))
                                    .no_proxy()
                                    .redirect(Policy::none())
                                    .build()
                                    .map_err(|_| request_error("create OCI client", &origin, "HTTP client initialization failed"))
                            })
                            .and_then(|client| {
                                Credentials::load(registry, started, policy.limits.git_timeout).map(|credentials| RegistryClient {
                                    client,
                                    credentials,
                                    policy,
                                    registry: origin.clone(),
                                    started,
                                    token: None,
                                })
                            })
                            .and_then(|mut transport| {
                                origin
                                    .join(&format!("v2/{repository}/manifests/{digest}"))
                                    .map_err(|_| invalid_manifest("invalid OCI repository path"))
                                    .and_then(|url| transport.get_bytes(url, policy.limits.process_output.checked_bytes().unwrap_or_default()))
                                    .and_then(|bytes| verify_digest(&bytes, digest, reference).map(|()| bytes))
                                    .and_then(|bytes| {
                                        serde_json::from_slice::<Manifest>(&bytes)
                                            .map_err(|_| invalid_manifest("invalid OCI manifest JSON"))
                                            .and_then(|manifest| validate_manifest(&manifest, policy.limits()).map(|()| (bytes, manifest)))
                                    })
                                    .and_then(|(bytes, manifest)| {
                                        self.write_manifest(digest, &bytes, &manifest).and_then(|()| {
                                            manifest.layers.iter().chain(once(&manifest.config)).try_for_each(|descriptor| {
                                                origin
                                                    .join(&format!("v2/{repository}/blobs/{}", descriptor.digest))
                                                    .map_err(|_| invalid_manifest("invalid OCI blob path"))
                                                    .and_then(|url| match descriptor.data.as_ref() {
                                                        | Some(data) => STANDARD
                                                            .decode(data)
                                                            .map_err(|_| invalid_manifest("invalid inline OCI blob"))
                                                            .and_then(|bytes| {
                                                                (bytes.len() as u64 == descriptor.size)
                                                                    .then_some(())
                                                                    .ok_or_else(|| invalid_manifest("OCI blob size does not match its descriptor"))
                                                                    .and_then(|()| verify_digest(&bytes, &descriptor.digest, reference))
                                                                    .and_then(|()| self.write_verified_blob(&descriptor.digest, &bytes))
                                                            }),
                                                        | None => transport.write_blob(url, self, descriptor, reference),
                                                    })
                                            })
                                        })
                                    })
                            })
                    })
            })
    }
}
impl RegistryClient<'_> {
    fn write_blob(&mut self, url: Url, layout: &Layout<'_>, descriptor: &Descriptor, reference: &str) -> Result<(), AcquisitionError> {
        layout.blob_path(&descriptor.digest).and_then(|path| {
            File::create(&path)
                .map_err(|source| io_error("stage OCI blob", &path, source))
                .and_then(|mut file| {
                    self.get(url.clone()).and_then(|mut response| {
                        let mut hasher = Sha256::new();
                        let mut observed = 0_u64;
                        let mut chunk = [0_u8; 16 * 1024];
                        let streamed = loop {
                            match response.read(&mut chunk) {
                                | Ok(0) => break Ok(()),
                                | Ok(count) => {
                                    let written = observed
                                        .checked_add(count as u64)
                                        .ok_or_else(|| invalid_manifest("OCI blob size overflow"))
                                        .and_then(|next| {
                                            (next <= descriptor.size).then_some(next).ok_or(AcquisitionError::Limit {
                                                limit: "OCI blob bytes",
                                                maximum: descriptor.size,
                                                observed: next,
                                            })
                                        })
                                        .and_then(|next| {
                                            file.write_all(chunk.get(..count).unwrap_or_default())
                                                .map_err(|source| io_error("write OCI blob", &path, source))
                                                .map(|()| next)
                                        });
                                    match written {
                                        | Ok(next) => {
                                            observed = next;
                                            hasher.update(chunk.get(..count).unwrap_or_default());
                                        }
                                        | Err(error) => break Err(error),
                                    }
                                }
                                | Err(_) => break Err(request_error("read OCI blob", &url, "response read failed")),
                            }
                        };
                        streamed
                            .and_then(|()| {
                                (observed == descriptor.size)
                                    .then_some(())
                                    .ok_or_else(|| invalid_manifest("OCI blob size does not match its descriptor"))
                            })
                            .and_then(|()| {
                                let calculated = format!("sha256:{:x}", hasher.finalize());
                                (calculated == descriptor.digest).then_some(()).ok_or(AcquisitionError::Sha256Mismatch {
                                    calculated,
                                    expected: descriptor.digest.clone(),
                                    remote: reference.to_string(),
                                })
                            })
                            .and_then(|()| file.sync_all().map_err(|source| io_error("sync OCI blob", &path, source)))
                    })
                })
        })
    }
    fn get_bytes(&mut self, url: Url, maximum: u64) -> Result<Vec<u8>, AcquisitionError> {
        self.get(url.clone()).and_then(|mut response| {
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 16 * 1024];
            loop {
                match response.read(&mut chunk) {
                    | Ok(0) => break Ok(bytes),
                    | Ok(count) => {
                        let observed = (bytes.len() as u64).saturating_add(count as u64);
                        if observed > maximum {
                            break Err(AcquisitionError::Limit {
                                limit: "OCI response bytes",
                                maximum,
                                observed,
                            });
                        }
                        bytes.extend_from_slice(chunk.get(..count).unwrap_or_default());
                    }
                    | Err(_) => break Err(request_error("read OCI response", &url, "response read failed")),
                }
            }
        })
    }
    fn get(&mut self, url: Url) -> Result<Response, AcquisitionError> {
        let authorization = self
            .token
            .as_ref()
            .map(|token| format!("Bearer {token}"))
            .or_else(|| self.credentials.as_ref().map(Credentials::authorization));
        self.send(url.clone(), authorization.as_deref(), "application/vnd.oci.image.manifest.v1+json")
            .and_then(|first| {
                if first.status().as_u16() == 401 {
                    first
                        .headers()
                        .get(WWW_AUTHENTICATE)
                        .and_then(|value| value.to_str().ok())
                        .ok_or_else(|| request_error("authorize OCI request", &url, "registry gave no authentication challenge"))
                        .and_then(|challenge| self.bearer_token(challenge))
                        .and_then(|token| {
                            self.token = Some(token);
                            let authorization = self.token.as_ref().map(|token| format!("Bearer {token}"));
                            self.send(url.clone(), authorization.as_deref(), "application/vnd.oci.image.manifest.v1+json")
                        })
                        .and_then(|retry| accepted(retry, &url))
                } else {
                    accepted(first, &url)
                }
            })
    }
    fn bearer_token(&self, challenge: &str) -> Result<String, AcquisitionError> {
        challenge
            .strip_prefix("Bearer ")
            .ok_or_else(|| invalid_manifest("unsupported OCI authentication challenge"))
            .and_then(|challenge| {
                let fields = challenge
                    .split(',')
                    .filter_map(|part| part.trim().split_once('='))
                    .map(|(name, value)| (name.trim(), value.trim().trim_matches('"')))
                    .collect::<BTreeMap<_, _>>();
                fields
                    .get("realm")
                    .ok_or_else(|| invalid_manifest("OCI bearer challenge has no realm"))
                    .and_then(|realm| Url::parse(realm).map_err(|_| invalid_manifest("OCI bearer realm is invalid")))
                    .and_then(|mut url| {
                        self.policy.validate_oci_request(&url, Some(&self.registry)).map(|()| {
                            let mut query = url.query_pairs_mut();
                            for field in ["service", "scope"] {
                                if let Some(value) = fields.get(field) {
                                    query.append_pair(field, value);
                                }
                            }
                            drop(query);
                            url
                        })
                    })
                    .and_then(|url| {
                        let basic = self.credentials.as_ref().and_then(Credentials::basic_header);
                        self.send(url.clone(), basic.as_deref(), "application/json")
                            .and_then(|response| accepted(response, &url))
                            .and_then(|response| {
                                let mut bytes = Vec::new();
                                response
                                    .take(16 * 1024 + 1)
                                    .read_to_end(&mut bytes)
                                    .map_err(|_| request_error("read OCI token", &url, "token response read failed"))
                                    .and_then(|_| {
                                        (bytes.len() <= 16 * 1024)
                                            .then_some(bytes)
                                            .ok_or_else(|| invalid_manifest("OCI token response is too large"))
                                    })
                            })
                            .and_then(|bytes| {
                                serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|_| invalid_manifest("invalid OCI token response"))
                            })
                            .and_then(|value| {
                                value
                                    .get("token")
                                    .or_else(|| value.get("access_token"))
                                    .and_then(serde_json::Value::as_str)
                                    .filter(|token| !token.is_empty())
                                    .map(str::to_string)
                                    .ok_or_else(|| invalid_manifest("OCI token response has no token"))
                            })
                    })
            })
    }
    fn send(&self, url: Url, authorization: Option<&str>, accept: &str) -> Result<Response, AcquisitionError> {
        let mut current = url.clone();
        let mut previous = None;
        let mut redirects = 0;
        loop {
            let response = self.policy.validate_oci_request(&current, previous.as_ref()).and_then(|()| {
                Duration::from(self.policy.limits.git_timeout)
                    .checked_sub(self.started.elapsed())
                    .filter(|duration| !duration.is_zero())
                    .ok_or(AcquisitionError::Limit {
                        limit: "OCI acquisition seconds",
                        maximum: self.policy.limits.git_timeout.get(),
                        observed: self.started.elapsed().as_secs(),
                    })
                    .and_then(|timeout| {
                        let request = self.client.get(current.clone()).timeout(timeout).header(ACCEPT, accept);
                        let request = match (url.origin() == current.origin(), authorization) {
                            | (true, Some(value)) => request.header(AUTHORIZATION, value),
                            | _ => request,
                        };
                        request
                            .send()
                            .map_err(|_| request_error("request OCI resource", &current, "request failed"))
                    })
            });
            match response {
                | Ok(response) if !response.status().is_redirection() => break Ok(response),
                | Ok(_) if redirects == self.policy.limits.redirects => {
                    break Err(request_error("follow OCI redirect", &current, "too many redirects"))
                }
                | Ok(response) => {
                    let next = response
                        .headers()
                        .get(LOCATION)
                        .and_then(|value| value.to_str().ok())
                        .ok_or_else(|| request_error("follow OCI redirect", &current, "redirect has no valid Location"))
                        .and_then(|location| {
                            current
                                .join(location)
                                .map_err(|_| request_error("follow OCI redirect", &current, "invalid redirect URL"))
                        })
                        .and_then(|next| self.policy.validate_oci_request(&next, Some(&current)).map(|()| next));
                    match next {
                        | Ok(next) => {
                            previous = Some(current);
                            current = next;
                            redirects = redirects.saturating_add(1);
                        }
                        | Err(error) => break Err(error),
                    }
                }
                | Err(error) => break Err(error),
            }
        }
    }
}
fn accepted(response: Response, url: &Url) -> Result<Response, AcquisitionError> {
    match response.status().is_success() {
        | true => Ok(response),
        | false => Err(request_error(
            "request OCI resource",
            url,
            &format!("HTTP {}", response.status().as_u16()),
        )),
    }
}
fn invalid_manifest(reason: &'static str) -> AcquisitionError {
    AcquisitionError::Identity {
        value: "OCI artifact".to_string(),
        reason,
    }
}
fn request_error(action: &'static str, url: &Url, reason: &str) -> AcquisitionError {
    AcquisitionError::Request {
        action,
        remote: redact_url(url.as_str()),
        reason: reason.to_string(),
    }
}
fn validate_digest(digest: &str) -> Result<&str, AcquisitionError> {
    validate_sha256_digest(digest)
        .map_err(|_| invalid_manifest("OCI descriptor has an invalid SHA-256 digest"))
        .and_then(|()| {
            digest
                .strip_prefix("sha256:")
                .ok_or_else(|| invalid_manifest("OCI descriptor has an invalid SHA-256 digest"))
        })
}
fn validate_manifest(manifest: &Manifest, limits: &AcquisitionLimits) -> Result<(), AcquisitionError> {
    let valid = manifest.schema_version == 2 && manifest.media_type == "application/vnd.oci.image.manifest.v1+json" && !manifest.layers.is_empty();
    let entries = u64::try_from(manifest.layers.len()).unwrap_or(u64::MAX);
    let mut paths = BTreeSet::<String>::new();
    let maximum = limits.git_staging.checked_bytes().unwrap_or_default();
    valid
        .then_some(())
        .ok_or_else(|| invalid_manifest("OCI artifact must be a version 2 image manifest with file layers"))
        .and_then(|()| {
            (entries <= limits.git_entries).then_some(()).ok_or(AcquisitionError::Limit {
                limit: "OCI artifact layers",
                maximum: limits.git_entries,
                observed: entries,
            })
        })
        .and_then(|()| {
            manifest.layers.iter().chain(once(&manifest.config)).try_fold(0_u64, |total, descriptor| {
                validate_digest(&descriptor.digest).and_then(|_| {
                    total
                        .checked_add(descriptor.size)
                        .ok_or_else(|| invalid_manifest("OCI descriptor sizes overflow"))
                })
            })
        })
        .and_then(|total| {
            (total <= maximum).then_some(()).ok_or(AcquisitionError::Limit {
                limit: "OCI artifact bytes",
                maximum,
                observed: total,
            })
        })
        .and_then(|()| {
            manifest.layers.iter().try_for_each(|layer| {
                layer
                    .annotations
                    .get("org.opencontainers.image.title")
                    .ok_or_else(|| invalid_manifest("OCI file layer has no title"))
                    .and_then(|path| {
                        let normalized = path.to_ascii_lowercase();
                        let prefix = format!("{normalized}/");
                        let ancestor = normalized.match_indices('/').any(|(index, _)| paths.contains(&normalized[..index]));
                        let descendant = paths.range(prefix.clone()..).next().is_some_and(|existing| existing.starts_with(&prefix));
                        let safe = layer.media_type == "application/vnd.oci.image.layer.v1.tar"
                            && !path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
                            && !path.starts_with('/')
                            && !path.contains(['\\', ':', '\0'])
                            && !ancestor
                            && !descendant
                            && paths.insert(normalized);
                        safe.then_some(())
                            .ok_or_else(|| invalid_manifest("OCI file layer has an unsupported format or unsafe path"))
                    })
            })
        })
}
pub(super) fn validate_manifest_bytes(bytes: &[u8], limits: &AcquisitionLimits) -> Result<(), AcquisitionError> {
    serde_json::from_slice::<Manifest>(bytes)
        .map_err(|_| invalid_manifest("invalid OCI manifest JSON"))
        .and_then(|manifest| validate_manifest(&manifest, limits))
}
fn verify_digest(bytes: &[u8], expected: &str, reference: &str) -> Result<(), AcquisitionError> {
    let calculated = sha256(bytes);
    match calculated == expected {
        | true => Ok(()),
        | false => Err(AcquisitionError::Sha256Mismatch {
            calculated,
            expected: expected.to_string(),
            remote: reference.to_string(),
        }),
    }
}
