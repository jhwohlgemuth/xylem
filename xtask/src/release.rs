use core::time::Duration;
use std::fs::read_to_string;
use std::process::Command;
use std::thread::sleep;

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

pub(crate) fn publish_crates() -> Result<(), String> {
    std::env::var("CARGO_REGISTRY_TOKEN")
        .map_err(|_| "CARGO_REGISTRY_TOKEN is required".to_string())
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
