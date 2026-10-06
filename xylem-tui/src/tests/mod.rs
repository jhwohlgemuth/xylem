use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};
use ratatui::backend::TestBackend;
use xylem_lib::journal::{ConfigurationIdentity, FinalStatus, JournalInput, JournalModule, RunMetadata, SandboxEvidence};

#[test]
fn test_cancellation_requires_confirmation() {
    let root = std::env::temp_dir().join(format!("xylem-tui-cancel-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let mut dashboard = Dashboard::new(Some(PathBuf::from("workflow.json")), store, DiagnosticLog::default()).expect("dashboard should initialize");
    let calls = Arc::new(AtomicUsize::new(0));
    let cancel_calls = Arc::clone(&calls);
    let (_sender, completion) = mpsc::channel();
    dashboard.active = Some(LaunchHandle::new(completion, move || {
        cancel_calls.fetch_add(1, Ordering::AcqRel);
    }));
    dashboard.handle_key(KeyCode::Char('c'), &|_| panic!("launcher should not be called"));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    dashboard.handle_key(KeyCode::Char('c'), &|_| panic!("launcher should not be called"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_diagnostic_log_preserves_fragmented_events() {
    let mut diagnostics = DiagnosticLog::default();
    diagnostics.write_all(b"\x1b[3").expect("ANSI fragment should write");
    diagnostics
        .write_all(b"2mfirst event\x1b[0m\nsecond event\n")
        .expect("remaining events should write");

    assert_eq!(diagnostics.lines(), vec!["first event".to_string(), "second event".to_string()]);
}
#[test]
fn test_every_tab_renders_at_compact_size() {
    let root = std::env::temp_dir().join(format!("xylem-tui-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let mut dashboard = Dashboard::new(None, store, DiagnosticLog::default()).expect("dashboard should initialize");
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    for tab in [Tab::Overview, Tab::Workflow, Tab::System, Tab::Logs, Tab::Runs] {
        dashboard.tab = tab;
        terminal.draw(|frame| render(frame, &dashboard)).expect("tab should render");
    }
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_navigation_scrolls_content_and_selects_runs() {
    let root = std::env::temp_dir().join(format!("xylem-tui-navigation-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let config = root.join("workflow.json");
    std::fs::write(&config, "{}").expect("test configuration should write");
    ["run-a", "run-b"].into_iter().for_each(|run_id| {
        let manifest = Manifest::from_config(run_id.to_string(), &config, 1).expect("manifest should build");
        store.start(&manifest).expect("bundle should start");
    });
    let mut dashboard = Dashboard::new(None, store, DiagnosticLog::default()).expect("dashboard should initialize");
    dashboard.select_tab(Tab::Workflow);
    dashboard.handle_key(KeyCode::Char('j'), &|_| panic!("launcher should not be called"));
    assert_eq!(dashboard.scroll, 1);
    assert_eq!(dashboard.selected, 0);
    dashboard.select_tab(Tab::Runs);
    dashboard.handle_key(KeyCode::Char('j'), &|_| panic!("launcher should not be called"));
    assert_eq!(dashboard.scroll, 0);
    assert_eq!(dashboard.selected, 1);
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_overview_renders_run_failure() {
    let root = std::env::temp_dir().join(format!("xylem-tui-failure-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let config = root.join("workflow.json");
    std::fs::write(&config, "{}").expect("test configuration should write");
    let manifest = Manifest::from_config("run-failed".to_string(), &config, 1).expect("manifest should build");
    let bundle = store.start(&manifest).expect("bundle should start");
    let metadata = RunMetadata::now(
        "run-failed".to_string(),
        ConfigurationIdentity {
            path: config.display().to_string(),
            id: None,
            version: None,
        },
        false,
        SandboxEvidence::off(),
    );
    let journal = RunJournal::create(
        bundle.journal_path(),
        metadata,
        vec![JournalInput::new(0, Some("alpha".to_string()))],
        Vec::new(),
    )
    .expect("journal should create");
    journal
        .finish(FinalStatus::Failed, Some("prepare containers failed"))
        .expect("journal should finish");
    let summary = RunJournal::read(bundle.journal_path()).expect("summary should recover");
    bundle.write_summary(&summary).expect("summary should write");
    let mut diagnostics = DiagnosticLog::default();
    diagnostics
        .write_all(b"INFO workflow preparation started\n")
        .expect("diagnostic event should write");
    let dashboard = Dashboard::new(None, store, diagnostics).expect("dashboard should initialize");
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal.draw(|frame| render(frame, &dashboard)).expect("overview should render");
    let display = terminal.backend().to_string();
    assert!(display.contains("failed"));
    assert!(display.contains("prepare containers failed"));
    assert!(display.contains("workflow preparation started"));
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_system_renders_metric_charts() {
    let root = std::env::temp_dir().join(format!("xylem-tui-system-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let config = root.join("workflow.json");
    std::fs::write(&config, "{}").expect("test configuration should write");
    let manifest = Manifest::from_config("run-system".to_string(), &config, 1).expect("manifest should build");
    let bundle = store.start(&manifest).expect("bundle should start");
    std::fs::write(
        bundle.path().join("metrics.jsonl"),
        r#"{"host_cpu_percent":42.0,"host_disk_read":"16KB","host_disk_written":"8KB","host_filesystem_available":"1048576KB","host_filesystem_total":"2097152KB","host_load_one":1.5,"host_memory_used":"4194304KB","host_network_received":"4KB","host_network_transmitted":"2KB","host_swap_used":"0KB","process_cpu_percent":8.0,"process_count":3,"process_disk_read":"1KB","process_disk_written":"1KB","process_memory":"65536KB","timestamp":1}
"#,
    )
    .expect("metric sample should write");
    let mut dashboard = Dashboard::new(None, store, DiagnosticLog::default()).expect("dashboard should initialize");
    dashboard.tab = Tab::System;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal.draw(|frame| render(frame, &dashboard)).expect("system charts should render");
    let display = terminal.backend().to_string();
    assert!(display.contains("HOST CPU"), "{display}");
    assert!(display.contains("XYLEM RSS"), "{display}");
    assert!(display.contains("NETWORK I/O"), "{display}");
    assert!(display.contains("DISK I/O"), "{display}");
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_tab_cycles_in_keyboard_order() {
    let root = std::env::temp_dir().join(format!("xylem-tui-tabs-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let mut dashboard = Dashboard::new(None, store, DiagnosticLog::default()).expect("dashboard should initialize");
    [Tab::Workflow, Tab::System, Tab::Logs, Tab::Runs, Tab::Overview]
        .into_iter()
        .for_each(|expected| {
            dashboard.scroll = 7;
            dashboard.handle_key(KeyCode::Tab, &|_| panic!("launcher should not be called"));
            assert_eq!(dashboard.tab, expected);
            assert_eq!(dashboard.scroll, 0);
        });
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn test_workflow_renders_chart_and_table() {
    let root = std::env::temp_dir().join(format!("xylem-tui-workflow-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(root.clone()).expect("test store should initialize");
    let config = root.join("workflow.json");
    std::fs::write(&config, "{}").expect("test configuration should write");
    let manifest = Manifest::from_config("run-workflow".to_string(), &config, 1).expect("manifest should build");
    let bundle = store.start(&manifest).expect("bundle should start");
    let metadata = RunMetadata::now(
        "run-workflow".to_string(),
        ConfigurationIdentity {
            path: config.display().to_string(),
            id: None,
            version: None,
        },
        false,
        SandboxEvidence::off(),
    );
    let journal = RunJournal::create(
        bundle.journal_path(),
        metadata,
        vec![JournalInput::new(0, Some("alpha".to_string()))],
        vec![JournalModule::new(
            0,
            "Module".to_string(),
            vec!["Group".to_string(), "Module".to_string()],
        )],
    )
    .expect("journal should create");
    journal.transition_input(0, ExecutionState::Running, None).expect("input should start");
    journal
        .transition_module(0, 0, ExecutionState::Running, None)
        .expect("module should start");
    journal
        .transition_module(0, 0, ExecutionState::Succeeded, None)
        .expect("module should finish");
    let mut dashboard = Dashboard::new(None, store, DiagnosticLog::default()).expect("dashboard should initialize");
    dashboard.tab = Tab::Workflow;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &dashboard))
        .expect("workflow visualization should render");
    let display = terminal.backend().to_string();
    assert!(display.contains("STEP STATES"), "{display}");
    assert!(display.contains("EXECUTION TABLE"), "{display}");
    assert!(display.contains("Group::Module"), "{display}");
    let _ = std::fs::remove_dir_all(root);
}
