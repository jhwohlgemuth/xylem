use crate::model::{
    Case, HostIdentity, Manifest, Observation, Partition, SubjectType, CASE_REPETITIONS, MIX_REPETITIONS, OBSERVATION_SCHEMA_VERSION,
};
#[cfg(target_os = "linux")]
use core::time::Duration;
use serde::Serialize;
use serde_json::{Map, Value};
#[cfg(target_os = "linux")]
use std::fs::remove_dir;
use std::fs::{create_dir, create_dir_all, read_to_string, remove_dir_all, write};
#[cfg(target_os = "linux")]
use std::io::Write;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::{Child, Stdio};
use std::process::{Command, ExitStatus};
#[cfg(target_os = "linux")]
use std::thread::sleep;
#[cfg(target_os = "linux")]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);

const WARMUP_REPETITIONS: usize = 1;

struct CommandMeasurement {
    anon_peak_bytes: u64,
    duration_millis: u64,
    exit_status: ExitStatus,
    file_peak_bytes: u64,
    oom_events: u64,
    oom_kill_events: u64,
    peak_bytes: u64,
    sampled_current_peak_bytes: u64,
}

pub(crate) struct MeasureOptions<'a> {
    pub(crate) cgroup_root: &'a Path,
    pub(crate) work_root: &'a Path,
    pub(crate) xylem: &'a Path,
}

#[derive(Serialize)]
pub(crate) struct ProbeObservation {
    pub(crate) anon_peak_bytes: u64,
    pub(crate) expected_minimum_bytes: u64,
    pub(crate) file_peak_bytes: u64,
    pub(crate) peak_bytes: u64,
    pub(crate) sampled_current_peak_bytes: u64,
}

pub(crate) struct ProbeOptions<'a> {
    pub(crate) child_mib: usize,
    pub(crate) cgroup_root: &'a Path,
    pub(crate) executable: &'a Path,
    pub(crate) hold_millis: u64,
    pub(crate) parent_mib: usize,
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct SamplePeaks {
    anon_bytes: u64,
    current_bytes: u64,
    file_bytes: u64,
}

struct Subject<'a> {
    cases: &'a [&'a Case],
    estimator_key: &'a str,
    partition: Partition,
    repetitions: usize,
    subject_id: &'a str,
    subject_type: SubjectType,
}

fn build_arguments(cases: &[&Case], config: &Path, report_root: &Path) -> Result<Vec<String>, String> {
    cases.first().map_or_else(
        || Err("measurement subject must contain a case".to_string()),
        |_| {
            let inputs = cases.iter().filter_map(|case| case.input.as_ref()).collect::<Vec<_>>();
            let comma_path = inputs.iter().find(|input| input.to_string_lossy().contains(','));
            match comma_path {
                | Some(path) => Err(format!("measurement input cannot contain a comma: {}", path.display())),
                | None => {
                    let mut arguments = vec![
                        "--threads".to_string(),
                        cases.len().max(1).to_string(),
                        "--no-progress".to_string(),
                        "run".to_string(),
                        config.display().to_string(),
                        "--report-output".to_string(),
                        report_root.display().to_string(),
                    ];
                    if !inputs.is_empty() {
                        arguments.push("--input".to_string());
                        arguments.push(inputs.iter().map(|input| input.display().to_string()).collect::<Vec<_>>().join(","));
                    }
                    Ok(arguments)
                }
            }
        },
    )
}

fn set_output_variable(object: &mut Map<String, Value>, output: &str, context: &str) -> Result<(), String> {
    let variables = object.entry("variables").or_insert_with(|| Value::Object(Map::new()));
    if variables.is_null() {
        *variables = Value::Object(Map::new());
    }
    variables.as_object_mut().map_or_else(
        || Err(format!("measurement config {context} variables must be an object")),
        |variables| {
            variables.insert("OUTPUT_DIRECTORY".to_string(), Value::String(output.to_string()));
            Ok(())
        },
    )
}

