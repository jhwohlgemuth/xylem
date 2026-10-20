use super::git::{git_environment, inspect_tree, remaining_timeout, run_git_command, verify_git, GitCommandContext};
use super::oci::run_oras;
use super::oci_registry::validate_manifest_bytes;
use super::policy::normalize_origin;
use super::util::sha256;
use super::*;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
#[cfg(unix)]
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
#[cfg(unix)]
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Barrier;
use std::thread::JoinHandle;
use std::time::Instant;

struct TestDirectory(PathBuf);
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.0);
    }
}
impl TestDirectory {
    fn create(name: &str) -> Self {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::AcqRel);
        let path = env::temp_dir().join(format!("xylem-acquisition-{name}-{}-{sequence}", process::id()));
        create_dir_all(&path).expect("create acquisition test directory");
        Self(path)
    }
}
fn accept_http_request(listener: &TcpListener) -> Option<(std::net::TcpStream, String)> {
    listener.set_nonblocking(true).expect("set test listener nonblocking");
    let started = Instant::now();
    loop {
        if started.elapsed() >= Duration::from_secs(20) {
            break None;
        }
        match listener.accept() {
            | Ok((mut stream, _)) => {
                stream.set_nonblocking(false).expect("set test HTTP request blocking");
                stream
                    .set_read_timeout(Some(Duration::from_secs(20)))
                    .expect("bound test HTTP request read");
                let mut request = [0_u8; 4096];
                let length = stream.read(&mut request).expect("read test HTTP request");
                break Some((stream, String::from_utf8_lossy(&request[..length]).into_owned()));
            }
            | Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
            | Err(error) => panic!("accept test HTTP request: {error}"),
        }
    }
}
#[cfg(unix)]
fn git_fixture(root: &Path) -> String {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let work = root.join("work");
    let bare = root.join("repo.git");
    let run = |args: &[&str]| {
        let output = Command::new("git").args(args).output().expect("run Git fixture command");
        assert!(
            output.status.success(),
            "Git fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("Git fixture output").trim().to_string()
    };
    run(&["init", "--quiet", "--bare", bare.to_str().expect("bare path")]);
    run(&["init", "--quiet", work.to_str().expect("work path")]);
    run(&["-C", work.to_str().expect("work path"), "config", "user.name", "Xylem Fixture"]);
    run(&["-C", work.to_str().expect("work path"), "config", "user.email", "fixture@example.invalid"]);
    std::fs::write(work.join("run.sh"), b"#!/bin/sh\nexit 0\n").expect("write Git fixture file");
    std::fs::set_permissions(work.join("run.sh"), std::fs::Permissions::from_mode(0o755)).expect("mark Git fixture script executable");
    std::fs::write(work.join("config.json"), b"{}\n").expect("write second Git fixture file");
    symlink("run.sh", work.join("run-link")).expect("create contained Git fixture symlink");
    run(&["-C", work.to_str().expect("work path"), "add", "run.sh", "run-link", "config.json"]);
    run(&["-C", work.to_str().expect("work path"), "commit", "--quiet", "-m", "fixture"]);
    let commit = run(&["-C", work.to_str().expect("work path"), "rev-parse", "HEAD"]);
    run(&[
        "-C",
        work.to_str().expect("work path"),
        "push",
        "--quiet",
        bare.to_str().expect("bare path"),
        "HEAD:refs/heads/main",
    ]);
    commit
}
fn policy(hosts: &[String], allow_http: bool, offline: bool) -> AcquisitionPolicy {
    AcquisitionPolicy::from_hosts(Some(hosts), allow_http, None, offline, AcquisitionLimits::default()).expect("construct acquisition policy")
}
fn serve(body: Vec<u8>) -> Option<(String, JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        | Ok(listener) => listener,
        | Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return None,
        | Err(error) => panic!("bind acquisition test server: {error}"),
    };
    let address = listener.local_addr().expect("read acquisition test address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept acquisition request");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        stream.write_all(header.as_bytes()).expect("write acquisition response header");
        stream.write_all(&body).expect("write acquisition response body");
    });
    Some((format!("http://{address}/module"), handle))
}
#[cfg(unix)]
fn serve_git_http(root: PathBuf) -> (String, Arc<AtomicBool>, JoinHandle<usize>) {
    serve_git(root, None)
}
#[cfg(unix)]
fn serve_git_https(root: PathBuf) -> (String, Arc<AtomicBool>, JoinHandle<usize>) {
    let certificate = CertificateDer::from_pem_slice(include_bytes!("fixtures/git-https-cert.pem")).expect("parse Git HTTPS certificate");
    let key = PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/git-https-key.pem")).expect("parse Git HTTPS key");
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .expect("configure Git HTTPS server");
    serve_git(root, Some(Arc::new(config)))
}
#[cfg(unix)]
fn serve_git(root: PathBuf, tls: Option<Arc<ServerConfig>>) -> (String, Arc<AtomicBool>, JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Git fixture HTTP server");
    let address = listener.local_addr().expect("Git fixture server address");
    let finished = Arc::new(AtomicBool::new(false));
    let server_finished = Arc::clone(&finished);
    let scheme = if tls.is_some() { "https" } else { "http" };
    let server = thread::spawn(move || {
        listener.set_nonblocking(true).expect("set Git server nonblocking");
        let mut requests = 0;
        let started = Instant::now();
        while !server_finished.load(Ordering::Acquire) && started.elapsed() < Duration::from_secs(20) {
            match listener.accept() {
                | Ok((stream, _)) => {
                    stream.set_nonblocking(false).expect("set Git HTTP request blocking");
                    match &tls {
                        | Some(config) => {
                            let connection = ServerConnection::new(Arc::clone(config)).expect("accept Git HTTPS connection");
                            let mut stream = StreamOwned::new(connection, stream);
                            serve_git_request(&mut stream, &root);
                            stream.conn.send_close_notify();
                            stream.flush().expect("close Git HTTPS connection");
                        }
                        | None => serve_git_request(stream, &root),
                    }
                    requests += 1;
                }
                | Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                | Err(error) => panic!("accept Git HTTP request: {error}"),
            }
        }
        requests
    });
    (format!("{scheme}://{address}/repo.git"), finished, server)
}
#[cfg(unix)]
fn serve_git_request(mut stream: impl Read + Write, root: &Path) {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") && header.len() < 16 * 1024 {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).expect("read Git HTTP header");
        header.push(byte[0]);
    }
    let header = String::from_utf8(header).expect("Git HTTP header UTF-8");
    let first = header.lines().next().expect("Git HTTP request line");
    let mut fields = first.split_whitespace();
    let method = fields.next().expect("Git HTTP method");
    let uri = fields.next().expect("Git HTTP URI");
    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    let length = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let content_type = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim().to_string())
        .unwrap_or_default();
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).expect("read Git HTTP body");
    let mut child = Command::new("git")
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("REQUEST_METHOD", method)
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("CONTENT_TYPE", content_type)
        .env("CONTENT_LENGTH", length.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start git http-backend");
    child
        .stdin
        .take()
        .expect("Git backend stdin")
        .write_all(&body)
        .expect("write Git backend request");
    let output = child.wait_with_output().expect("wait for Git backend");
    assert!(
        output.status.success(),
        "git http-backend failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n").expect("write Git status");
    stream.write_all(&output.stdout).expect("write Git backend response");
}
fn serve_many(body: Vec<u8>, requests: usize) -> Option<(String, JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        | Ok(listener) => listener,
        | Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return None,
        | Err(error) => panic!("bind acquisition test server: {error}"),
    };
    let address = listener.local_addr().expect("read acquisition test address");
    let handle = thread::spawn(move || {
        let mut streams = (0..requests)
            .map(|_| listener.accept().expect("accept acquisition request").0)
            .map(|mut stream| {
                let mut request = [0_u8; 1024];
                let _ = stream.read(&mut request);
                stream
            })
            .collect::<Vec<_>>();
        streams.iter_mut().for_each(|stream| {
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(header.as_bytes()).expect("write acquisition response header");
            stream.write_all(&body).expect("write acquisition response body");
        });
    });
    Some((format!("http://{address}/module"), handle))
}
fn with_loopback(run: impl FnOnce(TcpListener)) {
    match TcpListener::bind("127.0.0.1:0") {
        | Ok(listener) => run(listener),
        | Err(error) if error.kind() == io::ErrorKind::PermissionDenied => (),
        | Err(error) => panic!("bind test loopback listener: {error}"),
    }
}
fn write_http_response(stream: &mut std::net::TcpStream, status: &str, headers: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("write test HTTP header");
    stream.write_all(body).expect("write test HTTP body");
}

