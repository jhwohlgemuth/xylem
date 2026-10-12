use super::git::{git_environment, inspect_tree};
use super::util::sha256;
use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

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

#[test]
fn cache_reuses_verified_http_payload_offline() {
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
fn corrupted_cache_entry_fails_closed_offline() {
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
fn executor_rejects_a_tampered_plan_before_materialization() {
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
fn limits_can_only_tighten() {
    let parent = AcquisitionLimits::default();
    let child = AcquisitionLimits {
        binary: Memory::mb(256),
        git_entries: parent.git_entries * 2,
        ..parent.clone()
    };
    let effective = parent.restrict(&child);
    assert_eq!(effective.binary, child.binary);
    assert_eq!(effective.git_entries, parent.git_entries);
}
#[test]
fn offline_cache_miss_does_not_connect() {
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
fn planner_rejects_ambiguous_or_mutable_remote_sources() {
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
fn policy_is_fail_closed_and_schemes_require_operator_capabilities() {
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
fn response_limit_prevents_cache_publication() {
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
#[test]
fn ssh_policy_disables_inherited_agents_and_uses_explicit_files() {
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
    assert!(command.contains("IdentitiesOnly=yes"));
    assert!(command.contains("IdentityAgent=none"));
    let (_, arguments) = acorn_cmd::parse_sh(command).expect("parse SSH command");
    assert!(arguments.contains(&identity.display().to_string()));
    assert!(arguments.contains(&format!("UserKnownHostsFile={}", known_hosts.display())));
}
#[cfg(unix)]
#[test]
fn tree_inspection_rejects_escaping_symlinks() {
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