fn write_subject_config(source: &Path, destination: &Path, artifact_root: &Path, concurrent: bool) -> Result<(), String> {
    read_to_string(source)
        .map_err(|error| format!("read measurement config {}: {error}", source.display()))
        .and_then(|content| {
            serde_json::from_str::<Value>(&content).map_err(|error| format!("parse measurement config {}: {error}", source.display()))
        })
        .and_then(|mut configuration| {
            let artifact_root = artifact_root.display().to_string();
            let output = match concurrent {
                | true => format!("{artifact_root}/{{{{ PARENT_DIRECTORY }}}}"),
                | false => artifact_root.clone(),
            };
            configuration
                .as_object_mut()
                .ok_or_else(|| format!("measurement config {} must contain a JSON object", source.display()))
                .and_then(|object| {
                    object.insert("output".to_string(), Value::String(artifact_root));
                    set_output_variable(object, &output, "root")
                })
                .and_then(|()| {
                    configuration
                        .get_mut("modules")
                        .and_then(Value::as_array_mut)
                        .ok_or_else(|| format!("measurement config {} modules must be an array", source.display()))
                })
                .and_then(|modules| {
                    modules.iter_mut().enumerate().try_for_each(|(index, module)| {
                        module
                            .as_object_mut()
                            .ok_or_else(|| format!("measurement config module {index} must be an object"))
                            .and_then(|module| set_output_variable(module, &output, &format!("module {index}")))
                    })
                })
                .and_then(|()| {
                    serde_json::to_vec_pretty(&configuration)
                        .map_err(|error| format!("serialize measurement config {}: {error}", destination.display()))
                })
                .and_then(|content| {
                    write(destination, content).map_err(|error| format!("write measurement config {}: {error}", destination.display()))
                })
        })
}

fn config_checksum(path: &Path) -> Result<String, String> {
    Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|error| format!("run sha256sum for {}: {error}", path.display()))
        .and_then(|output| match output.status.success() {
            | true => String::from_utf8(output.stdout).map_err(|error| format!("decode sha256sum output for {}: {error}", path.display())),
            | false => Err(format!("sha256sum failed for {}: {}", path.display(), output.status)),
        })
        .and_then(|output| {
            output
                .split_whitespace()
                .next()
                .filter(|checksum| checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
                .map(str::to_string)
                .ok_or_else(|| format!("sha256sum returned an invalid digest for {}", path.display()))
        })
}

#[cfg(target_os = "linux")]
fn host_identity(xylem: &Path) -> Result<HostIdentity, String> {
    let kernel_release = read_to_string("/proc/sys/kernel/osrelease")
        .map_err(|error| format!("read Linux kernel release: {error}"))
        .map(|value| value.trim().to_string());
    let total_memory_bytes = read_to_string("/proc/meminfo")
        .map_err(|error| format!("read Linux memory information: {error}"))
        .and_then(|content| {
            content
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:").map(str::trim))
                .and_then(|value| value.split_whitespace().next())
                .ok_or_else(|| "Linux memory information does not contain MemTotal".to_string())
                .and_then(|value| value.parse::<u64>().map_err(|error| format!("parse Linux MemTotal: {error}")))
                .and_then(|kibibytes| kibibytes.checked_mul(1024).ok_or_else(|| "Linux MemTotal overflows bytes".to_string()))
        });
    let cpu_model = read_to_string("/proc/cpuinfo")
        .map_err(|error| format!("read Linux CPU information: {error}"))
        .and_then(|content| {
            content
                .lines()
                .find_map(|line| {
                    line.strip_prefix("model name")
                        .and_then(|value| value.split_once(':').map(|(_, model)| model.trim().to_string()))
                })
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Linux CPU information does not contain a model name".to_string())
        });
    let page_size_bytes = Command::new("getconf")
        .arg("PAGESIZE")
        .output()
        .map_err(|error| format!("run getconf PAGESIZE: {error}"))
        .and_then(|output| match output.status.success() {
            | true => String::from_utf8(output.stdout).map_err(|error| format!("decode getconf PAGESIZE: {error}")),
            | false => Err(format!("getconf PAGESIZE failed: {}", output.status)),
        })
        .and_then(|value| value.trim().parse::<u64>().map_err(|error| format!("parse getconf PAGESIZE: {error}")));
    let xylem_version = Command::new(xylem)
        .arg("--version")
        .output()
        .map_err(|error| format!("run {} --version: {error}", xylem.display()))
        .and_then(|output| match output.status.success() {
            | true => String::from_utf8(output.stdout).map_err(|error| format!("decode Xylem version: {error}")),
            | false => Err(format!("{} --version failed: {}", xylem.display(), output.status)),
        })
        .map(|value| value.trim().to_string());
    kernel_release.and_then(|kernel_release| {
        cpu_model.and_then(|cpu_model| {
            total_memory_bytes.and_then(|total_memory_bytes| {
                page_size_bytes.and_then(|page_size_bytes| {
                    xylem_version.map(|xylem_version| HostIdentity {
                        architecture: std::env::consts::ARCH.to_string(),
                        available_parallelism: std::thread::available_parallelism().map_or(1, usize::from),
                        cgroup_version: 2,
                        cpu_model,
                        kernel_release,
                        page_size_bytes,
                        total_memory_bytes,
                        xylem_version,
                    })
                })
            })
        })
    })
}

