use super::store::{BundleStatus, Manifest, Store, TaskLogSink};
use super::RunCancellation;
use crate::journal::JOURNAL_SCHEMA_VERSION;
use acorn_cmd::{OutputSink, OutputStream};
use std::fs::{create_dir_all, read_to_string, write};
use std::path::PathBuf;

fn temporary_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-run-store-{name}-{}", std::process::id()))
}

#[test]
fn test_cancellation_is_graceful_then_forced() {
    let cancellation = RunCancellation::default();
    cancellation.request();
    assert!(cancellation.is_requested());
    assert!(!cancellation.token().is_cancelled());
    cancellation.request();
    assert!(cancellation.token().is_cancelled());
}

#[test]
fn test_history_marks_unfinished_and_corrupt_bundles() {
    let root = temporary_root("history");
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("store should initialize");
    let config = root.join("workflow.json");
    write(&config, "{}").expect("config should write");
    let manifest = Manifest::from_config("run-a".to_string(), &config, 2).expect("manifest should build");
    store.start(&manifest).expect("bundle should start");
    let corrupt = root.join("run-b");
    create_dir_all(&corrupt).expect("corrupt directory should create");
    write(corrupt.join("manifest.json"), "not json").expect("corrupt manifest should write");
    let mut unsupported = Manifest::from_config("run-c".to_string(), &config, 2).expect("manifest should build");
    unsupported.journal_schema_version = JOURNAL_SCHEMA_VERSION.saturating_sub(1);
    store.start(&unsupported).expect("unsupported bundle should start");
    let runs = store.list().expect("history should load");
    assert_eq!(runs.len(), 3);
    assert!(runs.iter().any(|run| run.run_id == "run-a" && run.status == BundleStatus::Running));
    assert!(runs.iter().any(|run| run.run_id == "run-b" && run.status == BundleStatus::Corrupt));
    assert!(runs.iter().any(|run| run.run_id == "run-c" && run.status == BundleStatus::Unsupported));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_store_does_not_overwrite_run_identifier() {
    let root = temporary_root("exclusive");
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("store should initialize");
    let config = root.join("workflow.json");
    write(&config, "{}").expect("config should write");
    let manifest = Manifest::from_config("run".to_string(), &config, 1).expect("manifest should build");
    store.start(&manifest).expect("first bundle should start");
    assert!(store.start(&manifest).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_task_log_sink_attributes_streams_and_flushes_fragments() {
    let root = temporary_root("logs");
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("store should initialize");
    let config = root.join("workflow.json");
    write(&config, "{}").expect("config should write");
    let manifest = Manifest::from_config("run".to_string(), &config, 1).expect("manifest should build");
    let bundle = store.start(&manifest).expect("bundle should start");
    let sink = TaskLogSink::module(bundle.clone(), 2, 3);
    sink.write(OutputStream::Stdout, b"first\npartial").expect("stdout should write");
    sink.write(OutputStream::Stderr, b"failure\n").expect("stderr should write");
    sink.flush().expect("fragments should flush");
    let records = read_to_string(bundle.path().join("logs/input-2/module-3.jsonl")).expect("task log should read");
    assert!(records.contains("\"stream\":\"stdout\""));
    assert!(records.contains("\"stream\":\"stderr\""));
    assert!(records.contains("partial"));
    let _ = std::fs::remove_dir_all(root);
}
