use alloc::collections::BTreeSet;
use core::time::Duration;
use std::fs::{read_to_string, write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use toml_edit::{value, DocumentMut};

const CRATES: [&str; 3] = ["xylem-lib", "xylem-tui", "xylem-cli"];

pub(crate) fn canonical_version(version: &str) -> Result<String, String> {
    let valid = version.split('.').collect::<Vec<_>>();
    match valid.as_slice() {
        | [major, minor, patch]
            if [major, minor, patch].iter().all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_digit()) && (*part == &"0" || !part.starts_with('0'))
            }) =>
        {
            Ok(version.to_string())
        }
        | _ => Err(format!("version must use canonical MAJOR.MINOR.PATCH semantic versioning: {version}")),
    }
}

pub(crate) fn check_release_crates() -> Result<(), String> {
    metadata().and_then(|metadata| {
        let members = metadata
            .get("workspace_members")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "cargo metadata omitted workspace members".to_string())?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<BTreeSet<_>>();
        let expected = metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "cargo metadata omitted packages".to_string())?
            .iter()
            .filter(|package| {
                package
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| members.contains(id))
            })
            .filter(|package| match package.get("publish") {
                | None | Some(serde_json::Value::Null) => true,
                | Some(serde_json::Value::Array(registries)) => registries.iter().any(|registry| registry == "crates-io"),
                | _ => false,
            })
            .filter_map(|package| package.get("name").and_then(serde_json::Value::as_str).map(str::to_string))
            .collect::<BTreeSet<_>>();
        let configured = CRATES.iter().map(|name| name.to_string()).collect::<BTreeSet<_>>();
        let missing = expected.difference(&configured).cloned().collect::<Vec<_>>();
        let extra = configured.difference(&expected).cloned().collect::<Vec<_>>();
        match (missing.is_empty(), extra.is_empty(), configured.len() == CRATES.len()) {
            | (true, true, true) => {
                println!("release crate list matches {} publishable workspace crates", expected.len());
                Ok(())
            }
            | _ => Err(format!(
                "release crate list differs from publishable workspace crates: missing {missing:?}, extra {extra:?}"
            )),
        }
    })
}

pub(crate) fn check_version() -> Result<(), String> {
    let root = read_manifest(Path::new("Cargo.toml"))?;
    let version = workspace_version()?;
    let members = member_manifests(&root)?;
    let packages = package_names(&members)?;
    let mismatches = members
        .iter()
        .flat_map(|(path, manifest)| {
            internal_dependency_names(manifest, &packages)
                .into_iter()
                .filter_map(|name| {
                    let actual = dependency_version(manifest, &name);
                    (actual.as_deref() != Some(version.as_str())).then(|| format!("{}:{name} = {actual:?}", path.display()))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let citation = read_to_string("CITATION.cff").map_err(|error| format!("read CITATION.cff: {error}"))?;
    let citation_version = citation.lines().find_map(|line| line.strip_prefix("version: "));
    let mismatches = match citation_version == Some(version.as_str()) {
        | true => mismatches,
        | false => mismatches
            .into_iter()
            .chain([format!("CITATION.cff version = {citation_version:?}")])
            .collect(),
    };
    match mismatches.is_empty() {
        | true => metadata().map(|_| println!("release version {version} is consistent")),
        | false => Err(format!("release version {version} is inconsistent: {}", mismatches.join(", "))),
    }
}

pub(crate) fn publish_crates() -> Result<(), String> {
    check_release_crates()
        .and_then(|()| check_version())
        .and_then(|()| std::env::var("CARGO_REGISTRY_TOKEN").map_err(|_| "CARGO_REGISTRY_TOKEN is required".to_string()))
        .and_then(|_| workspace_version())
        .and_then(|version| verify_tag(&version).map(|()| version))
        .and_then(|version| {
            CRATES.iter().try_for_each(|name| {
                match published(name, &version) {
                    | true => {
                        println!("{name} {version} is already published");
                        Ok(())
                    }
                    | false => run("cargo", &["publish", "--locked", "--package", name]),
                }
                .and_then(|()| wait_until_published(name, &version))
            })
        })
}

pub(crate) fn release_version(version: &str) -> Result<(), String> {
    let version = canonical_version(version)?;
    let mut root = read_manifest(Path::new("Cargo.toml"))?;
    let mut members = member_manifests(&root)?;
    let packages = package_names(&members)?;
    let citation = read_to_string("CITATION.cff").map_err(|error| format!("read CITATION.cff: {error}"))?;
    match citation.lines().any(|line| line.starts_with("version: ")) {
        | true => {}
        | false => return Err("CITATION.cff has no version field".to_string()),
    }
    root.get_mut("workspace")
        .and_then(|workspace| workspace.get_mut("package"))
        .and_then(|package| package.get_mut("version"))
        .ok_or_else(|| "workspace package version is missing".to_string())
        .map(|item| *item = value(&version))?;
    members.iter_mut().try_for_each(|(_, manifest)| {
        internal_dependency_names(manifest, &packages).into_iter().try_for_each(|name| {
            manifest
                .get_mut("dependencies")
                .and_then(|dependencies| dependencies.get_mut(&name))
                .and_then(|dependency| dependency.get_mut("version"))
                .ok_or_else(|| format!("dependency {name} version is missing"))
                .map(|item| *item = value(&version))
        })
    })?;
    let updated_citation = citation
        .lines()
        .map(|line| match line.starts_with("version: ") {
            | true => format!("version: {version}"),
            | false => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if citation.ends_with('\n') { "\n" } else { "" };
    write("Cargo.toml", root.to_string()).map_err(|error| format!("write Cargo.toml: {error}"))?;
    members
        .into_iter()
        .filter(|(_, manifest)| !internal_dependency_names(manifest, &packages).is_empty())
        .try_for_each(|(path, manifest)| write(&path, manifest.to_string()).map_err(|error| format!("write {}: {error}", path.display())))?;
    write("CITATION.cff", updated_citation).map_err(|error| format!("write CITATION.cff: {error}"))?;
    run("cargo", &["update", "--workspace"])?;
    check_version()
}

fn dependency_version(manifest: &DocumentMut, name: &str) -> Option<String> {
    manifest.get("dependencies")?.get(name)?.get("version")?.as_str().map(str::to_string)
}

fn internal_dependency_names(manifest: &DocumentMut, packages: &BTreeSet<String>) -> Vec<String> {
    manifest
        .get("dependencies")
        .and_then(toml_edit::Item::as_table)
        .into_iter()
        .flat_map(|dependencies| dependencies.iter())
        .filter_map(|(name, dependency)| {
            let inline = dependency.as_inline_table()?;
            let package = inline.get("package").and_then(toml_edit::Value::as_str).unwrap_or(name);
            (inline.contains_key("path") && inline.contains_key("version") && packages.contains(package)).then(|| name.to_string())
        })
        .collect()
}

fn member_manifests(root: &DocumentMut) -> Result<Vec<(PathBuf, DocumentMut)>, String> {
    root.get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml_edit::Item::as_array)
        .ok_or_else(|| "workspace members are missing from Cargo.toml".to_string())?
        .iter()
        .map(|member| {
            member
                .as_str()
                .ok_or_else(|| "workspace member is not a path".to_string())
                .map(|member| Path::new(member).join("Cargo.toml"))
                .and_then(|path| read_manifest(&path).map(|manifest| (path, manifest)))
        })
        .collect()
}

fn metadata() -> Result<serde_json::Value, String> {
    Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked", "--offline"])
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))
        .and_then(|output| match output.status.success() {
            | true => serde_json::from_slice(&output.stdout).map_err(|error| format!("parse cargo metadata: {error}")),
            | false => Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr).trim())),
        })
}

