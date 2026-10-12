use super::git::inspect_tree;
use super::util::{io_error, now_seconds, sha256};
use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CacheEntry {
    manifest: CacheManifest,
    payload: PathBuf,
}
struct CacheLock {
    path: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct CacheManifest {
    pub(super) acquired_at_unix_seconds: u64,
    pub(super) bytes: u64,
    pub(super) entries: u64,
    pub(super) identity: AcquisitionIdentity,
    pub(super) source: String,
    pub(super) tree_digest: Option<String>,
    pub(super) version: u32,
}
pub(super) struct StagingDirectory {
    pub(super) path: PathBuf,
}
impl AcquisitionCache {
    /// Resolve the platform cache used by ordinary Xylem runs.
    pub fn system() -> Result<Self, AcquisitionError> {
        env::var_os(ACQUISITION_CACHE_ENV).map_or_else(
            || {
                SafePath::new("acquisitions-v1")
                    .map_err(AcquisitionError::CacheRoot)
                    .and_then(|namespace| {
                        project_cache_directory(QUALIFIER, ORGANIZATION, APPLICATION, &namespace)
                            .map(|root| Self { root })
                            .map_err(AcquisitionError::CacheRoot)
                    })
            },
            |root| Ok(Self { root: PathBuf::from(root) }),
        )
    }
    /// Use an explicit root, primarily for tests and embedded callers.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    /// Return the configured cache root without creating it.
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn entry_path(&self, plan: &AcquisitionPlan) -> Result<PathBuf, AcquisitionError> {
        plan.cache_key
            .as_ref()
            .map(|key| self.root.join(plan.source.namespace()).join(key))
            .ok_or_else(|| AcquisitionError::Identity {
                value: plan.source.to_string(),
                reason: "local sources do not have cache entries",
            })
    }
    pub(super) fn lookup(&self, plan: &AcquisitionPlan) -> Result<Option<CacheEntry>, AcquisitionError> {
        self.entry_path(plan).and_then(|path| match symlink_metadata(&path) {
            | Ok(_) => path
                .parent()
                .ok_or_else(|| AcquisitionError::Corrupt {
                    path: path.clone(),
                    reason: "cache entry has no parent".to_string(),
                })
                .and_then(|parent| validate_cache_parent(&self.root, parent, false))
                .and_then(|()| read_manifest(&path))
                .and_then(|manifest| validate_entry(plan, &path, manifest).map(Some)),
            | Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            | Err(source) => Err(io_error("inspect cache entry", path, source)),
        })
    }
    pub(super) fn publish(&self, plan: &AcquisitionPlan, staging: StagingDirectory, manifest: CacheManifest) -> Result<CacheEntry, AcquisitionError> {
        self.entry_path(plan).and_then(|destination| {
            destination
                .parent()
                .ok_or_else(|| AcquisitionError::Corrupt {
                    path: destination.clone(),
                    reason: "cache entry has no parent".to_string(),
                })
                .and_then(|parent| validate_cache_parent(&self.root, parent, true))
                .and_then(|()| CacheLock::acquire(&destination.with_extension("lock"), plan.limits.git_timeout))
                .and_then(|_lock| match self.lookup(plan) {
                    | Ok(Some(entry)) => Ok(entry),
                    | Ok(None) => {
                        let manifest_path = staging.path.join("manifest.json");
                        manifest.write(&manifest_path).and_then(|()| {
                            rename(&staging.path, &destination)
                                .map_err(|source| io_error("publish cache entry", destination.clone(), source))
                                .and_then(|()| {
                                    core::mem::forget(staging);
                                    validate_entry(plan, &destination, manifest)
                                })
                        })
                    }
                    | Err(error) => Err(error),
                })
        })
    }
    pub(super) fn staging(&self, plan: &AcquisitionPlan) -> Result<StagingDirectory, AcquisitionError> {
        self.entry_path(plan).and_then(|entry| {
            entry
                .parent()
                .ok_or_else(|| AcquisitionError::Corrupt {
                    path: entry.clone(),
                    reason: "cache entry has no parent".to_string(),
                })
                .and_then(|parent| validate_cache_parent(&self.root, parent, true).and_then(|()| StagingDirectory::create(parent)))
        })
    }
}
impl CacheEntry {
    pub(super) fn result(self, plan: &AcquisitionPlan, policy: &AcquisitionPolicy, cache: AcquisitionCacheStatus) -> AcquisitionResult {
        AcquisitionResult {
            evidence: AcquisitionEvidence {
                acquired_at_unix_seconds: self.manifest.acquired_at_unix_seconds,
                size: Memory::kb(self.manifest.bytes as f64 / 1024_f64),
                cache,
                entries: self.manifest.entries,
                identity: self.manifest.identity,
                limits: plan.limits.clone(),
                module_path: plan.module_path.clone(),
                policy: AcquisitionPolicyEvidence {
                    allow_insecure_http: policy.allow_insecure_http,
                    git_ssh_enabled: policy.git_ssh.is_some(),
                    offline: policy.offline,
                },
                source: self.manifest.source,
                tree_digest: self.manifest.tree_digest,
            },
            path: self.payload,
        }
    }
}
impl CacheLock {
    fn acquire(path: &Path, timeout: Seconds) -> Result<Self, AcquisitionError> {
        let started = Instant::now();
        let duration = Duration::from(timeout);
        loop {
            match OpenOptions::new().create_new(true).write(true).open(path) {
                | Ok(mut file) => {
                    let created = now_seconds();
                    let contents = format!("{} {created}\n", process::id());
                    break file
                        .write_all(contents.as_bytes())
                        .and_then(|()| file.sync_all())
                        .map(|()| Self { path: path.to_path_buf() })
                        .map_err(|source| io_error("write acquisition cache lock", path, source));
                }
                | Err(source) if source.kind() == io::ErrorKind::AlreadyExists => match stale_lock(path) {
                    | Ok(true) => {
                        if let Err(source) = remove_file(path) {
                            break Err(io_error("remove stale acquisition cache lock", path, source));
                        }
                    }
                    | Ok(false) if started.elapsed() >= duration => {
                        break Err(AcquisitionError::Limit {
                            limit: "cache lock wait seconds",
                            maximum: timeout.get(),
                            observed: started.elapsed().as_secs(),
                        });
                    }
                    | Ok(false) => thread::sleep(MONITOR_INTERVAL),
                    | Err(error) => break Err(error),
                },
                | Err(source) => break Err(io_error("create acquisition cache lock", path, source)),
            }
        }
    }
}
impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = remove_file(&self.path);
    }
}
impl CacheManifest {
    fn write(&self, path: &Path) -> Result<(), AcquisitionError> {
        File::create(path)
            .map_err(|source| io_error("create acquisition cache manifest", path, source))
            .and_then(|file| {
                let mut writer = BufWriter::new(file);
                serde_json::to_writer(&mut writer, self)
                    .map_err(|source| AcquisitionError::Corrupt {
                        path: path.to_path_buf(),
                        reason: source.to_string(),
                    })
                    .and_then(|()| {
                        writer
                            .write_all(b"\n")
                            .map_err(|source| io_error("write acquisition cache manifest", path, source))
                    })
                    .and_then(|()| {
                        writer
                            .flush()
                            .map_err(|source| io_error("flush acquisition cache manifest", path, source))
                    })
                    .and_then(|()| {
                        writer
                            .get_ref()
                            .sync_all()
                            .map_err(|source| io_error("sync acquisition cache manifest", path, source))
                    })
            })
    }
}
impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.path);
    }
}
impl StagingDirectory {
    fn create(parent: &Path) -> Result<Self, AcquisitionError> {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::AcqRel);
        let created = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let path = parent.join(format!(".staging-{}-{created}-{sequence}", process::id()));
        create_dir(&path)
            .map(|()| Self { path: path.clone() })
            .map_err(|source| io_error("create acquisition staging directory", path, source))
    }
}
fn read_manifest(path: &Path) -> Result<CacheManifest, AcquisitionError> {
    let manifest = path.join("manifest.json");
    symlink_metadata(&manifest)
        .map_err(|source| io_error("inspect acquisition cache manifest", &manifest, source))
        .and_then(|metadata| match metadata.is_file() && !metadata.file_type().is_symlink() {
            | true => read(&manifest)
                .map_err(|source| io_error("read acquisition cache manifest", &manifest, source))
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|source| AcquisitionError::Corrupt {
                        path: manifest,
                        reason: source.to_string(),
                    })
                }),
            | false => Err(AcquisitionError::Corrupt {
                path: manifest,
                reason: "cache manifest is not a regular file".to_string(),
            }),
        })
}
fn stale_lock(path: &Path) -> Result<bool, AcquisitionError> {
    read(path)
        .map_err(|source| io_error("read acquisition cache lock", path, source))
        .map(|contents| {
            let values = String::from_utf8_lossy(&contents)
                .split_whitespace()
                .filter_map(|value| value.parse::<u64>().ok())
                .collect::<Vec<_>>();
            match values.as_slice() {
                | [pid, created] => {
                    let owner_absent = System::new_all().process(Pid::from_u32(*pid as u32)).is_none();
                    owner_absent && now_seconds().saturating_sub(*created) > LOCK_STALE_AFTER.as_secs()
                }
                | _ => false,
            }
        })
}
fn validate_cache_parent(root: &Path, parent: &Path, create: bool) -> Result<(), AcquisitionError> {
    let prepared = match create {
        | true => create_dir_all(parent).map_err(|source| io_error("create cache namespace", parent, source)),
        | false => Ok(()),
    };
    prepared
        .and_then(|()| canonicalize(root).map_err(|source| io_error("canonicalize acquisition cache root", root, source)))
        .and_then(|canonical_root| {
            canonicalize(parent)
                .map_err(|source| io_error("canonicalize acquisition cache namespace", parent, source))
                .and_then(|canonical_parent| match canonical_parent.starts_with(&canonical_root) {
                    | true => Ok(()),
                    | false => Err(AcquisitionError::Corrupt {
                        path: parent.to_path_buf(),
                        reason: "cache namespace escapes the configured cache root".to_string(),
                    }),
                })
        })
}
fn validate_cache_path(path: &Path, directory: bool) -> Result<(), AcquisitionError> {
    symlink_metadata(path)
        .map_err(|source| io_error("inspect acquisition cache path", path, source))
        .and_then(
            |metadata| match !metadata.file_type().is_symlink() && if directory { metadata.is_dir() } else { metadata.is_file() } {
                | true => Ok(()),
                | false => Err(AcquisitionError::Corrupt {
                    path: path.to_path_buf(),
                    reason: match directory {
                        | true => "cache path is not a directory",
                        | false => "cache path is not a regular file",
                    }
                    .to_string(),
                }),
            },
        )
}
fn validate_entry(plan: &AcquisitionPlan, path: &Path, manifest: CacheManifest) -> Result<CacheEntry, AcquisitionError> {
    let payload = match &plan.source {
        | AcquisitionSource::Git { .. } => path.join("repository"),
        | AcquisitionSource::HttpBinary { .. } | AcquisitionSource::HttpDocument { .. } => path.join("payload"),
        | AcquisitionSource::Local { .. } => path.join("payload"),
    };
    validate_cache_path(path, true)
        .and_then(|()| validate_cache_path(&payload, matches!(plan.source, AcquisitionSource::Git { .. })))
        .and_then(|()| {
            let identity_matches = plan.identity.as_ref() == Some(&manifest.identity);
            let version_matches = manifest.version == ACQUISITION_CACHE_VERSION;
            match (identity_matches, version_matches) {
                | (false, _) => Err(AcquisitionError::Corrupt {
                    path: path.to_path_buf(),
                    reason: "manifest identity does not match the requested acquisition".to_string(),
                }),
                | (_, false) => Err(AcquisitionError::Corrupt {
                    path: path.to_path_buf(),
                    reason: format!("unsupported cache version {}", manifest.version),
                }),
                | (true, true) => match &plan.source {
                    | AcquisitionSource::HttpBinary { sha256: expected, .. } | AcquisitionSource::HttpDocument { sha256: expected, .. } => {
                        read(&payload)
                            .map_err(|source| io_error("read cached acquisition payload", &payload, source))
                            .and_then(|bytes| match (sha256(&bytes) == *expected, bytes.len() as u64 == manifest.bytes) {
                                | (true, true) => Ok(CacheEntry { manifest, payload }),
                                | (false, _) => Err(AcquisitionError::Corrupt {
                                    path: payload,
                                    reason: "cached payload digest does not match its identity".to_string(),
                                }),
                                | (_, false) => Err(AcquisitionError::Corrupt {
                                    path: payload,
                                    reason: "cached payload size does not match its manifest".to_string(),
                                }),
                            })
                    }
                    | AcquisitionSource::Git { .. } => inspect_tree(&payload, &plan.limits).and_then(|(bytes, entries, tree_digest)| {
                        match (
                            bytes == manifest.bytes,
                            entries == manifest.entries,
                            manifest.tree_digest.as_deref() == Some(tree_digest.as_str()),
                        ) {
                            | (true, true, true) => Ok(CacheEntry { manifest, payload }),
                            | _ => Err(AcquisitionError::Corrupt {
                                path: payload,
                                reason: "cached Git tree does not match its manifest".to_string(),
                            }),
                        }
                    }),
                    | AcquisitionSource::Local { .. } => Err(AcquisitionError::Identity {
                        value: plan.source.to_string(),
                        reason: "local sources do not have cache entries",
                    }),
                },
            }
        })
}
