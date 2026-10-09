#[cfg(any(target_os = "linux", test))]
use acorn_schema::hardware::Memory;

#[cfg(any(target_os = "linux", test))]
use std::fs::read_to_string;
#[cfg(any(target_os = "linux", test))]
use std::io::ErrorKind;
#[cfg(any(target_os = "linux", test))]
use std::path::{Component, Path};

#[cfg(target_os = "linux")]
pub(super) fn observed_peak(process_id: u32) -> Result<Option<Memory>, String> {
    observed_peak_at(Path::new("/proc"), Path::new("/sys/fs/cgroup"), process_id)
}

#[cfg(any(target_os = "linux", test))]
fn observed_peak_at(proc_root: &Path, cgroup_root: &Path, process_id: u32) -> Result<Option<Memory>, String> {
    let cgroup_path = proc_root.join(process_id.to_string()).join("cgroup");
    read_optional(&cgroup_path).and_then(|contents| {
        contents.map_or(Ok(None), |contents| {
            contents
                .lines()
                .find_map(|line| {
                    let mut fields = line.splitn(3, ':');
                    match (fields.next(), fields.next(), fields.next()) {
                        | (Some("0"), Some(""), Some(path)) => Some(path),
                        | _ => None,
                    }
                })
                .map_or(Ok(None), |path| peak_file(cgroup_root, path).and_then(|path| read_peak(&path)))
        })
    })
}

#[cfg(any(target_os = "linux", test))]
fn peak_file(cgroup_root: &Path, path: &str) -> Result<std::path::PathBuf, String> {
    Path::new(path)
        .strip_prefix("/")
        .map_err(|_| format!("unified cgroup path is not absolute: {path}"))
        .and_then(|relative| {
            match relative.components().all(|component| matches!(component, Component::Normal(_))) || relative.as_os_str().is_empty() {
                | true => Ok(cgroup_root.join(relative).join("memory.peak")),
                | false => Err(format!("unified cgroup path escapes its root: {path}")),
            }
        })
}

#[cfg(any(target_os = "linux", test))]
fn read_optional(path: &Path) -> Result<Option<String>, String> {
    read_to_string(path).map(Some).or_else(|why| match why.kind() {
        | ErrorKind::NotFound => Ok(None),
        | _ => Err(format!("read {}: {why}", path.display())),
    })
}

#[cfg(any(target_os = "linux", test))]
fn read_peak(path: &Path) -> Result<Option<Memory>, String> {
    read_optional(path).and_then(|value| {
        value.map_or(Ok(None), |value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|why| format!("parse {}: {why}", path.display()))
                .and_then(|bytes| match bytes {
                    | 0 => Ok(None),
                    | bytes => {
                        let memory = Memory::kb(bytes as f64 / 1_024.0);
                        match memory.checked_bytes() == Some(bytes) {
                            | true => Ok(Some(memory)),
                            | false => Err(format!("{} cannot represent {bytes} bytes exactly", path.display())),
                        }
                    }
                })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::observed_peak_at;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::path::PathBuf;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn root(name: &str) -> PathBuf {
        let suffix = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("xylem-sandbox-memory-{}-{suffix}-{name}", std::process::id()))
    }

    #[test]
    fn exact_unified_peak_is_returned() {
        let root = root("exact");
        let proc_root = root.join("proc");
        let cgroup_root = root.join("cgroup");
        create_dir_all(proc_root.join("42")).expect("create proc fixture");
        create_dir_all(cgroup_root.join("system.slice/xylem.scope")).expect("create cgroup fixture");
        write(proc_root.join("42/cgroup"), "0::/system.slice/xylem.scope\n").expect("write cgroup membership");
        write(cgroup_root.join("system.slice/xylem.scope/memory.peak"), "123456\n").expect("write memory peak");

        let peak = observed_peak_at(&proc_root, &cgroup_root, 42).expect("read peak");

        assert_eq!(peak.and_then(|memory| memory.checked_bytes()), Some(123_456));
        remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn escaping_unified_path_is_rejected() {
        let root = root("escape");
        let proc_root = root.join("proc");
        let cgroup_root = root.join("cgroup");
        create_dir_all(proc_root.join("42")).expect("create proc fixture");
        write(proc_root.join("42/cgroup"), "0::/../../outside\n").expect("write cgroup membership");

        let result = observed_peak_at(&proc_root, &cgroup_root, 42);

        assert!(result.is_err_and(|why| why.contains("escapes its root")));
        remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn missing_membership_is_unavailable() {
        let root = root("missing");

        let peak = observed_peak_at(&root.join("proc"), &root.join("cgroup"), 42).expect("missing peak is optional");

        assert_eq!(peak, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a live Linux cgroup v2 hierarchy"]
    fn live_unified_peak_is_positive() {
        let peak = super::observed_peak(std::process::id()).expect("read live cgroup peak");

        assert!(
            peak.as_ref().and_then(|memory| memory.checked_bytes()).is_some_and(|bytes| bytes > 0),
            "live cgroup v2 membership should expose a positive memory.peak"
        );
    }
}
