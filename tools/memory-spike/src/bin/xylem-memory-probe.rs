//! Synthetic process-tree allocator for validating cgroup memory measurement.
use core::hint::black_box;
use core::time::Duration;
use std::env;
use std::process::{exit, Command};
use std::thread::sleep;

const MIB: usize = 1024 * 1024;

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let result = match arguments.first().map(String::as_str) {
        | Some("--child") => arguments
            .get(1..)
            .ok_or_else(|| "missing child arguments".to_string())
            .and_then(parse_values)
            .and_then(|(memory, _, hold)| allocate(memory, hold)),
        | _ => parse_values(&arguments).and_then(|(parent, child, hold)| run_parent(parent, child, hold)),
    };
    if let Err(error) = result {
        eprintln!("Error: {error}");
        exit(exitcode::DATAERR);
    }
}
fn allocate(memory_mib: usize, hold_millis: u64) -> Result<(), String> {
    resident_memory(memory_mib).map(|memory| {
        black_box(&memory);
        sleep(Duration::from_millis(hold_millis));
    })
}
fn resident_memory(memory_mib: usize) -> Result<Vec<u8>, String> {
    memory_mib
        .checked_mul(MIB)
        .ok_or_else(|| "requested allocation overflows usize".to_string())
        .map(|bytes| {
            let mut memory = vec![0_u8; bytes];
            memory.iter_mut().step_by(4096).for_each(|value| *value = 1);
            memory
        })
}
fn parse_values(arguments: &[String]) -> Result<(usize, usize, u64), String> {
    match arguments {
        | [parent, child, hold] => parent
            .parse::<usize>()
            .map_err(|error| format!("invalid parent MiB: {error}"))
            .and_then(|parent| {
                child
                    .parse::<usize>()
                    .map_err(|error| format!("invalid child MiB: {error}"))
                    .and_then(|child| {
                        hold.parse::<u64>()
                            .map_err(|error| format!("invalid hold milliseconds: {error}"))
                            .map(|hold| (parent, child, hold))
                    })
            }),
        | _ => Err("usage: xylem-memory-probe PARENT_MIB CHILD_MIB HOLD_MILLIS".to_string()),
    }
}
fn run_parent(parent_mib: usize, child_mib: usize, hold_millis: u64) -> Result<(), String> {
    resident_memory(parent_mib).and_then(|memory| {
        env::current_exe()
            .map_err(|error| format!("resolve probe executable: {error}"))
            .and_then(|executable| {
                Command::new(executable)
                    .args(["--child", &child_mib.to_string(), "0", &hold_millis.to_string()])
                    .spawn()
                    .map_err(|error| format!("spawn child allocator: {error}"))
            })
            .and_then(|mut child| {
                black_box(&memory);
                child
                    .wait()
                    .map_err(|error| format!("wait for child allocator: {error}"))
                    .and_then(|status| match status.success() {
                        | true => Ok(()),
                        | false => Err(format!("child allocator failed: {status}")),
                    })
            })
    })
}