pub(crate) fn measure(manifest: &Manifest, options: &MeasureOptions<'_>) -> Result<Vec<Observation>, String> {
    validate_environment(options).and_then(|host| {
        create_dir_all(options.work_root)
            .map_err(|error| format!("create work root {}: {error}", options.work_root.display()))
            .and_then(|()| {
                manifest
                    .cases
                    .iter()
                    .map(|case| measure_case(case, &host, options))
                    .chain(manifest.mixes.iter().map(|mix| {
                        let cases = mix.case_ids.iter().filter_map(|case_id| manifest.case(case_id)).collect::<Vec<_>>();
                        measure_subject(
                            &Subject {
                                cases: &cases,
                                estimator_key: &mix.estimator_key,
                                partition: mix.partition,
                                repetitions: MIX_REPETITIONS,
                                subject_id: &mix.mix_id,
                                subject_type: SubjectType::Mix,
                            },
                            &host,
                            options,
                        )
                    }))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|observations| observations.into_iter().flatten().collect())
            })
    })
}

fn measure_case(case: &Case, host: &HostIdentity, options: &MeasureOptions<'_>) -> Result<Vec<Observation>, String> {
    let cases = [case];
    measure_subject(
        &Subject {
            cases: &cases,
            estimator_key: &case.estimator_key,
            partition: case.partition,
            repetitions: CASE_REPETITIONS,
            subject_id: &case.case_id,
            subject_type: SubjectType::Case,
        },
        host,
        options,
    )
}

fn measure_subject(subject: &Subject<'_>, host: &HostIdentity, options: &MeasureOptions<'_>) -> Result<Vec<Observation>, String> {
    let config = subject
        .cases
        .first()
        .map(|case| case.config.as_path())
        .ok_or_else(|| "measurement subject has no configuration".to_string());
    config.and_then(|config| {
        validate_config(config).and_then(|()| config_checksum(config)).and_then(|checksum| {
            (0..WARMUP_REPETITIONS)
                .try_for_each(|repetition| run_subject(subject.subject_id, subject.cases, repetition, true, options).map(|_| ()))
                .and_then(|()| {
                    (0..subject.repetitions)
                        .map(|repetition| {
                            run_subject(subject.subject_id, subject.cases, repetition, false, options).map(|measurement| Observation {
                                anon_peak_bytes: measurement.anon_peak_bytes,
                                command_succeeded: measurement.exit_status.success()
                                    && measurement.oom_events == 0
                                    && measurement.oom_kill_events == 0,
                                concurrency: subject.cases.len().max(1),
                                config_sha256: checksum.clone(),
                                duration_millis: measurement.duration_millis,
                                estimator_key: subject.estimator_key.to_string(),
                                exit_code: measurement.exit_status.code(),
                                file_peak_bytes: measurement.file_peak_bytes,
                                host: host.clone(),
                                observation_id: format!("{:?}-{}-{repetition}", subject.subject_type, subject.subject_id).to_lowercase(),
                                oom_events: measurement.oom_events,
                                oom_kill_events: measurement.oom_kill_events,
                                partition: subject.partition,
                                peak_bytes: measurement.peak_bytes,
                                repetition,
                                sampled_current_peak_bytes: measurement.sampled_current_peak_bytes,
                                schema_version: OBSERVATION_SCHEMA_VERSION,
                                subject_id: subject.subject_id.to_string(),
                                subject_type: subject.subject_type,
                            })
                        })
                        .collect()
                })
        })
    })
}

