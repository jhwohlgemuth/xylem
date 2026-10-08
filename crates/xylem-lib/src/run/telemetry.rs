//! Lightweight one-second host and Xylem process-tree sampling.
extern crate alloc;

use super::store::{timestamp, Bundle, MetricSample, StoreError};
use acorn_schema::hardware::Memory;
use alloc::sync::Arc;
use core::iter::successors;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::thread::{self, JoinHandle};
use sysinfo::{get_current_pid, Disks, Networks, Pid, ProcessesToUpdate, System};

/// Background telemetry sampler stopped and joined by its owner.
pub struct SystemMonitor {
    handle: JoinHandle<Result<(), StoreError>>,
    stop: Arc<AtomicBool>,
}

impl SystemMonitor {
    /// Start one-second sampling for a run bundle.
    pub fn start(bundle: Bundle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut system = System::new_all();
            let mut disks = Disks::new_with_refreshed_list();
            let mut networks = Networks::new_with_refreshed_list();
            let root = get_current_pid().ok();
            let mut result = Ok(());
            while !thread_stop.load(Ordering::Acquire) && result.is_ok() {
                system.refresh_cpu_usage();
                system.refresh_memory();
                system.refresh_processes(ProcessesToUpdate::All, true);
                disks.refresh(true);
                networks.refresh(true);
                let processes = root.map_or_else(Vec::new, |root| {
                    system
                        .processes()
                        .iter()
                        .filter(|(pid, _)| belongs_to_tree(**pid, root, &system))
                        .map(|(_, process)| process)
                        .collect::<Vec<_>>()
                });
                let load = System::load_average();
                let memory = |bytes| Memory::kb(bytes as f64 / 1_024.0);
                let sample = MetricSample::init()
                    .host_cpu_percent(system.global_cpu_usage())
                    .host_disk_read(memory(disks.list().iter().map(|disk| disk.usage().read_bytes).sum()))
                    .host_disk_written(memory(disks.list().iter().map(|disk| disk.usage().written_bytes).sum()))
                    .host_filesystem_available(memory(disks.list().iter().map(sysinfo::Disk::available_space).sum()))
                    .host_filesystem_total(memory(disks.list().iter().map(sysinfo::Disk::total_space).sum()))
                    .host_load_one(load.one)
                    .host_memory_used(memory(system.used_memory()))
                    .host_network_received(memory(networks.values().map(sysinfo::NetworkData::received).sum()))
                    .host_network_transmitted(memory(networks.values().map(sysinfo::NetworkData::transmitted).sum()))
                    .host_swap_used(memory(system.used_swap()))
                    .process_cpu_percent(processes.iter().map(|process| process.cpu_usage()).sum())
                    .process_count(processes.len() as u64)
                    .process_disk_read(memory(processes.iter().map(|process| process.disk_usage().read_bytes).sum()))
                    .process_disk_written(memory(processes.iter().map(|process| process.disk_usage().written_bytes).sum()))
                    .process_memory(memory(processes.iter().map(|process| process.memory()).sum()))
                    .timestamp(timestamp())
                    .build();
                result = bundle.append_json_line("metrics.jsonl", &sample);
                (0..10)
                    .take_while(|_| !thread_stop.load(Ordering::Acquire))
                    .for_each(|_| thread::sleep(Duration::from_millis(100)));
            }
            result
        });
        Self { handle, stop }
    }
    /// Stop sampling and return any durable-write or thread failure.
    pub fn stop(self) -> Result<(), StoreError> {
        self.stop.store(true, Ordering::Release);
        self.handle.join().unwrap_or_else(|_| {
            Err(StoreError::Io {
                path: std::path::PathBuf::from("metrics.jsonl"),
                source: std::io::Error::other("system monitor thread panicked"),
            })
        })
    }
}

fn belongs_to_tree(pid: Pid, root: Pid, system: &System) -> bool {
    successors(Some(pid), |pid| system.process(*pid).and_then(|process| process.parent()))
        .take(system.processes().len().saturating_add(1))
        .any(|pid| pid == root)
}