#[test]
fn test_cache_reuses_verified_http_payload_offline() {
    let body = b"verified module bytes".to_vec();
    let Some((url, server)) = serve(body.clone()) else {
        return;
    };
    let host = Url::parse(&url)
        .expect("parse fixture URL")
        .host_str()
        .map(str::to_string)
        .expect("fixture host");
    let port = Url::parse(&url).expect("parse fixture URL").port().expect("fixture port");
    let hosts = vec![format!("{host}:{port}")];
    let root = TestDirectory::create("cache-hit");
    let online = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary { sha256: sha256(&body), url },
        &online,
    )
    .expect("plan HTTP acquisition");
    let first = AcquisitionExecutor::new(online, AcquisitionCache::new(root.0.clone()))
        .execute(&plan)
        .expect("populate verified cache");
    server.join().expect("join acquisition server");
    assert_eq!(first.evidence.cache, AcquisitionCacheStatus::Miss);
    assert_eq!(read(&first.path).expect("read acquired payload"), body);
    let offline = policy(&hosts, true, true);
    let second = AcquisitionExecutor::new(offline, AcquisitionCache::new(root.0.clone()))
        .execute(&plan)
        .expect("reuse verified cache offline");
    assert_eq!(second.evidence.cache, AcquisitionCacheStatus::Hit);
    assert_eq!(second.evidence.size.checked_bytes(), Some(body.len() as u64));
    assert_eq!(first.path, second.path);
}
#[test]
fn test_checksum_escape_hatch_records_the_observed_digest() {
    let body = b"emergency module bytes".to_vec();
    let Some((url, server)) = serve(body.clone()) else {
        return;
    };
    let parsed = Url::parse(&url).expect("parse fixture URL");
    let hosts = vec![format!(
        "{}:{}",
        parsed.host_str().expect("fixture host"),
        parsed.port().expect("fixture port")
    )];
    let source = AcquisitionSource::HttpBinaryUnverified { url };
    let denied = AcquisitionPlan::build(vec!["binary".to_string()], source.clone(), &policy(&hosts, true, false));
    assert!(matches!(denied, Err(AcquisitionError::Policy { .. })));
    let enabled = policy(&hosts, true, false).with_checksum_verification_disabled(true);
    let plan = AcquisitionPlan::build(vec!["binary".to_string()], source, &enabled).expect("plan unverified HTTP acquisition");
    assert_eq!(plan.cache_key, None);
    assert_eq!(plan.identity, None);
    let root = TestDirectory::create("skip-checksum");
    let result = AcquisitionExecutor::new(enabled, AcquisitionCache::new(root.0.clone()))
        .execute(&plan)
        .expect("acquire unverified HTTP binary");
    server.join().expect("join acquisition server");
    assert_eq!(result.evidence.cache, AcquisitionCacheStatus::Miss);
    assert_eq!(result.evidence.identity, AcquisitionIdentity::Sha256 { digest: sha256(&body) });
    assert!(result.evidence.policy.skip_checksum_verification);
    assert_eq!(read(&result.path).expect("read acquired payload"), body);
}
#[test]
fn test_concurrent_requests_share_one_published_cache_entry() {
    let body = b"concurrent verified module bytes".to_vec();
    let Some((url, server)) = serve_many(body.clone(), 2) else {
        return;
    };
    let parsed = Url::parse(&url).expect("parse fixture URL");
    let hosts = vec![format!(
        "{}:{}",
        parsed.host_str().expect("fixture host"),
        parsed.port().expect("fixture port")
    )];
    let policy = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary { sha256: sha256(&body), url },
        &policy,
    )
    .expect("plan concurrent HTTP acquisition");
    let root = TestDirectory::create("concurrent-cache-publication");
    let executor = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone()));
    let barrier = Arc::new(Barrier::new(2));
    let results = (0..2)
        .map(|_| {
            let executor = executor.clone();
            let barrier = Arc::clone(&barrier);
            let plan = plan.clone();
            thread::spawn(move || {
                barrier.wait();
                executor.execute(&plan)
            })
        })
        .collect::<Vec<_>>();
    let acquired = results
        .into_iter()
        .map(|result| result.join().expect("join acquisition worker"))
        .collect::<Result<Vec<_>, _>>()
        .expect("acquire identical source concurrently");
    server.join().expect("join acquisition server");
    assert_eq!(acquired[0].path, acquired[1].path);
    assert_eq!(read(&acquired[0].path).expect("read published cache payload"), body);
    assert_eq!(read_dir(root.0.join("binary")).expect("read cache namespace").count(), 1);
}
#[test]
fn test_corrupted_cache_entry_fails_closed_offline() {
    let body = b"verified module bytes".to_vec();
    let Some((url, server)) = serve(body.clone()) else {
        return;
    };
    let parsed = Url::parse(&url).expect("parse fixture URL");
    let hosts = vec![format!(
        "{}:{}",
        parsed.host_str().expect("fixture host"),
        parsed.port().expect("fixture port")
    )];
    let root = TestDirectory::create("corrupt-cache");
    let digest = sha256(&body);
    let online = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(
        vec!["document".to_string()],
        AcquisitionSource::HttpDocument { sha256: digest.clone(), url },
        &online,
    )
    .expect("plan HTTP acquisition");
    AcquisitionExecutor::new(online, AcquisitionCache::new(root.0.clone()))
        .execute(&plan)
        .expect("populate verified cache");
    server.join().expect("join acquisition server");
    std::fs::write(
        root.0
            .join("document")
            .join(plan.cache_key.as_deref().expect("remote cache key"))
            .join("payload"),
        b"tampered",
    )
    .expect("tamper with cached payload");
    let result = AcquisitionExecutor::new(policy(&hosts, true, true), AcquisitionCache::new(root.0.clone())).execute(&plan);
    assert!(matches!(result, Err(AcquisitionError::Corrupt { .. })));
}
#[test]
fn test_executor_rejects_a_tampered_plan_before_materialization() {
    let hosts = vec!["127.0.0.1:9".to_string()];
    let policy = policy(&hosts, true, false);
    let mut plan = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary {
            sha256: format!("sha256:{}", "a".repeat(64)),
            url: "http://127.0.0.1:9/module".to_string(),
        },
        &policy,
    )
    .expect("plan HTTP acquisition");
    plan.cache_key = Some("../../outside-cache".to_string());
    let root = TestDirectory::create("tampered-plan");
    let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
    assert!(matches!(result, Err(AcquisitionError::Policy { .. })));
    assert_eq!(read_dir(&root.0).expect("read empty cache root").count(), 0);
}
#[test]
fn test_limits_can_only_tighten() {
    let parent = AcquisitionLimits::default();
    let child = AcquisitionLimits {
        binary: Memory::mb(256),
        git_entries: parent.git_entries * 2,
        ..parent.clone()
    };
    let effective = parent.restrict(&child);
    assert_eq!(effective.binary, child.binary);
    assert_eq!(effective.git_entries, parent.git_entries);
    let hosts = vec!["example.org".to_string()];
    let policy = policy(&hosts, false, false);
    let invalid = AcquisitionLimits {
        git_entries: 0,
        ..AcquisitionLimits::default()
    };
    assert!(matches!(policy.restrict(None, &invalid), Err(AcquisitionError::Identity { .. })));
}
#[test]
fn test_offline_cache_miss_does_not_connect() {
    let hosts = vec!["127.0.0.1:9".to_string()];
    let offline = policy(&hosts, true, true);
    let plan = AcquisitionPlan::build(
        vec!["document".to_string()],
        AcquisitionSource::HttpDocument {
            sha256: format!("sha256:{}", "a".repeat(64)),
            url: "http://127.0.0.1:9/module".to_string(),
        },
        &offline,
    )
    .expect("plan offline acquisition");
    let root = TestDirectory::create("offline-miss");
    let result = AcquisitionExecutor::new(offline, AcquisitionCache::new(root.0.clone())).execute(&plan);
    assert!(matches!(result, Err(AcquisitionError::Offline { .. })));
    assert!(!root.0.join("document").exists());
}
#[test]
fn test_planner_rejects_ambiguous_or_mutable_remote_sources() {
    let hosts = vec!["example.org".to_string()];
    let https = policy(&hosts, false, false);
    let missing_digest = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary {
            sha256: "abc".to_string(),
            url: "https://example.org/module".to_string(),
        },
        &https,
    );
    assert!(matches!(missing_digest, Err(AcquisitionError::Identity { .. })));
    let query = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary {
            sha256: format!("sha256:{}", "a".repeat(64)),
            url: "https://example.org/module?token=secret".to_string(),
        },
        &https,
    );
    assert!(matches!(query, Err(AcquisitionError::Policy { .. })));
    let branch = AcquisitionPlan::build(
        vec!["script".to_string()],
        AcquisitionSource::Git {
            commit: "main".to_string(),
            url: "https://example.org/project.git".to_string(),
        },
        &https,
    );
    assert!(matches!(branch, Err(AcquisitionError::Identity { .. })));
}
#[test]
fn test_policy_is_fail_closed_and_schemes_require_operator_capabilities() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let denied = AcquisitionPolicy::from_hosts(None, false, None, false, AcquisitionLimits::default()).expect("construct deny-all policy");
    let no_host = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary {
            sha256: digest.clone(),
            url: "https://example.org/module".to_string(),
        },
        &denied,
    );
    assert!(matches!(no_host, Err(AcquisitionError::Policy { .. })));
    let hosts = vec!["example.org".to_string()];
    let secure = policy(&hosts, false, false);
    let http = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary {
            sha256: digest,
            url: "http://example.org/module".to_string(),
        },
        &secure,
    );
    assert!(matches!(http, Err(AcquisitionError::Policy { .. })));
    let ssh_without_credentials = AcquisitionPlan::build(
        vec!["script".to_string()],
        AcquisitionSource::Git {
            commit: "a".repeat(40),
            url: "ssh://git@example.org/project.git".to_string(),
        },
        &secure,
    );
    assert!(matches!(ssh_without_credentials, Err(AcquisitionError::Policy { .. })));
    let invalid_ssh = AcquisitionPolicy::from_hosts(
        Some(&hosts),
        false,
        Some(GitSshPolicy {
            identity: PathBuf::from("missing-identity"),
            known_hosts: PathBuf::from("missing-known-hosts"),
        }),
        false,
        AcquisitionLimits::default(),
    );
    assert!(matches!(invalid_ssh, Err(AcquisitionError::Policy { .. })));
}
#[test]
fn test_response_limit_prevents_cache_publication() {
    let body = b"too large".to_vec();
    let Some((url, server)) = serve(body.clone()) else {
        return;
    };
    let parsed = Url::parse(&url).expect("parse fixture URL");
    let hosts = vec![format!(
        "{}:{}",
        parsed.host_str().expect("fixture host"),
        parsed.port().expect("fixture port")
    )];
    let limits = AcquisitionLimits {
        binary: Memory::kb(2_f64 / 1024_f64),
        ..AcquisitionLimits::default()
    };
    let policy = AcquisitionPolicy::from_hosts(Some(&hosts), true, None, false, limits).expect("construct bounded policy");
    let plan = AcquisitionPlan::build(
        vec!["binary".to_string()],
        AcquisitionSource::HttpBinary { sha256: sha256(&body), url },
        &policy,
    )
    .expect("plan bounded HTTP acquisition");
    let root = TestDirectory::create("response-limit");
    let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
    server.join().expect("join acquisition server");
    assert!(matches!(result, Err(AcquisitionError::Limit { .. })));
    assert!(!root.0.join("binary").join(plan.cache_key.as_deref().expect("remote cache key")).is_dir());
}
#[cfg(unix)]
#[test]
fn test_concurrent_git_acquisitions_publish_one_verified_entry() {
    let root = TestDirectory::create("git-concurrent-http");
    let commit = git_fixture(&root.0);
    let (url, finished, server) = serve_git_http(root.0.clone());
    let parsed = Url::parse(&url).expect("Git URL");
    let hosts = vec![format!("{}:{}", parsed.host_str().expect("Git host"), parsed.port().expect("Git port"))];
    let policy = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(vec!["git".to_string()], AcquisitionSource::Git { commit, url }, &policy).expect("plan concurrent Git fixture");
    let cache_root = root.0.join("cache");
    let executor = AcquisitionExecutor::new(policy, AcquisitionCache::new(cache_root.clone()));
    let barrier = Arc::new(Barrier::new(2));
    let acquired = (0..2)
        .map(|_| {
            let executor = executor.clone();
            let plan = plan.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                executor.execute(&plan).expect("concurrent Git acquisition")
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|worker| worker.join().expect("join Git acquisition"))
        .collect::<Vec<_>>();
    finished.store(true, Ordering::Release);
    assert!(server.join().expect("join Git fixture server") >= 2);
    assert_eq!(acquired[0].path, acquired[1].path);
    assert_eq!(read_dir(cache_root.join("git")).expect("read Git cache namespace").count(), 1);
}
#[cfg(unix)]
#[test]
fn test_git_checkout_rejects_symlinks_outside_the_staged_tree() {
    use std::os::unix::fs::symlink;

    let root = TestDirectory::create("git-escaping-symlink-http");
    git_fixture(&root.0);
    let work = root.0.join("work");
    let outside = root.0.join("outside");
    std::fs::write(&outside, b"outside").expect("write outside fixture");
    symlink(&outside, work.join("escape")).expect("create escaping Git symlink");
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&work)
            .args(args)
            .output()
            .expect("run Git fixture command");
        assert!(
            output.status.success(),
            "Git fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("Git fixture output").trim().to_string()
    };
    run(&["add", "escape"]);
    run(&["commit", "--quiet", "-m", "escaping link"]);
    let commit = run(&["rev-parse", "HEAD"]);
    run(&[
        "push",
        "--quiet",
        root.0.join("repo.git").to_str().expect("bare path"),
        "HEAD:refs/heads/main",
    ]);
    let (url, finished, server) = serve_git_http(root.0.clone());
    let parsed = Url::parse(&url).expect("Git URL");
    let hosts = vec![format!("{}:{}", parsed.host_str().expect("Git host"), parsed.port().expect("Git port"))];
    let policy = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(vec!["git".to_string()], AcquisitionSource::Git { commit, url }, &policy).expect("plan escaping Git fixture");
    let cache_root = root.0.join("cache");
    let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(cache_root.clone())).execute(&plan);
    finished.store(true, Ordering::Release);
    assert!(server.join().expect("join Git fixture server") >= 2);
    assert!(matches!(result, Err(AcquisitionError::Corrupt { .. })));
    assert_eq!(read_dir(cache_root.join("git")).expect("read Git cache namespace").count(), 0);
}
#[cfg(unix)]
#[test]
fn test_git_http_redirect_does_not_contact_an_unlisted_host() {
    with_loopback(|allowed| {
        let denied = TcpListener::bind("127.0.0.1:0").expect("bind denied Git destination");
        let allowed_address = allowed.local_addr().expect("allowed Git address");
        let denied_address = denied.local_addr().expect("denied Git address");
        let server = thread::spawn(move || {
            let (mut stream, _) = accept_http_request(&allowed).expect("Git redirect request");
            write_http_response(&mut stream, "302 Found", &format!("Location: http://{denied_address}/repo.git\r\n"), b"");
        });
        let hosts = vec![allowed_address.to_string()];
        let limits = AcquisitionLimits {
            git_timeout: Seconds::new(15),
            ..AcquisitionLimits::default()
        };
        let policy = AcquisitionPolicy::from_hosts(Some(&hosts), true, None, false, limits).expect("Git redirect policy");
        let plan = AcquisitionPlan::build(
            vec!["git".to_string()],
            AcquisitionSource::Git {
                commit: "a".repeat(40),
                url: format!("http://{allowed_address}/repo.git"),
            },
            &policy,
        )
        .expect("plan Git redirect fixture");
        let root = TestDirectory::create("git-redirect-denied");
        let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
        server.join().expect("join Git redirect fixture");
        denied.set_nonblocking(true).expect("inspect denied Git destination");
        assert!(matches!(result, Err(AcquisitionError::Process { .. })));
        assert!(matches!(denied.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(read_dir(root.0.join("git")).expect("read Git cache namespace").count(), 0);
    });
}
#[test]
fn test_git_origin_normalization_discards_query_and_fragment() {
    let normalized = normalize_origin("https://EXAMPLE.org/project.git/?token=hidden#fragment");
    assert!(normalized.is_ok_and(|origin| origin == "https://example.org/project.git"));
}
#[cfg(unix)]
#[test]
fn test_git_timeout_terminates_fetch_and_cleans_staging() {
    with_loopback(|listener| {
        let address = listener.local_addr().expect("stalling Git server address");
        let server = thread::spawn(move || {
            let received = accept_http_request(&listener).is_some();
            if received {
                thread::sleep(Duration::from_secs(12));
            }
            received
        });
        let hosts = vec![address.to_string()];
        let limits = AcquisitionLimits {
            git_timeout: Seconds::new(10),
            ..AcquisitionLimits::default()
        };
        let policy = AcquisitionPolicy::from_hosts(Some(&hosts), true, None, false, limits).expect("Git timeout policy");
        let plan = AcquisitionPlan::build(
            vec!["git".to_string()],
            AcquisitionSource::Git {
                commit: "a".repeat(40),
                url: format!("http://{address}/repo.git"),
            },
            &policy,
        )
        .expect("plan stalling Git source");
        let root = TestDirectory::create("git-timeout-cleanup");
        let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
        assert!(
            server.join().expect("join stalling Git server"),
            "Git timeout fixture received no request"
        );
        assert!(matches!(
            result,
            Err(AcquisitionError::Process { .. }) | Err(AcquisitionError::Limit { .. })
        ));
        assert_eq!(read_dir(root.0.join("git")).expect("read Git cache namespace").count(), 0);
    });
}
#[test]
fn test_git_timeout_uses_one_shared_deadline() {
    let started = Instant::now()
        .checked_sub(Duration::from_secs(2))
        .expect("construct expired Git deadline");
    assert!(remaining_timeout(started, Seconds::new(1)).is_err());
    assert!(remaining_timeout(Instant::now(), Seconds::new(10)).is_ok());
}
#[test]
fn test_git_tree_entry_limit_fails_closed() {
    let root = TestDirectory::create("git-tree-entry-limit");
    let repository = root.0.join("repository");
    create_dir(&repository).expect("create Git tree fixture");
    std::fs::write(repository.join("first.txt"), b"first").expect("write first Git tree entry");
    std::fs::write(repository.join("second.txt"), b"second").expect("write second Git tree entry");
    let limits = AcquisitionLimits {
        git_entries: 1,
        ..AcquisitionLimits::default()
    };
    assert!(matches!(
        inspect_tree(&repository, &limits),
        Err(AcquisitionError::Limit {
            limit: "Git tree entries",
            ..
        })
    ));
}
#[cfg(unix)]
#[test]
fn test_git_tree_limit_removes_staging_without_publishing() {
    let root = TestDirectory::create("git-tree-limit-http");
    let commit = git_fixture(&root.0);
    let (url, finished, server) = serve_git_http(root.0.clone());
    let parsed = Url::parse(&url).expect("Git URL");
    let hosts = vec![format!("{}:{}", parsed.host_str().expect("Git host"), parsed.port().expect("Git port"))];
    let limits = AcquisitionLimits {
        git_entries: 1,
        ..AcquisitionLimits::default()
    };
    let policy = AcquisitionPolicy::from_hosts(Some(&hosts), true, None, false, limits).expect("Git limit policy");
    let plan = AcquisitionPlan::build(vec!["git".to_string()], AcquisitionSource::Git { commit, url }, &policy).expect("plan bounded Git fixture");
    let cache_root = root.0.join("cache");
    let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(cache_root.clone())).execute(&plan);
    finished.store(true, Ordering::Release);
    assert!(server.join().expect("join Git fixture server") >= 2);
    assert!(matches!(
        result,
        Err(AcquisitionError::Limit {
            limit: "Git tree entries",
            ..
        })
    ));
    assert!(!cache_root.join("git").join(plan.cache_key.as_deref().expect("Git key")).exists());
    assert_eq!(read_dir(cache_root.join("git")).expect("read Git cache namespace").count(), 0);
}
#[cfg(unix)]
#[test]
fn test_git_verification_rejects_origin_mismatch() {
    let root = TestDirectory::create("git-origin-mismatch");
    let commit = git_fixture(&root.0);
    let work = root.0.join("work");
    let output = Command::new("git")
        .arg("-C")
        .arg(&work)
        .args(["remote", "add", "origin", "https://example.org/actual.git"])
        .output()
        .expect("set Git fixture origin");
    assert!(output.status.success());
    let policy = policy(&["example.org".to_string()], false, false);
    let environment = git_environment(&policy, &work).expect("construct isolated Git environment");
    let result = verify_git(
        &work,
        "https://example.org/expected.git",
        &commit,
        &environment,
        &Memory::kb(16),
        Instant::now(),
        Seconds::new(5),
    );
    assert!(matches!(
        result,
        Err(AcquisitionError::Process {
            action: "verify Git origin",
            ..
        })
    ));
}
#[cfg(unix)]
#[test]
fn test_pinned_git_http_acquisition_is_verified_and_reused_offline() {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDirectory::create("git-http-fixture");
    let commit = git_fixture(&root.0);
    let (url, finished, server) = serve_git_http(root.0.clone());
    let host = Url::parse(&url).expect("Git URL");
    let hosts = vec![format!("{}:{}", host.host_str().expect("Git host"), host.port().expect("Git port"))];
    let online = policy(&hosts, true, false);
    let plan = AcquisitionPlan::build(vec!["git".to_string()], AcquisitionSource::Git { commit, url }, &online).expect("plan Git fixture");
    let cache = AcquisitionCache::new(root.0.join("cache"));
    let first = AcquisitionExecutor::new(online, cache.clone())
        .execute(&plan)
        .expect("fetch pinned Git commit");
    finished.store(true, Ordering::Release);
    assert!(server.join().expect("join Git HTTP server") >= 2);
    assert_eq!(first.evidence.cache, AcquisitionCacheStatus::Miss);
    assert!(first.path.join("run.sh").is_file());
    assert_ne!(
        std::fs::metadata(first.path.join("run.sh"))
            .expect("Git script metadata")
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert_eq!(read_link(first.path.join("run-link")).expect("Git symlink"), PathBuf::from("run.sh"));
    assert!(!first.path.join(".git").exists());
    let second = AcquisitionExecutor::new(policy(&hosts, true, true), cache)
        .execute(&plan)
        .expect("reuse Git offline");
    assert_eq!(second.evidence.cache, AcquisitionCacheStatus::Hit);
}
#[cfg(unix)]
#[test]
fn test_pinned_git_https_fetch_uses_verified_tls_and_commit() {
    let root = TestDirectory::create("git-https-fixture");
    let commit = git_fixture(&root.0);
    let (url, finished, server) = serve_git_https(root.0.clone());
    let parsed = Url::parse(&url).expect("Git HTTPS URL");
    let hosts = vec![format!(
        "{}:{}",
        parsed.host_str().expect("Git HTTPS host"),
        parsed.port().expect("Git HTTPS port")
    )];
    let policy = policy(&hosts, false, false);
    AcquisitionPlan::build(
        vec!["git".to_string()],
        AcquisitionSource::Git {
            commit: commit.clone(),
            url: url.clone(),
        },
        &policy,
    )
    .expect("plan pinned HTTPS Git source");
    let repository = root.0.join("https-checkout");
    create_dir(&repository).expect("create HTTPS checkout");
    let started = Instant::now();
    let result = git_environment(&policy, &repository).and_then(|mut environment| {
        assert_eq!(environment.get("GIT_SSL_NO_VERIFY").map(String::as_str), Some("false"));
        environment.insert(
            "GIT_SSL_CAINFO".to_string(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/acquisition/fixtures/git-https-cert.pem").to_string(),
        );
        let context = GitCommandContext {
            environment: &environment,
            maximum_staging: policy.limits().git_staging.clone(),
            output: policy.limits().process_output.clone(),
            repository: &repository,
            url: &url,
        };
        [
            ("initialize HTTPS Git repository", args!["init", "--quiet"]),
            ("add HTTPS Git origin", args!["remote", "add", "origin", &url]),
            (
                "fetch pinned HTTPS Git commit",
                args![
                    ("-c", "credential.helper="),
                    ("-c", "core.hooksPath=/dev/null"),
                    ("-c", "fetch.recurseSubmodules=false"),
                    ("-c", "http.followRedirects=false"),
                    ("-c", "protocol.ext.allow=never"),
                    ("-c", "protocol.file.allow=never"),
                    "fetch",
                    "--depth=1",
                    "--no-tags",
                    "origin",
                    &commit,
                ],
            ),
            ("checkout HTTPS Git commit", args!["checkout", "--detach", "--quiet", "FETCH_HEAD"]),
        ]
        .into_iter()
        .try_for_each(|(action, arguments)| {
            remaining_timeout(started, policy.limits().git_timeout).and_then(|timeout| run_git_command(&context, action, arguments, timeout))
        })
        .and_then(|()| {
            verify_git(
                &repository,
                &url,
                &commit,
                &environment,
                &policy.limits().process_output,
                started,
                policy.limits().git_timeout,
            )
        })
    });
    finished.store(true, Ordering::Release);
    let requests = server.join().expect("join Git HTTPS server");
    result.expect("fetch pinned Git commit over verified HTTPS");
    assert!(requests >= 2, "Git HTTPS fixture received {requests} requests");
    assert!(repository.join("run.sh").is_file());
}
#[cfg(unix)]
#[test]
fn test_pinned_git_ssh_acquisition_uses_explicit_identity_and_known_hosts() {
    let root = TestDirectory::create("git-ssh-fixture");
    let commit = git_fixture(&root.0);
    let key = root.0.join("identity");
    let host_key = root.0.join("host-key");
    for path in [&key, &host_key] {
        let output = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .output()
            .expect("generate SSH fixture key");
        assert!(
            output.status.success(),
            "SSH fixture key generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let public_key = read(key.with_extension("pub")).expect("read SSH identity public key");
    let authorized = root.0.join("authorized_keys");
    std::fs::write(&authorized, public_key).expect("write SSH authorized key");
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve SSH fixture port");
    let port = listener.local_addr().expect("SSH fixture address").port();
    drop(listener);
    let known_hosts = root.0.join("known_hosts");
    let host_public = std::fs::read_to_string(host_key.with_extension("pub")).expect("read SSH host public key");
    let parts = host_public.split_whitespace().take(2).collect::<Vec<_>>();
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{port} {} {}\n", parts[0], parts[1])).expect("write SSH known hosts");
    let account = Command::new("id").arg("-un").output().expect("resolve SSH fixture account");
    assert!(account.status.success(), "resolve SSH fixture account failed");
    let user = String::from_utf8(account.stdout).expect("SSH fixture account is UTF-8");
    let user = user.trim();
    let config = root.0.join("sshd_config");
    std::fs::write(&config, format!(
        "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPidFile {}\nPasswordAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin prohibit-password\nStrictModes no\nUsePAM no\n",
        host_key.display(), authorized.display(), root.0.join("sshd.pid").display()
    )).expect("write SSH server config");
    let sshd = which::which("sshd").expect("OpenSSH server is required for Git acquisition qualification");
    let mut server = Command::new(sshd)
        .arg("-D")
        .arg("-f")
        .arg(&config)
        .arg("-E")
        .arg(root.0.join("sshd.log"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start SSH fixture server");
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() && started.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(20));
    }
    if TcpStream::connect(("127.0.0.1", port)).is_err() {
        let log = std::fs::read_to_string(root.0.join("sshd.log")).unwrap_or_default();
        let _ = server.kill();
        let output = server.wait_with_output().expect("collect SSH fixture failure");
        panic!(
            "SSH fixture server did not listen: {log}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let hosts = vec![format!("127.0.0.1:{port}")];
    let policy = AcquisitionPolicy::from_hosts(
        Some(&hosts),
        false,
        Some(GitSshPolicy { identity: key, known_hosts }),
        false,
        AcquisitionLimits::default(),
    )
    .expect("construct SSH Git policy");
    let bare = root.0.join("repo.git");
    let url = format!("ssh://{user}@127.0.0.1:{port}{}", bare.display());
    let plan = AcquisitionPlan::build(vec!["git".to_string()], AcquisitionSource::Git { commit, url }, &policy).expect("plan pinned SSH Git fixture");
    let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.join("cache"))).execute(&plan);
    let _ = server.kill();
    let _ = server.wait();
    assert!(result.expect("fetch pinned SSH Git fixture").path.join("run.sh").is_file());
}
#[test]
fn test_ssh_policy_disables_inherited_agents_and_uses_explicit_files() {
    let root = TestDirectory::create("ssh-policy");
    let identity = root.0.join("identity");
    let known_hosts = root.0.join("known-hosts");
    std::fs::write(&identity, b"fixture identity").expect("write identity fixture");
    std::fs::write(&known_hosts, b"fixture host key").expect("write known-hosts fixture");
    let hosts = vec!["example.org".to_string()];
    let policy = AcquisitionPolicy::from_hosts(
        Some(&hosts),
        false,
        Some(GitSshPolicy {
            identity: identity.clone(),
            known_hosts: known_hosts.clone(),
        }),
        false,
        AcquisitionLimits::default(),
    )
    .expect("construct SSH policy");
    let environment = git_environment(&policy, &root.0.join("repository")).expect("construct isolated Git environment");
    let command = environment.get("GIT_SSH_COMMAND").expect("SSH command");
    assert_eq!(environment.get("SSH_AGENT_PID").map(String::as_str), Some(""));
    assert_eq!(environment.get("SSH_AUTH_SOCK").map(String::as_str), Some(""));
    assert_eq!(environment.get("GIT_ALLOW_PROTOCOL").map(String::as_str), Some("https:ssh"));
    assert_eq!(environment.get("GIT_CONFIG_COUNT").map(String::as_str), Some("2"));
    assert_eq!(environment.get("GIT_CONFIG_KEY_0").map(String::as_str), Some("http.followRedirects"));
    assert_eq!(environment.get("GIT_CONFIG_VALUE_0").map(String::as_str), Some("false"));
    assert_eq!(environment.get("GIT_CONFIG_PARAMETERS").map(String::as_str), Some(""));
    assert_eq!(environment.get("HTTPS_PROXY").map(String::as_str), Some(""));
    assert!(Path::new(environment.get("GIT_TEMPLATE_DIR").expect("Git template directory")).is_dir());
    assert!(command.contains("IdentitiesOnly=yes"));
    assert!(command.contains("IdentityAgent=none"));
    let (_, arguments) = acorn_cmd::parse_sh(command).expect("parse SSH command");
    assert!(arguments.contains(&identity.display().to_string()));
    assert!(arguments.contains(&format!("UserKnownHostsFile={}", known_hosts.display())));
}
#[cfg(unix)]
#[test]
fn test_tree_inspection_rejects_escaping_symlinks() {
    use std::os::unix::fs::symlink;

    let root = TestDirectory::create("escaping-symlink");
    let repository = root.0.join("repository");
    create_dir(&repository).expect("create repository fixture");
    let outside = root.0.join("outside");
    std::fs::write(&outside, b"outside").expect("write outside fixture");
    symlink(&outside, repository.join("escape")).expect("create escaping symlink");
    let result = inspect_tree(&repository, &AcquisitionLimits::default());
    assert!(matches!(result, Err(AcquisitionError::Corrupt { .. })));
}
#[test]
fn test_concurrent_oci_acquisitions_publish_one_verified_entry() {
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let layout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle");
    let policy = policy(&[], false, false);
    let plan = AcquisitionPlan::build(
        vec!["oci".to_string()],
        AcquisitionSource::Oci {
            digest: digest.to_string(),
            reference: format!("oci-layout:{}@{digest}", layout.display()),
        },
        &policy,
    )
    .expect("plan concurrent OCI fixture");
    let root = TestDirectory::create("oci-concurrent");
    let executor = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone()));
    let barrier = Arc::new(Barrier::new(2));
    let acquired = (0..2)
        .map(|_| {
            let executor = executor.clone();
            let plan = plan.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                executor.execute(&plan).expect("concurrent OCI acquisition")
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|worker| worker.join().expect("join OCI acquisition"))
        .collect::<Vec<_>>();
    assert_eq!(acquired[0].path, acquired[1].path);
    assert_eq!(read_dir(root.0.join("oci-layout")).expect("read OCI layout cache namespace").count(), 1);
}
#[test]
fn test_local_oci_layout_is_verified_and_reused_offline() {
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let layout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle");
    let source = AcquisitionSource::Oci {
        digest: digest.to_string(),
        reference: format!("oci-layout:{}@{digest}", layout.display()),
    };
    let online = policy(&[], false, false);
    let plan = AcquisitionPlan::build(vec!["oci".to_string()], source, &online).expect("plan local OCI layout");
    let cache = TestDirectory::create("local-oci-cache");
    let cache = AcquisitionCache::new(cache.0.clone());
    let first = AcquisitionExecutor::new(online, cache.clone())
        .execute(&plan)
        .expect("acquire local OCI layout");
    assert_eq!(first.evidence.cache, AcquisitionCacheStatus::Miss);
    assert!(first.path.join("run.sh").is_file());
    let second = AcquisitionExecutor::new(policy(&[], false, true), cache)
        .execute(&plan)
        .expect("reuse OCI layout offline");
    assert_eq!(second.evidence.cache, AcquisitionCacheStatus::Hit);
    assert_eq!(first.path, second.path);
}
#[test]
fn test_oci_bearer_realm_and_blob_redirect_follow_explicit_host_policy() {
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let blob = "sha256:d74bed91bbc99064053346c46343df89ff3333c14447b7479ba1de142a8e662e";
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle/blobs/sha256");
    let manifest = read(bundle.join(digest.strip_prefix("sha256:").expect("manifest digest"))).expect("read OCI manifest fixture");
    let layer = read(bundle.join(blob.strip_prefix("sha256:").expect("layer digest"))).expect("read OCI layer fixture");
    with_loopback(|registry| {
        let token = TcpListener::bind("127.0.0.1:0").expect("bind token fixture");
        let cdn = TcpListener::bind("127.0.0.1:0").expect("bind blob redirect fixture");
        let registry_address = registry.local_addr().expect("registry address");
        let token_address = token.local_addr().expect("token address");
        let cdn_address = cdn.local_addr().expect("CDN address");
        let registry_server = thread::spawn(move || {
            let (mut first, request) = accept_http_request(&registry).expect("first registry request");
            assert!(request.contains("/manifests/"));
            write_http_response(
                &mut first,
                "401 Unauthorized",
                &format!("WWW-Authenticate: Bearer realm=\"http://{token_address}/token\",service=\"fixture\",scope=\"repository:demo:pull\"\r\n"),
                b"",
            );
            let (mut second, request) = accept_http_request(&registry).expect("authorized registry request");
            assert!(request.contains("Authorization: Bearer fixture-token") || request.contains("authorization: Bearer fixture-token"));
            write_http_response(&mut second, "200 OK", "", &manifest);
            let (mut third, request) = accept_http_request(&registry).expect("registry blob request");
            assert!(request.contains("/blobs/"));
            write_http_response(&mut third, "302 Found", &format!("Location: http://{cdn_address}/blob\r\n"), b"");
        });
        let token_server = thread::spawn(move || {
            let (mut stream, request) = accept_http_request(&token).expect("token request");
            assert!(request.contains("/token?service=fixture&scope=repository%3Ademo%3Apull"));
            write_http_response(
                &mut stream,
                "200 OK",
                "Content-Type: application/json\r\n",
                br#"{"token":"fixture-token"}"#,
            );
        });
        let cdn_server = thread::spawn(move || {
            let (mut stream, request) = accept_http_request(&cdn).expect("redirected blob request");
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            write_http_response(&mut stream, "200 OK", "", &layer);
        });
        let hosts = vec![registry_address.to_string(), token_address.to_string(), cdn_address.to_string()];
        let policy = policy(&hosts, true, false);
        let plan = AcquisitionPlan::build(
            vec!["oci".to_string()],
            AcquisitionSource::Oci {
                digest: digest.to_string(),
                reference: format!("oci://{registry_address}/demo@{digest}"),
            },
            &policy,
        )
        .expect("plan authorized OCI registry");
        let root = TestDirectory::create("oci-auth-redirect");
        let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
        registry_server.join().expect("join registry fixture");
        token_server.join().expect("join token fixture");
        cdn_server.join().expect("join CDN fixture");
        assert!(result.expect("acquire authorized OCI artifact").path.join("run.sh").is_file());
    });
}
#[test]
fn test_oci_denied_bearer_realm_is_never_contacted() {
    with_loopback(|registry| {
        let denied = TcpListener::bind("127.0.0.1:0").expect("bind denied token fixture");
        let registry_address = registry.local_addr().expect("registry address");
        let denied_address = denied.local_addr().expect("denied token address");
        let server = thread::spawn(move || {
            let (mut stream, _) = accept_http_request(&registry).expect("registry request");
            write_http_response(
                &mut stream,
                "401 Unauthorized",
                &format!("WWW-Authenticate: Bearer realm=\"http://{denied_address}/token\"\r\n"),
                b"",
            );
        });
        let hosts = vec![registry_address.to_string()];
        let policy = policy(&hosts, true, false);
        let digest = format!("sha256:{}", "a".repeat(64));
        let plan = AcquisitionPlan::build(
            vec!["oci".to_string()],
            AcquisitionSource::Oci {
                digest: digest.clone(),
                reference: format!("oci://{registry_address}/demo@{digest}"),
            },
            &policy,
        )
        .expect("plan denied-realm OCI registry");
        let root = TestDirectory::create("oci-denied-realm");
        let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
        server.join().expect("join registry fixture");
        denied.set_nonblocking(true).expect("inspect denied token fixture");
        assert!(matches!(result, Err(AcquisitionError::Policy { .. })));
        assert!(matches!(denied.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(read_dir(root.0.join("oci")).expect("read OCI cache namespace").count(), 0);
    });
}
#[test]
fn test_oci_denied_blob_redirect_is_never_contacted() {
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle/blobs/sha256");
    let manifest = read(bundle.join(digest.strip_prefix("sha256:").expect("manifest digest"))).expect("read OCI manifest fixture");
    with_loopback(|registry| {
        let denied = TcpListener::bind("127.0.0.1:0").expect("bind denied blob fixture");
        let registry_address = registry.local_addr().expect("registry address");
        let denied_address = denied.local_addr().expect("denied blob address");
        let server = thread::spawn(move || {
            let (mut first, _) = accept_http_request(&registry).expect("manifest request");
            write_http_response(&mut first, "200 OK", "", &manifest);
            let (mut second, _) = accept_http_request(&registry).expect("blob request");
            write_http_response(&mut second, "302 Found", &format!("Location: http://{denied_address}/blob\r\n"), b"");
        });
        let hosts = vec![registry_address.to_string()];
        let policy = policy(&hosts, true, false);
        let plan = AcquisitionPlan::build(
            vec!["oci".to_string()],
            AcquisitionSource::Oci {
                digest: digest.to_string(),
                reference: format!("oci://{registry_address}/demo@{digest}"),
            },
            &policy,
        )
        .expect("plan denied-redirect OCI registry");
        let root = TestDirectory::create("oci-denied-redirect");
        let result = AcquisitionExecutor::new(policy, AcquisitionCache::new(root.0.clone())).execute(&plan);
        server.join().expect("join registry fixture");
        denied.set_nonblocking(true).expect("inspect denied blob fixture");
        assert!(matches!(result, Err(AcquisitionError::Policy { .. })));
        assert!(matches!(denied.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(read_dir(root.0.join("oci")).expect("read OCI cache namespace").count(), 0);
    });
}
#[test]
fn test_oci_manifest_rejects_unsafe_file_layers_and_oversized_artifacts() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/oci-local/module/bundle/blobs/sha256/98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805");
    let bytes = read(path).expect("read OCI manifest fixture");
    let limits = AcquisitionLimits::default();
    assert!(validate_manifest_bytes(&bytes, &limits).is_ok());
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).expect("parse OCI manifest fixture");
    manifest["layers"][0]["annotations"]["org.opencontainers.image.title"] = serde_json::json!("../escape");
    assert!(matches!(
        validate_manifest_bytes(manifest.to_string().as_bytes(), &limits),
        Err(AcquisitionError::Identity { .. })
    ));
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).expect("parse OCI manifest fixture");
    manifest["layers"][0]["mediaType"] = serde_json::json!("application/x-unsupported");
    assert!(matches!(
        validate_manifest_bytes(manifest.to_string().as_bytes(), &limits),
        Err(AcquisitionError::Identity { .. })
    ));
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).expect("parse OCI manifest fixture");
    let duplicate = manifest["layers"][0].clone();
    manifest["layers"].as_array_mut().expect("OCI layers").push(duplicate);
    assert!(matches!(
        validate_manifest_bytes(manifest.to_string().as_bytes(), &limits),
        Err(AcquisitionError::Identity { .. })
    ));
    let limits = AcquisitionLimits {
        git_staging: Memory::kb(1_f64 / 1024_f64),
        ..limits
    };
    assert!(matches!(validate_manifest_bytes(&bytes, &limits), Err(AcquisitionError::Limit { .. })));
}
#[test]
fn test_oci_request_policy_checks_every_destination() {
    let policy = policy(&["registry.example".to_string(), "tokens.example".to_string()], false, false);
    let registry = Url::parse("https://registry.example/v2/project/manifests/sha256:a").expect("registry URL");
    let token = Url::parse("https://tokens.example/token?service=registry.example").expect("token URL");
    let denied = Url::parse("https://other.example/blob").expect("denied URL");
    let downgrade = Url::parse("http://tokens.example/token").expect("downgrade URL");
    assert!(policy.validate_oci_request(&registry, None).is_ok());
    assert!(policy.validate_oci_request(&token, Some(&registry)).is_ok());
    assert!(matches!(
        policy.validate_oci_request(&denied, Some(&registry)),
        Err(AcquisitionError::Policy { .. })
    ));
    assert!(matches!(
        policy.validate_oci_request(&downgrade, Some(&registry)),
        Err(AcquisitionError::Policy { .. })
    ));
}
#[test]
fn test_registry_oci_layout_is_fetched_through_policy_and_reused_offline() {
    let digest = "sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805";
    let blob = "sha256:d74bed91bbc99064053346c46343df89ff3333c14447b7479ba1de142a8e662e";
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/oci-local/module/bundle/blobs/sha256");
    let manifest = read(bundle.join(digest.strip_prefix("sha256:").expect("manifest digest"))).expect("read OCI manifest fixture");
    let layer = read(bundle.join(blob.strip_prefix("sha256:").expect("layer digest"))).expect("read OCI layer fixture");
    with_loopback(|listener| {
        let address = listener.local_addr().expect("OCI registry address");
        let server = thread::spawn(move || {
            listener.set_nonblocking(true).expect("set OCI registry nonblocking");
            let started = Instant::now();
            let mut paths = Vec::new();
            while paths.len() < 2 && started.elapsed() < Duration::from_secs(5) {
                match listener.accept() {
                    | Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).expect("set OCI request blocking");
                        let mut request = [0_u8; 2048];
                        let length = stream.read(&mut request).expect("read OCI request");
                        let line = String::from_utf8_lossy(&request[..length]);
                        let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                        let body = if path.contains("/manifests/") { &manifest } else { &layer };
                        let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        stream.write_all(header.as_bytes()).expect("write OCI response header");
                        stream.write_all(body).expect("write OCI response body");
                        paths.push(path);
                    }
                    | Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                    | Err(error) => panic!("accept OCI request: {error}"),
                }
            }
            paths
        });
        let hosts = vec![address.to_string()];
        let online = policy(&hosts, true, false);
        let plan = AcquisitionPlan::build(
            vec!["oci".to_string()],
            AcquisitionSource::Oci {
                digest: digest.to_string(),
                reference: format!("oci://{address}/demo@{digest}"),
            },
            &online,
        )
        .expect("plan registry OCI artifact");
        let cache = TestDirectory::create("registry-oci-cache");
        let cache = AcquisitionCache::new(cache.0.clone());
        let first = AcquisitionExecutor::new(online, cache.clone())
            .execute(&plan)
            .expect("acquire registry OCI artifact");
        assert_eq!(first.evidence.cache, AcquisitionCacheStatus::Miss);
        assert!(first.path.join("run.sh").is_file());
        assert_eq!(server.join().expect("join OCI registry").len(), 2);
        let second = AcquisitionExecutor::new(policy(&hosts, true, true), cache)
            .execute(&plan)
            .expect("reuse registry OCI artifact offline");
        assert_eq!(second.evidence.cache, AcquisitionCacheStatus::Hit);
    });
}
#[cfg(unix)]
#[test]
fn test_timed_out_oras_process_is_terminated() {
    let root = TestDirectory::create("oras-timeout");
    let destination = root.0.join("repository");
    create_dir(&destination).expect("create ORAS test destination");
    let started = Instant::now();
    let result = run_oras(
        "sh",
        "qualify ORAS cancellation",
        "local OCI fixture",
        args!["-c", "sleep 5"],
        &AcquisitionLimits {
            git_staging: Memory::mb(1),
            process_output: Memory::kb(1),
            ..AcquisitionLimits::default()
        },
        Duration::from_millis(100),
        &destination,
    );
    assert!(matches!(result, Err(AcquisitionError::Process { .. })));
    assert!(started.elapsed() < Duration::from_secs(2));
}