#[cfg(target_os = "linux")]
fn parse_metric(path: &Path) -> Result<u64, String> {
    read_to_string(path)
        .map_err(|error| format!("read cgroup metric {}: {error}", path.display()))
        .and_then(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| format!("parse cgroup metric {}: {error}", path.display()))
        })
}

#[cfg(any(target_os = "linux", test))]
fn parse_metrics(path: &Path) -> Result<Vec<(String, u64)>, String> {
    read_to_string(path)
        .map_err(|error| format!("read cgroup metrics {}: {error}", path.display()))
        .and_then(|content| {
            content
                .lines()
                .map(|line| {
                    let mut fields = line.split_whitespace();
                    match (fields.next(), fields.next(), fields.next()) {
                        | (Some(name), Some(value), None) => value
                            .parse::<u64>()
                            .map(|value| (name.to_string(), value))
                            .map_err(|error| format!("parse cgroup metric {}: {error}", path.display())),
                        | _ => Err(format!("invalid cgroup metric line in {}: {line}", path.display())),
                    }
                })
                .collect()
        })
}

pub(crate) fn probe(options: &ProbeOptions<'_>) -> Result<ProbeObservation, String> {
    let valid_request = match (options.parent_mib > 0, options.child_mib > 0, options.hold_millis > 0) {
        | (true, true, true) => Ok(()),
        | _ => Err("synthetic probe sizes and hold duration must be greater than zero".to_string()),
    };
    let expected_minimum_bytes = options
        .parent_mib
        .checked_add(options.child_mib)
        .and_then(|mib| mib.checked_mul(1024 * 1024))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| "synthetic allocation size overflows bytes".to_string());
    valid_request
        .and_then(|()| validate_cgroup_root(options.cgroup_root))
        .and_then(|()| match options.executable.is_file() {
            | true => Ok(()),
            | false => Err(format!("synthetic probe executable does not exist: {}", options.executable.display())),
        })
        .and(expected_minimum_bytes)
        .and_then(|expected_minimum_bytes| {
            let arguments = [
                options.parent_mib.to_string(),
                options.child_mib.to_string(),
                options.hold_millis.to_string(),
            ];
            run_in_cgroup(
                options.executable,
                &arguments,
                options.cgroup_root,
                &format!("xylem-memory-synthetic-{}", std::process::id()),
            )
            .and_then(
                |measurement| match (measurement.exit_status.success(), measurement.peak_bytes >= expected_minimum_bytes) {
                    | (true, true) => Ok(ProbeObservation {
                        anon_peak_bytes: measurement.anon_peak_bytes,
                        expected_minimum_bytes,
                        file_peak_bytes: measurement.file_peak_bytes,
                        peak_bytes: measurement.peak_bytes,
                        sampled_current_peak_bytes: measurement.sampled_current_peak_bytes,
                    }),
                    | (false, _) => Err(format!("synthetic probe failed: {}", measurement.exit_status)),
                    | (true, false) => Err(format!(
                        "synthetic process-tree peak {} did not include the requested {} bytes",
                        measurement.peak_bytes, expected_minimum_bytes
                    )),
                },
            )
        })
}

#[cfg(target_os = "linux")]
fn release_child(child: &mut Child) -> Result<(), String> {
    child.stdin.take().map_or_else(
        || Err("measurement barrier stdin is unavailable".to_string()),
        |mut stdin| stdin.write_all(b"start\n").map_err(|error| format!("release measurement child: {error}")),
    )
}

