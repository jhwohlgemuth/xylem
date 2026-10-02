use alloc::collections::{BTreeMap, BTreeSet};
use std::fs::read_to_string;
use std::process::{Command, Stdio};

const BASELINE_PATH: &str = "security/dependency-baseline.env";
const DENY_PATH: &str = "deny.toml";
const LEDGER_PATH: &str = "security/advisory-exceptions.tsv";
const LOCK_PATH: &str = "Cargo.lock";
const MANIFEST_PATH: &str = "Cargo.toml";

#[derive(Debug, PartialEq, Eq)]
struct Baseline {
    acorn_revision: String,
    forbidden_packages: Vec<String>,
    forbidden_prefixes: Vec<String>,
    max_duplicate_names: usize,
    max_locked_packages: usize,
    required_acorn_crates: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Exception {
    advisory: String,
    owner: String,
    rationale: String,
    reachability: String,
    removal_condition: String,
    review_by: String,
    upstream: String,
}

pub(crate) fn audit(offline: bool) -> Result<(), String> {
    check()
        .and_then(|()| run("cargo", &["shear", "--locked"]))
        .and_then(|()| {
            let arguments = match offline {
                | true => vec!["deny", "--offline", "--locked", "check"],
                | false => vec!["deny", "--locked", "check"],
            };
            run("cargo", &arguments)
        })
        .and_then(|()| run("cargo", &["test", "--locked", "--package", "xtask"]))
        .and_then(|()| audit_rustsec(offline))
}

pub(crate) fn check() -> Result<(), String> {
    run_quiet("cargo", &["metadata", "--locked", "--offline", "--format-version", "1"])
        .and_then(|()| read(BASELINE_PATH).and_then(|content| parse_baseline(&content)))
        .and_then(|baseline| {
            read(LOCK_PATH).and_then(|lock| {
                read(MANIFEST_PATH).and_then(|manifest| validate_dependencies(&baseline, &lock, &manifest))
            })
        })
        .and_then(|(locked_packages, duplicate_names)| {
            today().and_then(|today| {
                read(LEDGER_PATH).and_then(|ledger| {
                    read(DENY_PATH).and_then(|deny| {
                        validate_exceptions(&ledger, &deny, &today).map(|exceptions| {
                            println!(
                                "dependency policy: {locked_packages} locked packages, {duplicate_names} duplicate names, {} reviewed advisory exceptions",
                                exceptions.len()
                            );
                        })
                    })
                })
            })
        })
}

fn advisory_ids(content: &str) -> BTreeSet<String> {
    content
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .filter(|token| {
            token.len() == 17
                && token.starts_with("RUSTSEC-")
                && token[8..12].chars().all(|character| character.is_ascii_digit())
                && token.as_bytes().get(12) == Some(&b'-')
                && token[13..].chars().all(|character| character.is_ascii_digit())
        })
        .map(str::to_string)
        .collect()
}

fn audit_rustsec(offline: bool) -> Result<(), String> {
    read(LEDGER_PATH).and_then(|ledger| {
        parse_exceptions(&ledger).and_then(|exceptions| {
            let mut arguments = vec!["audit"];
            if offline {
                arguments.push("--no-fetch");
            }
            let ignores = exceptions
                .iter()
                .flat_map(|exception| ["--ignore", exception.advisory.as_str()])
                .collect::<Vec<_>>();
            arguments.extend(ignores);
            run("cargo", &arguments)
        })
    })
}

fn parse_baseline(content: &str) -> Result<Baseline, String> {
    let values = content
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim().trim_matches('"')))
        .collect::<BTreeMap<_, _>>();
    let get = |key| values.get(key).copied().ok_or_else(|| format!("{BASELINE_PATH} is missing {key}"));
    get("ACORN_REVISION").and_then(|acorn_revision| {
        get("FORBIDDEN_LOCKED_PACKAGES").and_then(|forbidden| {
            get("FORBIDDEN_LOCKED_PACKAGE_PREFIXES").and_then(|prefixes| {
                get("MAX_DUPLICATE_PACKAGE_NAMES").and_then(|duplicates| {
                    get("MAX_LOCKED_PACKAGES").and_then(|packages| {
                        get("REQUIRED_ACORN_CRATES").and_then(|required| {
                            duplicates
                                .parse::<usize>()
                                .map_err(|error| format!("invalid duplicate-name baseline: {error}"))
                                .and_then(|max_duplicate_names| {
                                    packages
                                        .parse::<usize>()
                                        .map_err(|error| format!("invalid package baseline: {error}"))
                                        .map(|max_locked_packages| Baseline {
                                            acorn_revision: acorn_revision.to_string(),
                                            forbidden_packages: forbidden.split_whitespace().map(str::to_string).collect(),
                                            forbidden_prefixes: prefixes.split_whitespace().map(str::to_string).collect(),
                                            max_duplicate_names,
                                            max_locked_packages,
                                            required_acorn_crates: required.split_whitespace().map(str::to_string).collect(),
                                        })
                                })
                        })
                    })
                })
            })
        })
    })
}

