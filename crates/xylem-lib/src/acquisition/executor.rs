use super::cache::CacheManifest;
use super::git::{git_environment, inspect_tree, remaining_timeout, run_git_command, verify_git, GitCommandContext};
use super::http::{request_error, stream_response};
use super::oci::pull_tree;
use super::policy::normalize_origin;
use super::util::{io_error, now_seconds};
use super::*;

impl AcquisitionExecutor {
    /// Construct an executor from a resolved policy and immutable cache.
    pub fn new(policy: AcquisitionPolicy, cache: AcquisitionCache) -> Self {
        Self { cache, policy }
    }
    /// Materialize one remote plan or return a verified immutable cache hit.
    pub fn execute(&self, plan: &AcquisitionPlan) -> Result<AcquisitionResult, AcquisitionError> {
        self.policy.validate_plan(plan).and_then(|()| match plan.action {
            | AcquisitionAction::UseLocal => Err(AcquisitionError::Identity {
                value: plan.source.to_string(),
                reason: "local sources are not executed by the acquisition cache",
            }),
            | AcquisitionAction::Download if matches!(&plan.source, AcquisitionSource::HttpBinaryUnverified { .. }) => match self.policy.offline {
                | true => Err(AcquisitionError::Offline {
                    remote: plan.source.to_string(),
                }),
                | false => self.download(plan),
            },
            | AcquisitionAction::Download | AcquisitionAction::FetchGit | AcquisitionAction::FetchOci => {
                self.cache.lookup(plan).and_then(|entry| match entry {
                    | Some(entry) => Ok(entry.result(plan, &self.policy, AcquisitionCacheStatus::Hit)),
                    | None if self.policy.offline => Err(AcquisitionError::Offline {
                        remote: plan.source.to_string(),
                    }),
                    | None => match plan.action {
                        | AcquisitionAction::Download => self.download(plan),
                        | AcquisitionAction::FetchGit => self.fetch_git(plan),
                        | AcquisitionAction::FetchOci => self.fetch_oci(plan),
                        | AcquisitionAction::UseLocal => unreachable!("local acquisition returned before cache lookup"),
                    },
                })
            }
        })
    }
    fn download(&self, plan: &AcquisitionPlan) -> Result<AcquisitionResult, AcquisitionError> {
        let request = match &plan.source {
            | AcquisitionSource::HttpBinary { sha256, url } => Ok((Some(sha256.as_str()), &plan.limits.binary, url)),
            | AcquisitionSource::HttpBinaryUnverified { url } => Ok((None, &plan.limits.binary, url)),
            | AcquisitionSource::HttpDocument { sha256, url } => Ok((Some(sha256.as_str()), &plan.limits.document, url)),
            | AcquisitionSource::Git { .. } | AcquisitionSource::Oci { .. } | AcquisitionSource::Local { .. } => Err(AcquisitionError::Identity {
                value: plan.source.to_string(),
                reason: "HTTP executor received a non-HTTP source",
            }),
        };
        request.and_then(|(expected, maximum, url)| {
            match expected {
                | Some(_) => self.cache.staging(plan),
                | None => {
                    tracing::warn!(
                        module_path = ?plan.module_path,
                        remote = %redact_url(url),
                        "checksum verification disabled — I hope you know what you're doing"
                    );
                    self.cache.staging_unverified(&plan.source)
                }
            }
            .and_then(|staging| {
                let payload = staging.path.join("payload");
                self.policy
                    .http_client()
                    .and_then(|client| {
                        client
                            .get(url)
                            .header(ACCEPT_ENCODING, "identity")
                            .send()
                            .map_err(|source| request_error("request remote source", url, source))
                    })
                    .and_then(|response| stream_response(response, url, &payload, maximum.checked_bytes().unwrap_or_default(), expected))
                    .and_then(|(bytes, calculated)| {
                        match expected {
                            | Some(_) => Ok(plan.clone()),
                            | None => AcquisitionPlan::build(
                                plan.module_path.clone(),
                                AcquisitionSource::HttpBinary {
                                    sha256: calculated.clone(),
                                    url: url.clone(),
                                },
                                &self.policy,
                            ),
                        }
                        .and_then(|resolved| {
                            let manifest = CacheManifest {
                                acquired_at_unix_seconds: now_seconds(),
                                bytes,
                                entries: 1,
                                identity: resolved.identity.clone().unwrap_or(AcquisitionIdentity::Sha256 { digest: calculated }),
                                source: redact_url(url),
                                tree_digest: None,
                                version: ACQUISITION_CACHE_VERSION,
                            };
                            self.cache
                                .publish(&resolved, staging, manifest)
                                .map(|entry| entry.result(&resolved, &self.policy, AcquisitionCacheStatus::Miss))
                        })
                    })
            })
        })
    }
    fn fetch_git(&self, plan: &AcquisitionPlan) -> Result<AcquisitionResult, AcquisitionError> {
        let request = match &plan.source {
            | AcquisitionSource::Git { commit, url } => Ok((commit, url)),
            | AcquisitionSource::HttpBinary { .. }
            | AcquisitionSource::HttpBinaryUnverified { .. }
            | AcquisitionSource::HttpDocument { .. }
            | AcquisitionSource::Oci { .. }
            | AcquisitionSource::Local { .. } => Err(AcquisitionError::Identity {
                value: plan.source.to_string(),
                reason: "Git executor received a non-Git source",
            }),
        };
        request.and_then(|(commit, url)| {
            self.cache.staging(plan).and_then(|staging| {
                let repository = staging.path.join("repository");
                create_dir(&repository)
                    .map_err(|source| io_error("create Git staging repository", repository.clone(), source))
                    .and_then(|()| self.run_git(plan, &repository, url, commit))
                    .and_then(|()| {
                        let environment = staging.path.join("git-environment");
                        match environment.try_exists() {
                            | Ok(true) => {
                                remove_dir_all(&environment).map_err(|source| io_error("remove Git acquisition environment", environment, source))
                            }
                            | Ok(false) => Ok(()),
                            | Err(source) => Err(io_error("inspect Git acquisition environment", environment, source)),
                        }
                    })
                    .and_then(|()| inspect_tree(&repository, &plan.limits))
                    .and_then(|(bytes, entries, tree_digest)| {
                        let git = repository.join(".git");
                        remove_dir_all(&git)
                            .map_err(|source| io_error("remove Git metadata", git, source))
                            .map(|()| (bytes, entries, tree_digest))
                    })
                    .and_then(|(bytes, entries, tree_digest)| {
                        let manifest = CacheManifest {
                            acquired_at_unix_seconds: now_seconds(),
                            bytes,
                            entries,
                            identity: plan.identity.clone().unwrap_or_else(|| AcquisitionIdentity::GitCommit {
                                commit: commit.to_string(),
                                origin: normalize_origin(url).unwrap_or_else(|_| redact_url(url)),
                            }),
                            source: redact_url(url),
                            tree_digest: Some(tree_digest),
                            version: ACQUISITION_CACHE_VERSION,
                        };
                        self.cache
                            .publish(plan, staging, manifest)
                            .map(|entry| entry.result(plan, &self.policy, AcquisitionCacheStatus::Miss))
                    })
            })
        })
    }
    fn fetch_oci(&self, plan: &AcquisitionPlan) -> Result<AcquisitionResult, AcquisitionError> {
        match &plan.source {
            | AcquisitionSource::Oci { digest, reference } => {
                let started = Instant::now();
                self.cache.staging(plan).and_then(|staging| {
                    let repository = staging.path.join("repository");
                    create_dir(&repository)
                        .map_err(|source| io_error("create OCI staging tree", repository.clone(), source))
                        .and_then(|()| {
                            pull_tree(
                                reference,
                                &repository,
                                &plan.limits.git_staging,
                                plan.limits.git_entries,
                                &plan.limits.process_output,
                                started,
                                plan.limits.git_timeout,
                                self.policy.allow_insecure_http,
                            )
                        })
                        .and_then(|()| inspect_tree(&repository, &plan.limits))
                        .and_then(|(bytes, entries, tree_digest)| {
                            let manifest = CacheManifest {
                                acquired_at_unix_seconds: now_seconds(),
                                bytes,
                                entries,
                                identity: AcquisitionIdentity::OciDigest { digest: digest.to_string() },
                                source: reference.to_string(),
                                tree_digest: Some(tree_digest),
                                version: ACQUISITION_CACHE_VERSION,
                            };
                            self.cache
                                .publish(plan, staging, manifest)
                                .map(|entry| entry.result(plan, &self.policy, AcquisitionCacheStatus::Miss))
                        })
                })
            }
            | _ => Err(AcquisitionError::Identity {
                value: plan.source.to_string(),
                reason: "OCI executor received a non-OCI source",
            }),
        }
    }
    fn run_git(&self, plan: &AcquisitionPlan, repository: &Path, url: &str, commit: &str) -> Result<(), AcquisitionError> {
        let started = Instant::now();
        let environment = git_environment(&self.policy, repository);
        let commands = [
            ("initialize Git repository", args!["init", "--quiet"]),
            ("add Git origin", args!["remote", "add", "origin", url]),
            (
                "fetch Git commit",
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
                    commit,
                ],
            ),
            ("checkout Git commit", args!["checkout", "--detach", "--quiet", "FETCH_HEAD"]),
        ];
        environment.and_then(|environment| {
            let context = GitCommandContext {
                environment: &environment,
                maximum_staging: plan.limits.git_staging.clone(),
                output: plan.limits.process_output.clone(),
                repository,
                url,
            };
            commands
                .into_iter()
                .try_for_each(|(action, arguments)| {
                    remaining_timeout(started, plan.limits.git_timeout).and_then(|timeout| run_git_command(&context, action, arguments, timeout))
                })
                .and_then(|()| verify_git(repository, url, commit, &plan.limits.process_output, started, plan.limits.git_timeout))
        })
    }
}