fn run_in_cgroup(command: &Path, arguments: &[String], cgroup_root: &Path, cgroup_name: &str) -> Result<CommandMeasurement, String> {
    #[cfg(target_os = "linux")]
    {
        let cgroup = cgroup_root.join(cgroup_name);
        create_dir(&cgroup)
            .map_err(|error| format!("create measurement cgroup {}: {error}", cgroup.display()))
            .and_then(|()| {
                let mut child = Command::new("/bin/sh")
                    .args(["-c", "IFS= read -r _; exec \"$@\"", "xylem-memory-spike"])
                    .arg(command)
                    .args(arguments)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|error| format!("spawn measurement command {}: {error}", command.display()));
                let result = match &mut child {
                    | Ok(child) => write(cgroup.join("cgroup.procs"), child.id().to_string())
                        .map_err(|error| format!("move measurement child into {}: {error}", cgroup.display()))
                        .and_then(|()| release_child(child))
                        .and_then(|()| sample_child(child, &cgroup)),
                    | Err(error) => Err(error.clone()),
                }
                .inspect_err(|_| {
                    if let Ok(child) = &mut child {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                });
                let cleanup = remove_dir(&cgroup).map_err(|error| format!("remove measurement cgroup {}: {error}", cgroup.display()));
                match (result, cleanup) {
                    | (Ok(measurement), Ok(())) => Ok(measurement),
                    | (Err(error), Ok(())) => Err(error),
                    | (Ok(_), Err(error)) => Err(error),
                    | (Err(error), Err(cleanup)) => Err(format!("{error}; {cleanup}")),
                }
            })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (command, arguments, cgroup_root, cgroup_name);
        Err("memory measurement requires Linux cgroup v2".to_string())
    }
}