fn parse_exceptions(content: &str) -> Result<Vec<Exception>, String> {
    content
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            match fields.as_slice() {
                | [advisory, owner, review_by, reachability, upstream, removal_condition, rationale] => Ok(Exception {
                    advisory: (*advisory).to_string(),
                    owner: (*owner).to_string(),
                    rationale: (*rationale).to_string(),
                    reachability: (*reachability).to_string(),
                    removal_condition: (*removal_condition).to_string(),
                    review_by: (*review_by).to_string(),
                    upstream: (*upstream).to_string(),
                }),
                | _ => Err(format!("invalid advisory exception row: {line}")),
            }
        })
        .collect()
}

fn read(path: &str) -> Result<String, String> {
    read_to_string(path).map_err(|error| format!("read {path}: {error}"))
}

fn run(program: &str, arguments: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(arguments)
        .status()
        .map_err(|error| format!("run {program}: {error}"))
        .and_then(|status| match status.success() {
            | true => Ok(()),
            | false => Err(format!("{program} {} failed with {status}", arguments.join(" "))),
        })
}

fn run_quiet(program: &str, arguments: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(arguments)
        .stdout(Stdio::null())
        .status()
        .map_err(|error| format!("run {program}: {error}"))
        .and_then(|status| match status.success() {
            | true => Ok(()),
            | false => Err(format!("{program} {} failed with {status}", arguments.join(" "))),
        })
}

fn today() -> Result<String, String> {
    Command::new("date")
        .args(["-u", "+%F"])
        .output()
        .map_err(|error| format!("read current date: {error}"))
        .and_then(|output| match output.status.success() {
            | true => String::from_utf8(output.stdout)
                .map(|date| date.trim().to_string())
                .map_err(|error| error.to_string()),
            | false => Err("date -u +%F failed".to_string()),
        })
}

fn validate_dependencies(baseline: &Baseline, lock: &str, manifest: &str) -> Result<(usize, usize), String> {
    let names = lock
        .lines()
        .filter_map(|line| line.trim().strip_prefix("name = \"").and_then(|name| name.strip_suffix('"')))
        .collect::<Vec<_>>();
    let counts = names.iter().fold(BTreeMap::new(), |mut counts, name| {
        *counts.entry(*name).or_insert(0_usize) += 1;
        counts
    });
    let duplicate_names = counts.values().filter(|count| **count > 1).count();
    if names.len() > baseline.max_locked_packages {
        return Err(format!(
            "Cargo.lock grew from {} to {} packages",
            baseline.max_locked_packages,
            names.len()
        ));
    }
    if duplicate_names > baseline.max_duplicate_names {
        return Err(format!(
            "duplicate package names grew from {} to {duplicate_names}",
            baseline.max_duplicate_names
        ));
    }
    if let Some(package) = names
        .iter()
        .find(|name| baseline.forbidden_packages.iter().any(|forbidden| *name == forbidden))
    {
        return Err(format!("forbidden locked package found: {package}"));
    }
    if let Some(package) = names
        .iter()
        .find(|name| baseline.forbidden_prefixes.iter().any(|prefix| name.starts_with(prefix)))
    {
        return Err(format!("forbidden locked package found: {package}"));
    }
    baseline
        .required_acorn_crates
        .iter()
        .try_for_each(|name| {
            let expected = format!(
                "{name} = {{ git = \"https://code.ornl.gov/research-enablement/acorn.git\", rev = \"{}\"",
                baseline.acorn_revision
            );
            match manifest.contains(&expected) {
                | true => Ok(()),
                | false => Err(format!("{name} must remain pinned to ACORN revision {}", baseline.acorn_revision)),
            }
        })
        .map(|()| (names.len(), duplicate_names))
}

