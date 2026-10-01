//! Reproducible measurement and analysis for the memory-aware scheduling spike.
extern crate alloc;

mod analysis;
mod measure;
mod model;

use analysis::{analyze, render_markdown};
use measure::{measure, probe, MeasureOptions, ProbeOptions};
use model::{Manifest, Observation};
use std::env;
use std::fs::{read_to_string, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

enum Operation {
    Analyze {
        json: PathBuf,
        manifest: PathBuf,
        markdown: PathBuf,
        observations: PathBuf,
    },
    Measure {
        cgroup_root: PathBuf,
        manifest: PathBuf,
        observations: PathBuf,
        work_root: PathBuf,
        xylem: PathBuf,
    },
    Probe {
        cgroup_root: PathBuf,
        child_mib: usize,
        executable: PathBuf,
        hold_millis: u64,
        json: PathBuf,
        parent_mib: usize,
    },
    Validate {
        manifest: PathBuf,
    },
}

fn main() {
    let result = parse(env::args().skip(1).collect()).and_then(run);
    if let Err(error) = result {
        eprintln!("Error: {error}");
        exit(exitcode::DATAERR);
    }
}
fn parse(arguments: Vec<String>) -> Result<Operation, String> {
    arguments.first().map_or_else(
        || Err(usage()),
        |operation| match operation.as_str() {
            | "analyze" => required_value(&arguments, "--json").and_then(|json| {
                required_value(&arguments, "--manifest").and_then(|manifest| {
                    required_value(&arguments, "--markdown").and_then(|markdown| {
                        required_value(&arguments, "--observations").map(|observations| Operation::Analyze {
                            json,
                            manifest,
                            markdown,
                            observations,
                        })
                    })
                })
            }),
            | "measure" => required_value(&arguments, "--cgroup-root").and_then(|cgroup_root| {
                required_value(&arguments, "--manifest").and_then(|manifest| {
                    required_value(&arguments, "--observations").and_then(|observations| {
                        required_value(&arguments, "--work-root").and_then(|work_root| {
                            required_value(&arguments, "--xylem").map(|xylem| Operation::Measure {
                                cgroup_root,
                                manifest,
                                observations,
                                work_root,
                                xylem,
                            })
                        })
                    })
                })
            }),
            | "probe" => required_value(&arguments, "--cgroup-root").and_then(|cgroup_root| {
                required_usize(&arguments, "--child-mib").and_then(|child_mib| {
                    required_value(&arguments, "--probe").and_then(|executable| {
                        required_u64(&arguments, "--hold-millis").and_then(|hold_millis| {
                            required_value(&arguments, "--json").and_then(|json| {
                                required_usize(&arguments, "--parent-mib").map(|parent_mib| Operation::Probe {
                                    cgroup_root,
                                    child_mib,
                                    executable,
                                    hold_millis,
                                    json,
                                    parent_mib,
                                })
                            })
                        })
                    })
                })
            }),
            | "validate" => required_value(&arguments, "--manifest").map(|manifest| Operation::Validate { manifest }),
            | _ => Err(usage()),
        },
    )
}
fn read_observations(path: &Path) -> Result<Vec<Observation>, String> {
    read_to_string(path)
        .map_err(|error| format!("read observations {}: {error}", path.display()))
        .and_then(|content| {
            content
                .lines()
                .enumerate()
                .filter(|(_, line)| !line.trim().is_empty())
                .map(|(index, line)| {
                    serde_json::from_str(line).map_err(|error| format!("parse observation {} line {}: {error}", path.display(), index + 1))
                })
                .collect()
        })
}
fn required_value(arguments: &[String], name: &str) -> Result<PathBuf, String> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing required {name}\n{}", usage()))
}
fn required_u64(arguments: &[String], name: &str) -> Result<u64, String> {
    required_value(arguments, name).and_then(|value| {
        value
            .to_str()
            .ok_or_else(|| format!("{name} must be valid UTF-8"))
            .and_then(|value| value.parse::<u64>().map_err(|error| format!("invalid {name}: {error}")))
    })
}
fn required_usize(arguments: &[String], name: &str) -> Result<usize, String> {
    required_value(arguments, name).and_then(|value| {
        value
            .to_str()
            .ok_or_else(|| format!("{name} must be valid UTF-8"))
            .and_then(|value| value.parse::<usize>().map_err(|error| format!("invalid {name}: {error}")))
    })
}
fn write_new(path: &Path, content: impl AsRef<[u8]>) -> Result<(), String> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("create output {}: {error}", path.display()))
        .and_then(|mut output| {
            output
                .write_all(content.as_ref())
                .map_err(|error| format!("write output {}: {error}", path.display()))
        })
}
fn run(operation: Operation) -> Result<(), String> {
    match operation {
        | Operation::Analyze {
            json,
            manifest,
            markdown,
            observations,
        } => Manifest::read(&manifest).and_then(|manifest| {
            read_observations(&observations)
                .and_then(|observations| analyze(&manifest, &observations))
                .and_then(|analysis| {
                    serde_json::to_vec_pretty(&analysis)
                        .map_err(|error| format!("serialize analysis: {error}"))
                        .and_then(|content| write_new(&json, content))
                        .and_then(|()| write_new(&markdown, render_markdown(&analysis)))
                })
        }),
        | Operation::Measure {
            cgroup_root,
            manifest,
            observations: observation_path,
            work_root,
            xylem,
        } => Manifest::read(&manifest).and_then(|manifest| {
            measure(
                &manifest,
                &MeasureOptions {
                    cgroup_root: &cgroup_root,
                    work_root: &work_root,
                    xylem: &xylem,
                },
            )
            .and_then(|observations| {
                observations
                    .iter()
                    .map(serde_json::to_string)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| format!("serialize observations: {error}"))
                    .and_then(|lines| write_new(&observation_path, format!("{}\n", lines.join("\n"))))
            })
        }),
        | Operation::Probe {
            cgroup_root,
            child_mib,
            executable,
            hold_millis,
            json,
            parent_mib,
        } => probe(&ProbeOptions {
            child_mib,
            cgroup_root: &cgroup_root,
            executable: &executable,
            hold_millis,
            parent_mib,
        })
        .and_then(|observation| {
            serde_json::to_vec_pretty(&observation)
                .map_err(|error| format!("serialize synthetic probe observation: {error}"))
                .and_then(|content| write_new(&json, content))
        }),
        | Operation::Validate { manifest } => Manifest::read(&manifest).map(|_| println!("Manifest is valid: {}", manifest.display())),
    }
}
fn usage() -> String {
    "usage:\n  xylem-memory-spike validate --manifest PATH\n  xylem-memory-spike probe --cgroup-root PATH --probe PATH --parent-mib N --child-mib N --hold-millis N --json PATH\n  xylem-memory-spike measure --manifest PATH --cgroup-root PATH --xylem PATH --work-root PATH --observations PATH\n  xylem-memory-spike analyze --manifest PATH --observations PATH --json PATH --markdown PATH".to_string()
}