fn run_subject(
    subject_id: &str,
    cases: &[&Case],
    repetition: usize,
    warmup: bool,
    options: &MeasureOptions<'_>,
) -> Result<CommandMeasurement, String> {
    let suffix = match warmup {
        | true => format!("warmup-{repetition}"),
        | false => format!("run-{repetition}"),
    };
    let run_id = format!("{}-{subject_id}-{suffix}", std::process::id());
    let run_root = options.work_root.join(&run_id);
    let artifact_root = run_root.join("artifacts");
    let config = run_root.join("config.json");
    let report_root = run_root.join("reports");
    create_dir(&run_root)
        .map_err(|error| format!("create measurement run directory {}: {error}", run_root.display()))
        .and_then(|()| {
            let result = create_dir(&artifact_root)
                .map_err(|error| format!("create artifact directory {}: {error}", artifact_root.display()))
                .and_then(|()| create_dir(&report_root).map_err(|error| format!("create report directory {}: {error}", report_root.display())))
                .and_then(|()| {
                    cases
                        .first()
                        .ok_or_else(|| "measurement subject must contain a case".to_string())
                        .and_then(|first| write_subject_config(&first.config, &config, &artifact_root, cases.len() > 1))
                })
                .and_then(|()| build_arguments(cases, &config, &report_root))
                .and_then(|arguments| run_in_cgroup(options.xylem, &arguments, options.cgroup_root, &format!("xylem-memory-{run_id}")));
            let cleanup = remove_dir_all(&run_root).map_err(|error| format!("remove measurement run directory {}: {error}", run_root.display()));
            match (result, cleanup) {
                | (Ok(measurement), Ok(())) => Ok(measurement),
                | (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
                | (Err(error), Err(cleanup)) => Err(format!("{error}; {cleanup}")),
            }
        })
}

#[cfg(target_os = "linux")]
fn sample(cgroup: &Path, peaks: &mut SamplePeaks) -> Result<(), String> {
    parse_metric(&cgroup.join("memory.current")).and_then(|current| {
        parse_metrics(&cgroup.join("memory.stat")).map(|statistics| {
            let anon = statistics.iter().find(|(name, _)| name == "anon").map_or(0, |(_, value)| *value);
            let file = statistics.iter().find(|(name, _)| name == "file").map_or(0, |(_, value)| *value);
            peaks.anon_bytes = peaks.anon_bytes.max(anon);
            peaks.current_bytes = peaks.current_bytes.max(current);
            peaks.file_bytes = peaks.file_bytes.max(file);
        })
    })
}

#[cfg(target_os = "linux")]
fn sample_child(child: &mut Child, cgroup: &Path) -> Result<CommandMeasurement, String> {
    let started = Instant::now();
    let mut peaks = SamplePeaks::default();
    let status = loop {
        let sampled = sample(cgroup, &mut peaks);
        match (sampled, child.try_wait()) {
            | (Err(error), _) => break Err(error),
            | (Ok(()), Err(error)) => break Err(format!("wait for measurement child: {error}")),
            | (Ok(()), Ok(Some(status))) => break Ok(status),
            | (Ok(()), Ok(None)) => sleep(SAMPLE_INTERVAL),
        }
    };
    status.and_then(|exit_status| {
        sample(cgroup, &mut peaks)
            .and_then(|()| parse_metric(&cgroup.join("memory.peak")))
            .and_then(|peak_bytes| {
                parse_metrics(&cgroup.join("memory.events")).map(|events| {
                    let event = |name: &str| events.iter().find(|(key, _)| key == name).map_or(0, |(_, value)| *value);
                    let duration_millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    CommandMeasurement {
                        anon_peak_bytes: peaks.anon_bytes,
                        duration_millis,
                        exit_status,
                        file_peak_bytes: peaks.file_bytes,
                        oom_events: event("oom"),
                        oom_kill_events: event("oom_kill"),
                        peak_bytes,
                        sampled_current_peak_bytes: peaks.current_bytes,
                    }
                })
            })
    })
}

fn validate_cgroup_root(cgroup_root: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let probe = cgroup_root.join(format!("xylem-memory-probe-{}-{nonce}", std::process::id()));
        let controllers = read_to_string(cgroup_root.join("cgroup.controllers"))
            .map_err(|error| format!("read delegated cgroup controllers at {}: {error}", cgroup_root.display()));
        controllers
            .and_then(
                |controllers| match controllers.split_whitespace().any(|controller| controller == "memory") {
                    | true => Ok(()),
                    | false => Err(format!("memory controller is unavailable at {}", cgroup_root.display())),
                },
            )
            .and_then(|()| create_dir(&probe).map_err(|error| format!("create delegated cgroup probe {}: {error}", probe.display())))
            .and_then(|()| match (probe.join("memory.current").is_file(), probe.join("memory.peak").is_file()) {
                | (true, true) => remove_dir(&probe).map_err(|error| format!("remove delegated cgroup probe {}: {error}", probe.display())),
                | _ => {
                    let _ = remove_dir(&probe);
                    Err(format!(
                        "delegated cgroup {} does not expose memory.current and memory.peak",
                        probe.display()
                    ))
                }
            })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cgroup_root;
        Err("memory measurement requires Linux cgroup v2".to_string())
    }
}

fn validate_config(path: &Path) -> Result<(), String> {
    read_to_string(path)
        .map_err(|error| format!("read measurement config {}: {error}", path.display()))
        .and_then(|content| {
            serde_json::from_str::<serde_json::Value>(&content).map_err(|error| format!("parse measurement config {}: {error}", path.display()))
        })
        .and_then(|configuration| match configuration.pointer("/options/batch") {
            | Some(value) if !value.is_null() => Err(format!("measurement config {} must not set options.batch", path.display())),
            | _ => Ok(()),
        })
}

#[cfg(target_os = "linux")]
fn validate_environment(options: &MeasureOptions<'_>) -> Result<HostIdentity, String> {
    validate_cgroup_root(options.cgroup_root).and_then(|()| match options.xylem.is_file() {
        | true => host_identity(options.xylem),
        | false => Err(format!("Xylem executable does not exist: {}", options.xylem.display())),
    })
}

