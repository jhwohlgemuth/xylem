//! Local terminal dashboard for Xylem workflow runs
extern crate alloc;

use alloc::sync::Arc;
use core::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::{Frame, Terminal};
use std::env;
use std::fs::{read_dir, read_to_string};
use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use xylem_lib::journal::RunJournal;
use xylem_lib::projection::DurationHistory;
use xylem_lib::run::store::{Manifest, MetricSample, Store, StoreError, StoredRun, TaskLogRecord};

const TICK: Duration = Duration::from_millis(200);
/// Failure while initializing or running the dashboard.
#[derive(Debug, Error)]
pub enum DashboardError {
    /// Terminal input or rendering failed.
    #[error("terminal dashboard failed: {0}")]
    Io(#[from] io::Error),
    /// Run history could not be loaded.
    #[error(transparent)]
    Store(#[from] StoreError),
}
/// Dashboard tab identifiers in keyboard order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    /// Run summary and recent activity.
    Overview,
    /// Per-input and per-module workflow state.
    Workflow,
    /// Host and process telemetry.
    System,
    /// Durable task output.
    Logs,
    /// Retained run history.
    Runs,
}
struct Dashboard {
    active: Option<LaunchHandle>,
    config: Option<PathBuf>,
    confirm_cancel: bool,
    follow_logs: bool,
    help: bool,
    message: String,
    quit: bool,
    runs: Vec<StoredRun>,
    selected: usize,
    store: Store,
    tab: Tab,
}
/// Handle returned by a caller-owned workflow launcher.
pub struct LaunchHandle {
    cancel: Arc<dyn Fn() + Send + Sync>,
    completion: mpsc::Receiver<Result<(), String>>,
}
struct TerminalGuard;
impl Dashboard {
    fn new(config: Option<PathBuf>, store: Store) -> Result<Self, StoreError> {
        store.list().map(|runs| Self {
            active: None,
            config,
            confirm_cancel: false,
            follow_logs: true,
            help: false,
            message: "n new run · ? help · q quit".to_string(),
            quit: false,
            runs,
            selected: 0,
            store,
            tab: Tab::Overview,
        })
    }
    fn active_run(&self) -> Option<&StoredRun> {
        self.runs.get(self.selected)
    }
    fn handle_key<F>(&mut self, code: KeyCode, launcher: &F)
    where
        F: Fn(PathBuf) -> LaunchHandle,
    {
        self.confirm_cancel = self.confirm_cancel && code == KeyCode::Char('c');
        match code {
            | KeyCode::Char('1') => self.tab = Tab::Overview,
            | KeyCode::Char('2') => self.tab = Tab::Workflow,
            | KeyCode::Char('3') => self.tab = Tab::System,
            | KeyCode::Char('4') => self.tab = Tab::Logs,
            | KeyCode::Char('5') => self.tab = Tab::Runs,
            | KeyCode::Char('?') => self.help = !self.help,
            | KeyCode::Char('/') => self.message = "Search is scoped to the selected retained run".to_string(),
            | KeyCode::Char('c') if self.active.is_some() && self.confirm_cancel => {
                if let Some(active) = &self.active {
                    (active.cancel)();
                    self.message = "Cancellation requested; waiting for managed children".to_string();
                }
                self.confirm_cancel = false;
            }
            | KeyCode::Char('c') if self.active.is_some() => {
                self.confirm_cancel = true;
                self.message = "Press c again to cancel the active run".to_string();
            }
            | KeyCode::Char('f') => {
                self.follow_logs = !self.follow_logs;
                self.message = match self.follow_logs {
                    | true => "Following the newest log records".to_string(),
                    | false => "Showing the first log records".to_string(),
                };
            }
            | KeyCode::Char('n') if self.active.is_none() => match self.config.clone() {
                | Some(config) => {
                    self.active = Some(launcher(config));
                    self.selected = 0;
                    self.message = "Workflow started; following canonical evidence".to_string();
                }
                | None => self.message = "No configuration selected; restart with xylem tui CONFIG".to_string(),
            },
            | KeyCode::Char('q') if self.active.is_none() => self.quit = true,
            | KeyCode::Char('q') => self.message = "Cancel the active run before exiting".to_string(),
            | KeyCode::Down | KeyCode::Char('j') => {
                self.selected = self.selected.saturating_add(1).min(self.runs.len().saturating_sub(1));
            }
            | KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            | _ => {}
        }
    }
    fn refresh(&mut self) {
        if let Some(active) = &self.active {
            match active.completion.try_recv() {
                | Ok(Ok(())) => {
                    self.active = None;
                    self.message = "Workflow completed".to_string();
                }
                | Ok(Err(message)) => {
                    self.active = None;
                    self.message = message;
                }
                | Err(mpsc::TryRecvError::Disconnected) => {
                    self.active = None;
                    self.message = "Workflow runner disconnected".to_string();
                }
                | Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        match self.store.list() {
            | Ok(runs) => {
                self.runs = runs;
                self.selected = self.selected.min(self.runs.len().saturating_sub(1));
            }
            | Err(why) => self.message = why.to_string(),
        }
    }
}
impl LaunchHandle {
    /// Construct a launch handle from completion and cancellation callbacks.
    pub fn new(completion: mpsc::Receiver<Result<(), String>>, cancel: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            cancel: Arc::new(cancel),
            completion,
        }
    }
}
impl Tab {
    fn index(self) -> usize {
        match self {
            | Self::Overview => 0,
            | Self::Workflow => 1,
            | Self::System => 2,
            | Self::Logs => 3,
            | Self::Runs => 4,
        }
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn accent_color() -> Color {
    match env::var_os("NO_COLOR") {
        | Some(_) => Color::Reset,
        | None => Color::Cyan,
    }
}
fn collect_files(root: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = read_dir(root) {
        entries.flatten().for_each(|entry| match entry.path().is_dir() {
            | true => collect_files(&entry.path(), files),
            | false => files.push(entry.path()),
        });
    }
}
fn log_lines(run: Option<&StoredRun>, follow: bool) -> Vec<Line<'static>> {
    run.map_or_else(
        || vec![Line::from("No retained run selected")],
        |run| {
            let mut files = Vec::new();
            collect_files(&run.path.join("logs"), &mut files);
            files.sort();
            let lines = files
                .into_iter()
                .flat_map(|path| {
                    read_to_string(&path)
                        .unwrap_or_default()
                        .lines()
                        .filter_map(|line| serde_json::from_str::<TaskLogRecord>(line).ok())
                        .map(move |record| {
                            Line::from(format!(
                                "{} {} {}",
                                path.strip_prefix(&run.path).unwrap_or(&path).display(),
                                record.stream,
                                record.text
                            ))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .into_iter();
            match follow {
                | true => lines.rev().take(20).collect::<Vec<_>>().into_iter().rev().collect(),
                | false => lines.take(20).collect(),
            }
        },
    )
}
fn metric_lines(run: Option<&StoredRun>) -> Vec<Line<'static>> {
    run.and_then(|run| read_to_string(run.path.join("metrics.jsonl")).ok()).map_or_else(
        || vec![Line::from("No metrics recorded")],
        |content| {
            content
                .lines()
                .filter_map(|line| serde_json::from_str::<MetricSample>(line).ok())
                .map(|sample| {
                    let storage = sample
                        .host_filesystem_total
                        .as_ref()
                        .and_then(|memory| memory.checked_bytes())
                        .unwrap_or_default();
                    let available = sample
                        .host_filesystem_available
                        .as_ref()
                        .and_then(|memory| memory.checked_bytes())
                        .unwrap_or_default();
                    Line::from(format!(
                        "cpu {:>5.1}% · load {:>4.2} · memory {:>6} MiB · xylem {:>6} MiB/{} proc · net ↓{}/↑{} KiB · disk ↓{}/↑{} KiB · fs {}/{} GiB",
                        sample.host_cpu_percent.unwrap_or_default(),
                        sample.host_load_one.unwrap_or_default(),
                        sample.host_memory_used.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1_048_576,
                        sample.process_memory.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1_048_576,
                        sample.process_count.unwrap_or_default(),
                        sample.host_network_received.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1024,
                        sample.host_network_transmitted.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1024,
                        sample.host_disk_read.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1024,
                        sample.host_disk_written.as_ref().and_then(|memory| memory.checked_bytes()).unwrap_or_default() / 1024,
                        storage.saturating_sub(available) / 1_073_741_824,
                        storage / 1_073_741_824,
                    ))
                })
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        },
    )
}

fn projection_text(dashboard: &Dashboard) -> String {
    dashboard.active_run().map_or_else(
        || "unavailable".to_string(),
        |selected| {
            let current = RunJournal::read(selected.path.join("journal.jsonl"));
            current.map_or_else(
                |_| "unavailable".to_string(),
                |current| {
                    let histories = dashboard
                        .runs
                        .iter()
                        .filter(|run| run.configuration_path == selected.configuration_path && run.run_id != selected.run_id)
                        .filter_map(|run| RunJournal::read(run.path.join("journal.jsonl")).ok())
                        .take(50)
                        .collect::<Vec<_>>();
                    let history = DurationHistory::from_summaries(histories.iter());
                    let threads = read_to_string(selected.path.join("manifest.json"))
                        .ok()
                        .and_then(|content| serde_json::from_str::<Manifest>(&content).ok())
                        .map_or(1, |manifest| manifest.threads.max(1));
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
                    history.project(&current, now, threads).map_or_else(
                        || "complete or collecting samples".to_string(),
                        |projection| {
                            format!(
                                "{}s · {:?} · {} of {} steps",
                                projection.eta.get() / 1000,
                                projection.confidence,
                                projection.completed_steps,
                                projection.total_steps
                            )
                        },
                    )
                },
            )
        },
    )
}
fn render(frame: &mut Frame<'_>, dashboard: &Dashboard) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)])
        .split(frame.area());
    let titles = ["1 Overview", "2 Workflow", "3 System", "4 Logs", "5 Runs"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(
        Tabs::new(titles)
            .select(dashboard.tab.index())
            .block(Block::default().borders(Borders::ALL).title("Xylem"))
            .highlight_style(Style::default().fg(accent_color()).add_modifier(Modifier::BOLD)),
        vertical[0],
    );
    let content = vertical[1];
    match dashboard.tab {
        | Tab::Overview => render_overview(frame, content, dashboard),
        | Tab::Workflow => render_workflow(frame, content, dashboard.active_run()),
        | Tab::System => render_lines(frame, content, "System · 1 second samples", metric_lines(dashboard.active_run())),
        | Tab::Logs => render_lines(
            frame,
            content,
            match dashboard.follow_logs {
                | true => "Logs · following newest",
                | false => "Logs · first records",
            },
            log_lines(dashboard.active_run(), dashboard.follow_logs),
        ),
        | Tab::Runs => render_runs(frame, content, dashboard),
    }
    let status = match dashboard.help {
        | true => "1-5 tabs · j/k select · n launch · c,c cancel · f follow · / search · q quit",
        | false => dashboard.message.as_str(),
    };
    frame.render_widget(Paragraph::new(status).style(Style::default().fg(Color::DarkGray)), vertical[2]);
}
fn render_lines(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'static>>) {
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}
fn render_overview(frame: &mut Frame<'_>, area: Rect, dashboard: &Dashboard) {
    let run = dashboard.active_run();
    let status = run.map_or_else(|| "No retained runs".to_string(), |run| format!("{:?}", run.status).to_lowercase());
    let config = dashboard
        .config
        .as_ref()
        .map_or_else(|| "history only".to_string(), |path| path.display().to_string());
    let eta = projection_text(dashboard);
    let lines = vec![
        Line::from(vec![
            Span::styled("Status  ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(status),
        ]),
        Line::from(format!("Config  {config}")),
        Line::from(format!("Runs    {} retained", dashboard.runs.len())),
        Line::from(if dashboard.active.is_some() {
            "Live    workflow execution active"
        } else {
            "Live    idle"
        }),
        Line::from(format!("ETA     {eta}")),
    ];
    render_lines(frame, area, "Overview", lines);
}
fn render_runs(frame: &mut Frame<'_>, area: Rect, dashboard: &Dashboard) {
    let items = dashboard
        .runs
        .iter()
        .enumerate()
        .map(|(index, run)| {
            let marker = if index == dashboard.selected { ">" } else { " " };
            ListItem::new(format!("{marker} {} · {:?} · {}", run.run_id, run.status, run.configuration_path))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Runs · newest first")),
        area,
    );
}
fn render_workflow(frame: &mut Frame<'_>, area: Rect, run: Option<&StoredRun>) {
    let lines = run.map_or_else(
        || vec![Line::from("No retained run selected")],
        |run| match RunJournal::read(run.path.join("journal.jsonl")) {
            | Ok(summary) => summary
                .inputs
                .into_iter()
                .flat_map(|input| {
                    let heading = Line::from(format!("{} · {:?}", input.value.unwrap_or(input.input_id), input.state));
                    core::iter::once(heading)
                        .chain(
                            input
                                .modules
                                .into_iter()
                                .map(|module| Line::from(format!("  {:>3} {:?} {}", module.index, module.state, module.module_path.join("::")))),
                        )
                        .collect::<Vec<_>>()
                })
                .collect(),
            | Err(why) => vec![Line::from(why.to_string())],
        },
    );
    render_lines(frame, area, "Workflow", lines);
}
/// Open the dashboard and delegate workflow launches to the caller.
pub fn run<F>(config: Option<PathBuf>, launcher: F) -> Result<(), DashboardError>
where
    F: Fn(PathBuf) -> LaunchHandle,
{
    let store = Store::system()?;
    let mut dashboard = Dashboard::new(config, store)?;
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let _guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    while !dashboard.quit {
        dashboard.refresh();
        terminal.draw(|frame| render(frame, &dashboard))?;
        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    dashboard.handle_key(key.code, &launcher);
                }
            }
        }
    }
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use ratatui::backend::TestBackend;

    #[test]
    fn test_cancellation_requires_confirmation() {
        let root = std::env::temp_dir().join(format!("xylem-tui-cancel-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store::new(root.clone()).expect("test store should initialize");
        let mut dashboard = Dashboard::new(Some(PathBuf::from("workflow.json")), store).expect("dashboard should initialize");
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
    fn test_every_tab_renders_at_compact_size() {
        let root = std::env::temp_dir().join(format!("xylem-tui-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store::new(root.clone()).expect("test store should initialize");
        let mut dashboard = Dashboard::new(None, store).expect("dashboard should initialize");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        for tab in [Tab::Overview, Tab::Workflow, Tab::System, Tab::Logs, Tab::Runs] {
            dashboard.tab = tab;
            terminal.draw(|frame| render(frame, &dashboard)).expect("tab should render");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