fn package_names(members: &[(PathBuf, DocumentMut)]) -> Result<BTreeSet<String>, String> {
    members
        .iter()
        .map(|(path, manifest)| {
            manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml_edit::Item::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("package name is missing from {}", path.display()))
        })
        .collect()
}

fn read_manifest(path: &Path) -> Result<DocumentMut, String> {
    read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))
        .and_then(|content| content.parse().map_err(|error| format!("parse {}: {error}", path.display())))
}

fn published(name: &str, version: &str) -> bool {
    Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--user-agent",
            &format!("xylem-ci/{version}"),
            &format!("https://crates.io/api/v1/crates/{name}/{version}"),
        ])
        .status()
        .is_ok_and(|status| status.success())
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

fn verify_tag(version: &str) -> Result<(), String> {
    match std::env::var("CI_COMMIT_TAG").ok().filter(|tag| !tag.is_empty()) {
        | Some(tag) if tag != format!("v{version}") => Err(format!("tag {tag} does not match workspace version {version}")),
        | _ => Ok(()),
    }
}

fn wait_until_published(name: &str, version: &str) -> Result<(), String> {
    (0..30)
        .find(|_| {
            let available = published(name, version);
            if !available {
                sleep(Duration::from_secs(10));
            }
            available
        })
        .map(|_| ())
        .ok_or_else(|| format!("{name} {version} was not available from crates.io after 5 minutes"))
}

pub(crate) fn workspace_version() -> Result<String, String> {
    read_to_string("Cargo.toml")
        .map_err(|error| format!("read Cargo.toml: {error}"))
        .and_then(|manifest| {
            manifest
                .lines()
                .skip_while(|line| line.trim() != "[workspace.package]")
                .skip(1)
                .take_while(|line| !line.trim_start().starts_with('['))
                .find_map(|line| line.trim().strip_prefix("version = \"").and_then(|value| value.strip_suffix('"')))
                .map(str::to_string)
                .ok_or_else(|| "workspace version not found in Cargo.toml".to_string())
        })
        .and_then(|version| canonical_version(&version))
}

#[cfg(test)]
mod tests {
    use super::canonical_version;

    #[test]
    fn canonical_versions_are_accepted() {
        assert_eq!(canonical_version("1.0.3").as_deref(), Ok("1.0.3"));
    }

    #[test]
    fn noncanonical_versions_are_rejected() {
        for version in ["v1.0.3", "1.0", "01.0.3", "1.0.3-alpha"] {
            assert!(canonical_version(version).is_err(), "accepted {version}");
        }
    }
}