#[cfg(not(target_os = "linux"))]
fn validate_environment(options: &MeasureOptions<'_>) -> Result<HostIdentity, String> {
    let _ = options;
    Err("memory measurement requires Linux cgroup v2".to_string())
}

#[cfg(test)]
mod tests {
    #![expect(clippy::indexing_slicing, reason = "test indexing deliberately fails on malformed fixtures")]
    use super::*;
    use crate::model::InputFeatures;

    fn test_case(input: Option<&str>) -> Case {
        Case {
            case_id: "case".to_string(),
            config: PathBuf::from("config.json"),
            estimator_key: "estimator".to_string(),
            features: InputFeatures {
                decoded_bytes: 0,
                file_count: 0,
                input_bytes: 0,
                rasters: Vec::new(),
            },
            input: input.map(PathBuf::from),
            partition: Partition::Training,
        }
    }
    #[test]
    fn arguments_are_deterministic_and_bound_concurrency() {
        let first = test_case(Some("one"));
        let mut second = test_case(Some("two"));
        second.case_id = "second".to_string();
        let arguments = build_arguments(&[&first, &second], Path::new("resolved.json"), Path::new("reports")).expect("valid arguments");
        assert_eq!(
            arguments,
            [
                "--threads",
                "2",
                "--no-progress",
                "run",
                "resolved.json",
                "--report-output",
                "reports",
                "--input",
                "one,two",
            ]
        );
    }
    #[test]
    fn arguments_reject_comma_delimited_paths() {
        let case = test_case(Some("one,two"));
        assert!(build_arguments(&[&case], Path::new("resolved.json"), Path::new("reports")).is_err());
    }
    #[test]
    fn concurrent_config_isolates_outputs_by_input_name() {
        let root = std::env::temp_dir().join(format!("xylem-memory-config-{}", std::process::id()));
        create_dir_all(&root).expect("create config test directory");
        let source = root.join("source.json");
        let destination = root.join("resolved.json");
        write(
            &source,
            r#"{"modules":[{"name":"one","variables":{"EXISTING":"value"}},{"name":"two","variables":null}],"requirements":[]}"#,
        )
        .expect("write source config");

        write_subject_config(&source, &destination, Path::new("/work/artifacts"), true).expect("write concurrent config");

        let config: Value = serde_json::from_str(&read_to_string(&destination).expect("read concurrent config")).expect("parse concurrent config");
        assert_eq!(config["output"], "/work/artifacts");
        assert_eq!(config["variables"]["OUTPUT_DIRECTORY"], "/work/artifacts/{{ PARENT_DIRECTORY }}");
        assert_eq!(config["modules"][0]["variables"]["EXISTING"], "value");
        assert_eq!(
            config["modules"][0]["variables"]["OUTPUT_DIRECTORY"],
            "/work/artifacts/{{ PARENT_DIRECTORY }}"
        );
        assert_eq!(
            config["modules"][1]["variables"]["OUTPUT_DIRECTORY"],
            "/work/artifacts/{{ PARENT_DIRECTORY }}"
        );
        remove_dir_all(root).expect("remove config test directory");
    }
    #[test]
    fn metric_parser_rejects_malformed_content() {
        let root = std::env::temp_dir().join(format!("xylem-memory-metric-{}", std::process::id()));
        create_dir_all(&root).expect("create metric test directory");
        let path = root.join("memory.stat");
        std::fs::write(&path, "anon nope\n").expect("write invalid metric");
        assert!(parse_metrics(&path).is_err());
        remove_dir_all(root).expect("remove metric test directory");
    }
    #[test]
    fn probe_rejects_empty_allocations_before_platform_validation() {
        let result = probe(&ProbeOptions {
            child_mib: 1,
            cgroup_root: Path::new("missing-cgroup"),
            executable: Path::new("missing-probe"),
            hold_millis: 1,
            parent_mib: 0,
        });
        assert_eq!(
            result.err().as_deref(),
            Some("synthetic probe sizes and hold duration must be greater than zero")
        );
    }
}
