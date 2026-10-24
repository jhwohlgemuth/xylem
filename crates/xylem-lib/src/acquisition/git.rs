use super::policy::normalize_origin;
use super::util::io_error;
use super::{
    args, canonicalize, cmd, command_string, create_dir, read, read_dir, read_link, redact_url, symlink_metadata, thread, AcquisitionError,
    AcquisitionLimits, AcquisitionPolicy, Arc, AtomicBool, AtomicU64, BTreeMap, CancellationToken, CaptureLimits, Digest, Duration, ExecutionOptions,
    File, Instant, Memory, Ordering, Path, Seconds, Sha256, UnitOfTime, MONITOR_INTERVAL,
};
use std::ffi::OsString;

pub(super) struct GitCommandContext<'a> {
    pub(super) environment: &'a BTreeMap<String, String>,
    pub(super) maximum_staging: Memory,
    pub(super) output: Memory,
    pub(super) repository: &'a Path,
    pub(super) url: &'a str,
}
#[derive(Default)]
struct TreeTotals {
    bytes: u64,
    entries: u64,
    hasher: Sha256,
}
fn add_tree_entry(
    totals: &mut TreeTotals,
    relative: &Path,
    kind: &[u8],
    mode: u32,
    contents: &[u8],
    limits: &AcquisitionLimits,
) -> Result<(), AcquisitionError> {
    totals.entries = totals.entries.saturating_add(1);
    totals.bytes = totals.bytes.saturating_add(contents.len() as u64);
    let maximum_tree = limits.git_tree.checked_bytes().unwrap_or_default();
    let limit = match (totals.entries > limits.git_entries, totals.bytes > maximum_tree) {
        | (true, _) => Some(("Git tree entries", totals.entries, limits.git_entries)),
        | (_, true) => Some(("Git tree bytes", totals.bytes, maximum_tree)),
        | (false, false) => None,
    };
    limit.map_or_else(
        || {
            totals.hasher.update(kind);
            totals.hasher.update([0]);
            totals.hasher.update(relative.to_string_lossy().as_bytes());
            totals.hasher.update([0]);
            totals.hasher.update(mode.to_le_bytes());
            totals.hasher.update(contents);
            Ok(())
        },
        |(limit, observed, maximum)| Err(AcquisitionError::Limit { limit, maximum, observed }),
    )
}
pub(super) fn directory_size(path: &Path) -> Result<Memory, AcquisitionError> {
    directory_size_value(path).map(|bytes| Memory::kb(bytes as f64 / 1024_f64))
}
fn directory_size_value(path: &Path) -> Result<u64, AcquisitionError> {
    symlink_metadata(path)
        .map_err(|source| io_error("inspect Git staging path", path, source))
        .and_then(|metadata| match metadata.is_dir() {
            | false => Ok(metadata.len()),
            | true => read_dir(path)
                .map_err(|source| io_error("read Git staging directory", path, source))
                .and_then(|entries| {
                    entries
                        .map(|entry| {
                            entry
                                .map_err(|source| io_error("read Git staging entry", path, source))
                                .and_then(|entry| directory_size_value(&entry.path()))
                        })
                        .try_fold(0_u64, |total, bytes| bytes.map(|bytes| total.saturating_add(bytes)))
                }),
        })
}
#[cfg(unix)]
fn file_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    u32::from(metadata.permissions().mode() & 0o111 != 0)
}
#[cfg(not(unix))]
fn file_mode(_metadata: &std::fs::Metadata) -> u32 {
    0
}
pub(super) fn git_environment(policy: &AcquisitionPolicy, repository: &Path) -> Result<BTreeMap<String, String>, AcquisitionError> {
    repository
        .parent()
        .map(|parent| parent.join("git-environment"))
        .ok_or_else(|| AcquisitionError::Corrupt {
            path: repository.to_path_buf(),
            reason: "Git staging repository has no parent".to_string(),
        })
        .and_then(|home| {
            create_dir(&home)
                .map_err(|source| io_error("create isolated Git environment", &home, source))
                .and_then(|()| {
                    let global = home.join("global.gitconfig");
                    let templates = home.join("templates");
                    create_dir(&templates)
                        .map_err(|source| io_error("create isolated Git templates", &templates, source))
                        .and_then(|()| File::create(&global).map_err(|source| io_error("create isolated Git configuration", &global, source)))
                        .map(|_| {
                            let mut environment = BTreeMap::from([
                                ("ALL_PROXY".to_string(), String::new()),
                                ("GCM_INTERACTIVE".to_string(), "Never".to_string()),
                                ("GIT_ASKPASS".to_string(), String::new()),
                                (
                                    "GIT_ALLOW_PROTOCOL".to_string(),
                                    match policy.allow_insecure_http {
                                        | true => "https:http:ssh",
                                        | false => "https:ssh",
                                    }
                                    .to_string(),
                                ),
                                ("GIT_ALTERNATE_OBJECT_DIRECTORIES".to_string(), String::new()),
                                ("GIT_CONFIG_COUNT".to_string(), "2".to_string()),
                                ("GIT_CONFIG_GLOBAL".to_string(), global.display().to_string()),
                                ("GIT_CONFIG_KEY_0".to_string(), "http.followRedirects".to_string()),
                                ("GIT_CONFIG_KEY_1".to_string(), "http.proxy".to_string()),
                                ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
                                ("GIT_CONFIG_PARAMETERS".to_string(), String::new()),
                                ("GIT_CONFIG_VALUE_0".to_string(), "false".to_string()),
                                ("GIT_CONFIG_VALUE_1".to_string(), String::new()),
                                ("GIT_LFS_SKIP_SMUDGE".to_string(), "1".to_string()),
                                ("GIT_PROTOCOL_FROM_USER".to_string(), "0".to_string()),
                                ("GIT_SSL_NO_VERIFY".to_string(), "false".to_string()),
                                ("GIT_TEMPLATE_DIR".to_string(), templates.display().to_string()),
                                ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
                                ("HOME".to_string(), home.display().to_string()),
                                ("HTTP_PROXY".to_string(), String::new()),
                                ("HTTPS_PROXY".to_string(), String::new()),
                                ("SSH_AGENT_PID".to_string(), String::new()),
                                ("SSH_AUTH_SOCK".to_string(), String::new()),
                                ("USERPROFILE".to_string(), home.display().to_string()),
                                ("XDG_CONFIG_HOME".to_string(), home.display().to_string()),
                                ("all_proxy".to_string(), String::new()),
                                ("http_proxy".to_string(), String::new()),
                                ("https_proxy".to_string(), String::new()),
                            ]);
                            if let Some(ssh) = policy.git_ssh.as_ref() {
                                let arguments = args![
                                    ("-F", "/dev/null"),
                                    ("-i", &ssh.identity),
                                    ("-o", "BatchMode=yes"),
                                    ("-o", "ClearAllForwardings=yes"),
                                    ("-o", "GlobalKnownHostsFile=/dev/null"),
                                    ("-o", "IdentitiesOnly=yes"),
                                    ("-o", "IdentityAgent=none"),
                                    ("-o", "PermitLocalCommand=no"),
                                    ("-o", "ProxyCommand=none"),
                                    ("-o", "StrictHostKeyChecking=yes"),
                                    ("-o", format!("UserKnownHostsFile={}", ssh.known_hosts.display()))
                                ];
                                let command = command_string("ssh", &arguments);
                                environment.insert("GIT_SSH_COMMAND".to_string(), command);
                            }
                            environment
                        })
                })
        })
}
pub(super) fn inspect_tree(root: &Path, limits: &AcquisitionLimits) -> Result<(u64, u64, String), AcquisitionError> {
    let mut totals = TreeTotals::default();
    inspect_tree_at(root, root, limits, &mut totals).map(|()| {
        let digest = format!("{:x}", totals.hasher.finalize());
        (totals.bytes, totals.entries, digest)
    })
}
fn inspect_tree_at(root: &Path, path: &Path, limits: &AcquisitionLimits, totals: &mut TreeTotals) -> Result<(), AcquisitionError> {
    read_dir(path)
        .map_err(|source| io_error("read acquired Git tree", path, source))
        .and_then(|entries| {
            entries
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| io_error("read acquired Git tree entry", path, source))
        })
        .and_then(|mut entries| {
            entries.sort_by_key(std::fs::DirEntry::file_name);
            entries
                .into_iter()
                .try_for_each(|entry| inspect_tree_entry(root, &entry.path(), limits, totals))
        })
}
fn inspect_tree_entry(root: &Path, path: &Path, limits: &AcquisitionLimits, totals: &mut TreeTotals) -> Result<(), AcquisitionError> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    symlink_metadata(path)
        .map_err(|source| io_error("inspect acquired Git tree", path, source))
        .and_then(|metadata| {
            if relative == Path::new(".git") || relative.starts_with(".git") {
                Ok(())
            } else if metadata.file_type().is_symlink() {
                read_link(path)
                    .map_err(|source| io_error("read acquired Git symlink", path, source))
                    .and_then(|target| {
                        canonicalize(root)
                            .map_err(|source| io_error("canonicalize acquired Git root", root, source))
                            .and_then(|canonical_root| {
                                canonicalize(path)
                                    .map_err(|source| io_error("resolve acquired Git symlink", path, source))
                                    .and_then(|resolved| match resolved.starts_with(&canonical_root) {
                                        | true => add_tree_entry(totals, relative, b"link", 0o120000, target.to_string_lossy().as_bytes(), limits),
                                        | false => Err(AcquisitionError::Corrupt {
                                            path: path.to_path_buf(),
                                            reason: "Git symlink escapes the acquired tree".to_string(),
                                        }),
                                    })
                            })
                    })
            } else if metadata.is_dir() {
                add_tree_entry(totals, relative, b"directory", 0o040000, &[], limits).and_then(|()| inspect_tree_at(root, path, limits, totals))
            } else if metadata.is_file() {
                read(path)
                    .map_err(|source| io_error("read acquired Git file", path, source))
                    .and_then(|bytes| add_tree_entry(totals, relative, b"file", file_mode(&metadata), &bytes, limits))
            } else {
                Err(AcquisitionError::Corrupt {
                    path: path.to_path_buf(),
                    reason: "Git tree contains an unsupported special file".to_string(),
                })
            }
        })
}
pub(super) fn remaining_timeout(started: Instant, timeout: Seconds) -> Result<Duration, AcquisitionError> {
    Duration::from(timeout)
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(AcquisitionError::Limit {
            limit: "Git acquisition seconds",
            maximum: timeout.get(),
            observed: started.elapsed().as_secs(),
        })
}
pub(super) fn run_git_command(
    context: &GitCommandContext<'_>,
    action: &'static str,
    arguments: Vec<OsString>,
    timeout: Duration,
) -> Result<(), AcquisitionError> {
    let cancellation = CancellationToken::new();
    let finished = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicU64::new(0));
    let monitor_finished = Arc::clone(&finished);
    let monitor_observed = Arc::clone(&observed);
    let monitor_cancellation = cancellation.clone();
    let staging = context.repository.parent().unwrap_or(context.repository).to_path_buf();
    let maximum_staging = context.maximum_staging.clone();
    let maximum_staging_bytes = maximum_staging.checked_bytes().unwrap_or_default();
    let monitor = thread::spawn(move || {
        while !monitor_finished.load(Ordering::Acquire) {
            if let Ok(size) = directory_size(&staging) {
                let bytes = size.checked_bytes().unwrap_or(u64::MAX);
                monitor_observed.store(bytes, Ordering::Release);
                if bytes > maximum_staging_bytes {
                    monitor_cancellation.cancel();
                    break;
                }
            }
            thread::sleep(MONITOR_INTERVAL);
        }
    });
    let options = ExecutionOptions::new()
        .with_cancellation(cancellation)
        .with_capture_limits(CaptureLimits::new(
            Some(context.output.checked_bytes().unwrap_or_default().min(usize::MAX as u64) as usize),
            None,
        ))
        .with_timeout(timeout);
    let result = cmd!(exec checked "git", arguments; opt: &options, dir: context.repository, env: context.environment.clone());
    finished.store(true, Ordering::Release);
    let _ = monitor.join();
    directory_size(context.repository.parent().unwrap_or(context.repository)).and_then(|final_size| {
        let final_bytes = final_size.checked_bytes().unwrap_or(u64::MAX);
        let observed = observed.load(Ordering::Acquire).max(final_bytes);
        match observed > maximum_staging_bytes {
            | true => Err(AcquisitionError::Limit {
                limit: "Git staging bytes",
                maximum: maximum_staging_bytes,
                observed,
            }),
            | false => result.map(|_| ()).map_err(|source| AcquisitionError::Process {
                action,
                remote: redact_url(context.url),
                reason: source.to_string(),
            }),
        }
    })
}
pub(super) fn verify_git(
    repository: &Path,
    url: &str,
    commit: &str,
    environment: &BTreeMap<String, String>,
    output: &Memory,
    started: Instant,
    timeout: Seconds,
) -> Result<(), AcquisitionError> {
    let maximum = output.checked_bytes().unwrap_or_default().min(usize::MAX as u64) as usize;
    normalize_origin(url).and_then(|origin| {
        [
            ("verify Git HEAD", args!["rev-parse", "HEAD"], commit.to_string()),
            ("verify Git origin", args!["remote", "get-url", "origin"], origin),
        ]
        .into_iter()
        .try_for_each(|(action, arguments, expected)| {
            remaining_timeout(started, timeout).and_then(|timeout| {
                let options = ExecutionOptions::new()
                    .with_capture_limits(CaptureLimits::new(Some(maximum), None))
                    .with_timeout(timeout);
                cmd!(exec checked "git", arguments; opt: &options, dir: repository, env: environment.clone())
                    .map_err(|source| AcquisitionError::Process {
                        action,
                        remote: redact_url(url),
                        reason: source.to_string(),
                    })
                    .and_then(|result| {
                        let actual = String::from_utf8_lossy(result.stdout().bytes()).trim().to_string();
                        match action {
                            | "verify Git origin" => normalize_origin(&actual),
                            | _ => Ok(actual),
                        }
                        .and_then(|actual| match actual == expected {
                            | true => Ok(()),
                            | false => Err(AcquisitionError::Process {
                                action,
                                remote: redact_url(url),
                                reason: format!("expected {expected}, observed {actual}"),
                            }),
                        })
                    })
            })
        })
    })
}