fn validate_exceptions(ledger: &str, deny: &str, today: &str) -> Result<Vec<Exception>, String> {
    parse_exceptions(ledger).and_then(|exceptions| {
        exceptions
            .iter()
            .try_for_each(|exception| {
                if !advisory_ids(&exception.advisory).contains(&exception.advisory) {
                    return Err(format!("invalid advisory exception ID: {}", exception.advisory));
                }
                if !exception.owner.starts_with('@') {
                    return Err(format!("{} must have an owning account", exception.advisory));
                }
                if exception.review_by.len() != 10
                    || exception.review_by.as_bytes().get(4) != Some(&b'-')
                    || exception.review_by.as_bytes().get(7) != Some(&b'-')
                {
                    return Err(format!("{} has an invalid review date", exception.advisory));
                }
                for (name, value) in [
                    ("reachability", &exception.reachability),
                    ("upstream tracking", &exception.upstream),
                    ("removal condition", &exception.removal_condition),
                    ("rationale", &exception.rationale),
                ] {
                    if value.trim().is_empty() {
                        return Err(format!("{} must document {name}", exception.advisory));
                    }
                }
                if exception.review_by.as_str() < today {
                    return Err(format!(
                        "expired advisory exceptions: {} (review was due {})",
                        exception.advisory, exception.review_by
                    ));
                }
                let deny_entry = deny.lines().find(|line| line.contains(&exception.advisory)).unwrap_or_default();
                match deny_entry.contains(&exception.owner) && deny_entry.contains(&exception.review_by) {
                    | true => Ok(()),
                    | false => Err(format!("{} owner or review date differs between policy files", exception.advisory)),
                }
            })
            .and_then(|()| {
                let expected = exceptions.iter().map(|exception| exception.advisory.clone()).collect::<BTreeSet<_>>();
                match advisory_ids(deny) == expected {
                    | true => Ok(exceptions),
                    | false => Err(format!("{DENY_PATH} advisory exceptions do not match {LEDGER_PATH}")),
                }
            })
    })
}

#[cfg(test)]
mod tests {
    use super::{validate_dependencies, validate_exceptions, Baseline, Exception};

    const HEADER: &str = "advisory\towner\treview_by\treachability\tupstream\tremoval_condition\trationale\n";
    const ROW: &str = "RUSTSEC-2026-0194\t@owner\t2026-10-28\treachable\thttps://example.com\tupgrade\trequired\n";

    fn deny(exception: &Exception) -> String {
        format!(
            "[advisories]\nignore = [{{ id = \"{}\", reason = \"{} review by {}\" }}]\n",
            exception.advisory, exception.owner, exception.review_by
        )
    }

    #[test]
    fn expired_exception_fails_closed() {
        let ledger = format!("{HEADER}{}", ROW.replace("2026-10-28", "2000-01-01"));
        let exception = super::parse_exceptions(&ledger).expect("valid fixture").remove(0);
        let error = validate_exceptions(&ledger, &deny(&exception), "2026-10-02").expect_err("expired exception");
        assert!(error.contains("expired advisory exceptions"));
    }

    #[test]
    fn forbidden_dependency_fails_closed() {
        let baseline = Baseline {
            acorn_revision: String::new(),
            forbidden_packages: vec!["acorn-lib".to_string()],
            forbidden_prefixes: vec!["polars-".to_string()],
            max_duplicate_names: 0,
            max_locked_packages: 1,
            required_acorn_crates: Vec::new(),
        };
        let error = validate_dependencies(&baseline, "name = \"polars-core\"\n", "").expect_err("forbidden package prefix");
        assert!(error.contains("polars-core"));
    }

    #[test]
    fn unconfigured_exception_fails_closed() {
        let ledger = format!("{HEADER}{ROW}");
        let error = validate_exceptions(&ledger, "[advisories]\nignore = []\n", "2026-10-02").expect_err("missing deny entry");
        assert!(error.contains("owner or review date differs"));
    }
}
