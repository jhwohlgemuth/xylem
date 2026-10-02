use crate::release::{canonical_version, workspace_version};
use std::fs::{create_dir_all, read_to_string, remove_file, write};
use std::path::Path;
use std::process::Command;

const SOURCE_PREFIX: &str = "https://static.crates.io/crates/xylem-cli/xylem-cli-";
const TEMPLATE_PATH: &str = "scripts/homebrew/formula.template.rb";

pub(crate) fn generate(version: &str, output: &str, supplied_sha256: Option<&String>) -> Result<(), String> {
    canonical_version(version)
        .and_then(|version| {
            workspace_version().and_then(|workspace| match version == workspace {
                | true => Ok(version),
                | false => Err(format!("requested version {version} does not match workspace version {workspace}")),
            })
        })
        .and_then(|version| {
            let source = format!("{SOURCE_PREFIX}{version}.crate");
            sha256(&source, supplied_sha256.map(String::as_str)).and_then(|sha256| {
                read_to_string(TEMPLATE_PATH)
                    .map_err(|error| format!("read {TEMPLATE_PATH}: {error}"))
                    .and_then(|template| render(&template, &version, &sha256))
                    .and_then(|formula| write_formula(output, &formula))
                    .map(|()| {
                        println!("Wrote {output}");
                        println!("Source URL: {source}");
                        println!("sha256: {sha256}");
                    })
            })
        })
}

fn render(template: &str, version: &str, sha256: &str) -> Result<String, String> {
    match sha256.len() == 64
        && sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        | false => Err("SHA-256 must contain exactly 64 lowercase hexadecimal characters".to_string()),
        | true => {
            let rendered = template.replace("__VERSION__", version).replace("__SHA256__", sha256);
            match rendered.contains("__") {
                | true => Err("rendered formula contains unresolved template values".to_string()),
                | false => Ok(rendered),
            }
        }
    }
}

fn sha256(source: &str, supplied: Option<&str>) -> Result<String, String> {
    match supplied {
        | Some(value) => Ok(value.to_string()),
        | None => {
            let archive = std::env::temp_dir().join(format!("xylem-formula-{}.crate", std::process::id()));
            let download = Command::new("curl")
                .args([
                    "-A",
                    "xylem-homebrew-formula/1.0",
                    "--retry",
                    "3",
                    "--retry-delay",
                    "1",
                    "-LsSf",
                    source,
                    "-o",
                ])
                .arg(&archive)
                .status()
                .map_err(|error| format!("download {source}: {error}"))
                .and_then(|status| match status.success() {
                    | true => checksum(&archive),
                    | false => Err(format!("download {source} failed with {status}")),
                });
            let _ = remove_file(archive);
            download
        }
    }
}

fn checksum(path: &Path) -> Result<String, String> {
    [["sha256sum", ""], ["shasum", "-a"]]
        .into_iter()
        .find_map(|[program, flag]| {
            let mut command = Command::new(program);
            if !flag.is_empty() {
                command.args([flag, "256"]);
            }
            command.arg(path).output().ok().filter(|output| output.status.success())
        })
        .ok_or_else(|| "no SHA-256 utility found (need sha256sum or shasum)".to_string())
        .and_then(|output| String::from_utf8(output.stdout).map_err(|error| error.to_string()))
        .and_then(|output| {
            output
                .split_whitespace()
                .next()
                .map(str::to_string)
                .ok_or_else(|| "SHA-256 utility returned no digest".to_string())
        })
}

fn write_formula(output: &str, formula: &str) -> Result<(), String> {
    let path = Path::new(output);
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(create_dir_all)
        .transpose()
        .map_err(|error| format!("create output directory: {error}"))
        .and_then(|_| write(path, formula).map_err(|error| format!("write {output}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn formula_render_is_deterministic() {
        let template = "url __VERSION__\nsha256 __SHA256__\n";
        let sha256 = "a".repeat(64);
        let first = render(template, "1.0.3", &sha256).expect("valid formula");
        let second = render(template, "1.0.3", &sha256).expect("valid formula");
        assert_eq!(first, second);
        assert!(first.contains("1.0.3"));
        assert!(first.contains(&sha256));
    }

    #[test]
    fn formula_render_rejects_invalid_checksum() {
        let error = render("__VERSION__ __SHA256__", "1.0.3", "missing").expect_err("invalid checksum");
        assert!(error.contains("64 lowercase"));
    }
}
