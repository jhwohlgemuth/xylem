//! Render a local Xylem workflow as a standalone GitLab pipeline.
extern crate alloc;

use crate::cli::DEFAULT_CONFIG_NAMES;
use acorn_cmd::args;
use acorn_host::config::resolve_config_path;
use acorn_schema::validation::Validate;
use alloc::collections::{BTreeMap, BTreeSet};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsString;
use std::fs::{canonicalize, create_dir_all, read, read_to_string, rename, write};
use core::iter::once;
use std::path::{Component, Path, PathBuf};
use xylem_lib::config::{Config, InputAttribute};
use xylem_lib::script::{Module, ModuleType, ModuleUri, VirtualEnvironmentManager};
use xylem_lib::util::constants::app::{GITLAB_DEFAULT_IMAGE, MINIFORGE_SHA256, MINIFORGE_VERSION, PIXI_SHA256, PIXI_VERSION};

#[derive(Serialize)]
struct GitLabArtifacts {
    expire_in: &'static str,
    paths: Vec<String>,
}
#[derive(Serialize)]
struct GitLabDefault {
    before_script: Vec<String>,
    image: String,
}
#[derive(Serialize)]
struct GitLabJob {
    stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    needs: Option<Vec<GitLabNeed>>,
    script: Vec<String>,
    artifacts: GitLabArtifacts,
}
#[derive(Serialize)]
struct GitLabNeed {
    job: String,
    artifacts: bool,
}
#[derive(Serialize)]
struct GitLabPipeline {
    workflow: GitLabWorkflow,
    stages: Vec<String>,
    default: GitLabDefault,
    #[serde(flatten)]
    jobs: BTreeMap<String, GitLabJob>,
}
#[derive(Serialize)]
struct GitLabRule {
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    condition: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    when: Option<&'static str>,
}
#[derive(Serialize)]
struct GitLabWorkflow {
    rules: Vec<GitLabRule>,
}
fn collect_sources(path: &Path, root: &Path, sources: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    canonicalize(path)
        .map_err(|error| format!("resolve {}: {error}", path.display()))
        .and_then(|path| match path.strip_prefix(root).is_ok() {
            | true => Ok(path),
            | false => Err(format!("configuration {} is outside the current repository", path.display())),
        })
        .and_then(|path| match sources.insert(path.clone()) {
            | false => Ok(()),
            | true => Config::try_from(path.as_path()).map_err(|error| error.to_string()).and_then(|config| {
                validate_source(&config, &path, sources.len()).and_then(|()| {
                    config.modules.iter().try_for_each(|module| match &module.module_type {
                        | ModuleType::Configuration { uri, .. } => match ModuleUri::from(uri) {
                            | ModuleUri::File(child) => {
                                let child = if child.is_absolute() {
                                    child
                                } else {
                                    path.parent().unwrap_or(root).join(child)
                                };
                                collect_sources(&child, root, sources)
                            }
                            | _ => Err(format!("remote configuration module {} cannot be exported to GitLab CI", module.name)),
                        },
                        | _ => Ok(()),
                    })
                })
            }),
        })
}
/// Export a discoverable workflow to a GitLab CI file, or compare it with an existing file.
pub fn gitlab(config: Option<&PathBuf>, output: &Path, image: Option<&str>, force: bool, check: bool) -> Result<(), String> {
    env::current_dir()
        .map_err(|error| format!("resolve current directory: {error}"))
        .and_then(|root| {
            resolve_config_path(config.map(PathBuf::as_path), &root, &DEFAULT_CONFIG_NAMES)
                .ok_or_else(|| format!("no workflow configuration found; expected one of {}", DEFAULT_CONFIG_NAMES.join(", ")))
                .and_then(|path| canonicalize(path).map_err(|error| format!("resolve configuration path: {error}")))
                .and_then(|config_path| {
                    relative_path(output).and_then(|output_relative| {
                        let image = image.unwrap_or(GITLAB_DEFAULT_IMAGE);
                        match image.trim().is_empty() {
                            | true => Err("GitLab image must not be empty".to_string()),
                            | false => render_from_path(&root, &config_path, &output_relative, image)
                                .and_then(|rendered| persist(&root.join(output_relative), rendered, force, check)),
                        }
                    })
                })
        })
}
fn installers(config: &Config) -> Vec<String> {
    let mut before_script = vec![
        shell_command(
            "cargo",
            &args!["install", "--locked", ("--version", env!("CARGO_PKG_VERSION")), "xylem-cli"],
        ),
        "export PATH=\"$HOME/.cargo/bin:$PATH\"".to_string(),
    ];
    let mamba = config.requirements.iter().any(|name| name == "mamba")
        || config.modules.iter().any(|module| module.uses_manager(VirtualEnvironmentManager::Mamba));
    let pixi = config.requirements.iter().any(|name| name == "pixi")
        || config.modules.iter().any(|module| module.uses_manager(VirtualEnvironmentManager::Pixi));
    if mamba {
        before_script.extend([
            shell_command(
                "curl",
                &args![
                    "-LsSf",
                    ("--output", "/tmp/xylem-miniforge.sh"),
                    format!("https://github.com/conda-forge/miniforge/releases/download/{MINIFORGE_VERSION}/Miniforge3-{MINIFORGE_VERSION}-Linux-x86_64.sh")
                ],
            ),
            format!(
                "{} | {}",
                shell_command("printf", &args!["%s  %s\\n", MINIFORGE_SHA256, "/tmp/xylem-miniforge.sh"]),
                shell_command("sha256sum", &args!["--check", "-"])
            ),
            shell_command("bash", &args!["/tmp/xylem-miniforge.sh", "-b", "-p", "/tmp/xylem-miniforge"]),
            "export PATH=\"/tmp/xylem-miniforge/bin:$PATH\"".to_string(),
        ]);
    }
    if pixi {
        before_script.extend([
            shell_command(
                "curl",
                &args![
                    "-LsSf",
                    ("--output", "/tmp/pixi-x86_64-unknown-linux-musl"),
                    format!("https://github.com/prefix-dev/pixi/releases/download/v{PIXI_VERSION}/pixi-x86_64-unknown-linux-musl")
                ],
            ),
            format!(
                "{} | {}",
                shell_command("printf", &args!["%s  %s\\n", PIXI_SHA256, "/tmp/pixi-x86_64-unknown-linux-musl"]),
                shell_command("sha256sum", &args!["--check", "-"])
            ),
            shell_command("mv", &args!["/tmp/pixi-x86_64-unknown-linux-musl", "/tmp/pixi"]),
            shell_command("chmod", &args!["0755", "/tmp/pixi"]),
            "export PATH=\"/tmp:$PATH\"".to_string(),
        ]);
    }
    before_script
}
fn persist(output_path: &Path, rendered: String, force: bool, check: bool) -> Result<(), String> {
    match read_to_string(output_path) {
        | Ok(existing) if existing == rendered => Ok(()),
        | Ok(_) if check => Err(format!("{} is stale; run xylem export --gitlab to regenerate it", output_path.display())),
        | Ok(_) if !force => Err(format!("{} already exists; pass --force to replace it", output_path.display())),
        | Err(error) if check => Err(format!("read {}: {error}", output_path.display())),
        | Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!("read {}: {error}", output_path.display())),
        | _ => output_path
            .parent()
            .ok_or_else(|| "GitLab output has no parent directory".to_string())
            .and_then(|parent| create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display())))
            .and_then(|()| {
                let temporary = output_path.with_extension(format!("tmp-{}", std::process::id()));
                write(&temporary, rendered)
                    .map_err(|error| format!("write {}: {error}", temporary.display()))
                    .and_then(|()| rename(&temporary, output_path).map_err(|error| format!("write {}: {error}", output_path.display())))
            })
            .map(|()| println!("Wrote {}", output_path.display())),
    }
}
fn relative_path(path: &Path) -> Result<PathBuf, String> {
    match path.is_absolute() {
        | true => Err(format!("path {} must be relative to the current repository", path.display())),
        | false => path
            .components()
            .try_fold(PathBuf::new(), |mut clean, part| match part {
                | Component::CurDir => Ok(clean),
                | Component::Normal(name) => {
                    clean.push(name);
                    Ok(clean)
                }
                | Component::ParentDir if clean.pop() => Ok(clean),
                | _ => Err(format!("path {} escapes the current repository", path.display())),
            })
            .and_then(|clean| match clean.as_os_str().is_empty() {
                | true => Err("path must not be empty".to_string()),
                | false => Ok(clean),
            }),
    }
}
fn render(config: &Config, config_path: &Path, output_path: &Path, image: &str, root: &Path, digest: &str) -> Result<String, String> {
    let config_dir = config_path.parent().unwrap_or(Path::new(""));
    match config.modules.is_empty() {
        | true => Err("GitLab export requires at least one executable module".to_string()),
        | false => config
            .modules
            .iter()
            .try_for_each(|module| validate_module(module, config_dir, root))
            .and_then(|()| {
                config
                    .output
                    .as_ref()
                    .ok_or_else(|| "GitLab export requires a workflow output directory".to_string())
            })
            .and_then(|output| relative_path(&config_dir.join(output)))
            .and_then(|artifact| {
                let absolute = root.join(&artifact);
                match absolute.exists() && !absolute.is_dir() {
                    | true => Err(format!("workflow output {} is not a directory", absolute.display())),
                    | false => render_yaml(config, config_path, output_path, image, digest, &artifact),
                }
            }),
    }
}
fn render_from_path(root: &Path, config_path: &Path, output_relative: &Path, image: &str) -> Result<String, String> {
    config_path
        .strip_prefix(root)
        .map_err(|_| "workflow configuration must be inside the current repository".to_string())
        .and_then(|config_relative| {
            let mut sources = BTreeSet::new();
            collect_sources(config_path, root, &mut sources)
                .and_then(|()| source_digest(&sources, root))
                .and_then(|digest| {
                    Config::try_from(config_path)
                        .map_err(|error| error.to_string())
                        .and_then(|config| config.resolve(config_path).map_err(|error| error.to_string()))
                        .and_then(|config| config.validate().map_err(|error| error.to_string()).map(|()| config))
                        .and_then(|config| render(&config, config_relative, output_relative, image, root, &digest))
                })
        })
}
fn render_yaml(config: &Config, config_path: &Path, output_path: &Path, image: &str, digest: &str, artifact: &Path) -> Result<String, String> {
    let config_dir = config_path.parent().unwrap_or(Path::new(""));
    let job_directory = match config_dir.as_os_str().is_empty() {
        | true => Path::new("."),
        | false => config_dir,
    };
    let names = config
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| format!("xylem-{index:03}-{}", slug(&module.name)))
        .collect::<Vec<_>>();
    let stages = names.iter().enumerate().map(|(index, _)| format!("xylem-{index:03}")).collect::<Vec<_>>();
    let export_check = shell_command(
        "xylem",
        &args![
            "export",
            "--gitlab",
            config_path.as_os_str(),
            ("--output", output_path.as_os_str()),
            ("--image", image),
            "--check"
        ],
    );
    let jobs = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let script = (index == 0)
                .then(|| export_check.clone())
                .into_iter()
                .chain([
                    shell_command("cd", &args![job_directory.as_os_str()]),
                    shell_command(
                        "xylem",
                        &args![
                            "--no-progress",
                            "run",
                            config_path.file_name().unwrap_or_default(),
                            ("--module-index", index.to_string())
                        ],
                    ),
                ])
                .collect();
            (
                name.clone(),
                GitLabJob {
                    stage: stages[index].clone(),
                    needs: index.checked_sub(1).map(|previous| {
                        vec![GitLabNeed {
                            job: names[previous].clone(),
                            artifacts: true,
                        }]
                    }),
                    script,
                    artifacts: GitLabArtifacts {
                        expire_in: "1 week",
                        paths: vec![artifact.to_string_lossy().into_owned()],
                    },
                },
            )
        })
        .collect();
    let pipeline = GitLabPipeline {
        workflow: GitLabWorkflow {
            rules: vec![
                GitLabRule {
                    condition: Some("$CI_PIPELINE_SOURCE == \"web\""),
                    when: None,
                },
                GitLabRule {
                    condition: None,
                    when: Some("never"),
                },
            ],
        },
        stages,
        default: GitLabDefault {
            before_script: installers(config),
            image: image.to_string(),
        },
        jobs,
    };
    serde_json::to_string(&config_path.to_string_lossy())
        .map_err(|error| format!("serialize configuration path: {error}"))
        .and_then(|source| {
            serde_norway::to_string(&pipeline)
                .map(|yaml| format!("# Generated by xylem export --gitlab from {source}\n# Source configuration digest: {digest}\n{yaml}"))
                .map_err(|error| format!("serialize GitLab CI: {error}"))
        })
}
fn shell_command(program: &str, arguments: &[OsString]) -> String {
    once(shell_quote(program))
        .chain(arguments.iter().map(|argument| shell_quote(&argument.to_string_lossy())))
        .collect::<Vec<_>>()
        .join(" ")
}
fn shell_quote(value: &str) -> String {
    let unquoted = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | ':' | '@'));
    match unquoted {
        | true => value.to_string(),
        | false => format!("'{}'", value.replace('\'', "'\\''")),
    }
}
fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            | true => character.to_ascii_lowercase(),
            | false => '-',
        })
        .collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    match slug.is_empty() {
        | true => "module".to_string(),
        | false => slug,
    }
}
fn source_digest(sources: &BTreeSet<PathBuf>, root: &Path) -> Result<String, String> {
    sources
        .iter()
        .try_fold(Sha256::new(), |mut digest, path| {
            path.strip_prefix(root)
                .map_err(|error| error.to_string())
                .and_then(|relative| {
                    read(path)
                        .map_err(|error| format!("read {}: {error}", path.display()))
                        .map(|content| (relative, content))
                })
                .map(|(relative, content)| {
                    digest.update(relative.to_string_lossy().as_bytes());
                    digest.update([0]);
                    digest.update(&content);
                    digest.update([0]);
                    digest
                })
        })
        .map(|digest| format!("{:x}", digest.finalize()))
}
fn validate_module(module: &Module, config_dir: &Path, root: &Path) -> Result<(), String> {
    match &module.module_type {
        | ModuleType::Binary { uri, .. } | ModuleType::Script { uri, .. } => match ModuleUri::from(uri) {
            | ModuleUri::File(path) => {
                let path = if path.is_absolute() { path } else { root.join(config_dir).join(path) };
                canonicalize(&path)
                    .map_err(|error| format!("resolve module {} at {}: {error}", module.name, path.display()))
                    .and_then(|path| {
                        path.strip_prefix(root)
                            .map(|_| ())
                            .map_err(|_| format!("module {} is outside the current repository", module.name))
                    })
            }
            | _ => Err(format!("remote module {} cannot be exported to GitLab CI", module.name)),
        },
        | ModuleType::Configuration { .. } => Err(format!("module {} did not resolve to an executable leaf", module.name)),
    }
}
fn validate_source(config: &Config, path: &Path, source_count: usize) -> Result<(), String> {
    match (config.sandbox, &config.input, source_count > 1 && config.output.is_some()) {
        | (true, _, _) => Err(format!("sandboxed configuration {} cannot be exported to GitLab CI", path.display())),
        | (_, Some(InputAttribute::Path(_) | InputAttribute::Select(_)), _) => {
            Err(format!("dynamic input discovery in {} cannot be exported to GitLab CI", path.display()))
        }
        | (_, _, true) => Err(format!("nested configuration {} must inherit the root workflow output", path.display())),
        | _ => Ok(()),
    }
}
