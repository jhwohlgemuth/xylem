//! Local terminal dashboard for Xylem workflow runs
extern crate alloc;

use alloc::sync::Arc;
use core::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Sparkline, Table, TableState, Tabs, Wrap};
use ratatui::{Frame, Terminal};
use std::env;
use std::fs::{read_dir, read_to_string};
use std::io::{self, stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use xylem_lib::journal::{ExecutionState, RunJournal, RunSummary};
use xylem_lib::projection::DurationHistory;
use xylem_lib::run::store::{BundleStatus, Manifest, MetricSample, Store, StoreError, StoredRun, TaskLogRecord};

const DIAGNOSTIC_LOG_LIMIT: usize = 1024 * 1024;
const LOG_LINE_LIMIT: usize = 500;
const TICK: Duration = Duration::from_millis(200);
trait ExecutionStateSymbol {
    fn symbol(self) -> &'static str;
}
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
#[derive(Clone, Copy, Default)]
enum EscapeState {
    Csi,
    Escape,
    Osc,
    OscEscape,
    #[default]
    Plain,
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
struct BundleStatusStyle(Style);
struct Dashboard {
    active: Option<LaunchHandle>,
    config: Option<PathBuf>,
    confirm_cancel: bool,
    diagnostics: DiagnosticLog,
    follow_logs: bool,
    help: bool,
    message: String,
    quit: bool,
    runs: Vec<StoredRun>,
    scroll: u16,
    selected: usize,
    store: Store,
    tab: Tab,
}
/// Bounded, cloneable destination for diagnostic events rendered by the dashboard.
#[derive(Clone, Default)]
pub struct DiagnosticLog {
    state: Arc<Mutex<DiagnosticLogState>>,
}
#[derive(Default)]
struct DiagnosticLogState {
    bytes: Vec<u8>,
    escape: EscapeState,
}
struct ExecutionStateStyle(Style);
/// Handle returned by a caller-owned workflow launcher.
pub struct LaunchHandle {
    cancel: Arc<dyn Fn() + Send + Sync>,
    completion: mpsc::Receiver<Result<(), String>>,
}
struct TerminalGuard;
#[derive(Clone, Copy, Default)]
struct WorkflowCounts {
    cancelled: u64,
    failed: u64,
    running: u64,
    skipped: u64,
    succeeded: u64,
    waiting: u64,
}
impl From<BundleStatus> for BundleStatusStyle {
    fn from(status: BundleStatus) -> Self {
        let color = match status {
            | BundleStatus::Succeeded => Color::Green,
            | BundleStatus::Running => Color::Cyan,
            | BundleStatus::Cancelled | BundleStatus::Interrupted => Color::Yellow,
            | BundleStatus::Failed => Color::Red,
            | BundleStatus::Corrupt | BundleStatus::Unsupported => Color::Magenta,
        };
        Self(Style::default().fg(theme_color(color)).add_modifier(Modifier::BOLD))
    }
}
impl Dashboard {
    fn new(config: Option<PathBuf>, store: Store, diagnostics: DiagnosticLog) -> Result<Self, StoreError> {
        store.list().map(|runs| Self {
            active: None,
            config,
            confirm_cancel: false,
            diagnostics,
            follow_logs: true,
            help: false,
            message: "n new run · ? help · q quit".to_string(),
            quit: false,
            runs,
            scroll: 0,
            selected: 0,
            store,
            tab: Tab::Overview,
        })
    }
    fn handle_key<F>(&mut self, code: KeyCode, launcher: &F)
    where
        F: Fn(PathBuf) -> LaunchHandle,
    {
        self.confirm_cancel = self.confirm_cancel && code == KeyCode::Char('c');
        match code {
            | KeyCode::Tab => self.select_tab(self.tab.next()),
            | KeyCode::Char('1') => self.select_tab(Tab::Overview),
            | KeyCode::Char('2') => self.select_tab(Tab::Workflow),
            | KeyCode::Char('3') => self.select_tab(Tab::System),
            | KeyCode::Char('4') => self.select_tab(Tab::Logs),
            | KeyCode::Char('5') => self.select_tab(Tab::Runs),
            | KeyCode::Char('?') => self.help = !self.help,
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
                self.scroll = 0;
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
            | KeyCode::Down | KeyCode::Char('j') if self.tab == Tab::Runs => {
                self.selected = self.selected.saturating_add(1).min(self.runs.len().saturating_sub(1));
            }
            | KeyCode::Up | KeyCode::Char('k') if self.tab == Tab::Runs => self.selected = self.selected.saturating_sub(1),
            | KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            | KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            | KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            | KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            | KeyCode::Home => self.scroll = 0,
            | _ => {}
        }
    }
    fn refresh(&mut self) {
        let selected_run_id = self.runs.get(self.selected).map(|run| run.run_id.clone());
        match self.store.list() {
            | Ok(runs) => {
                self.runs = runs;
                self.selected = match self.active.is_some() {
                    | true => 0,
                    | false => selected_run_id
                        .and_then(|run_id| self.runs.iter().position(|run| run.run_id == run_id))
                        .unwrap_or_else(|| self.selected.min(self.runs.len().saturating_sub(1))),
                };
            }
            | Err(why) => self.message = why.to_string(),
        }
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
    }
    fn select_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.scroll = 0;
    }
}
impl DiagnosticLog {
    fn lines(&self) -> Vec<String> {
        self.state.lock().map_or_else(
            |_| vec!["Diagnostic log is unavailable".to_string()],
            |state| String::from_utf8_lossy(&state.bytes).lines().map(String::from).collect(),
        )
    }
}
impl Write for DiagnosticLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = bytes.len();
        self.state
            .lock()
            .map_err(|_| io::Error::other("diagnostic log lock is poisoned"))
            .map(|mut state| {
                bytes.iter().for_each(|byte| match state.escape {
                    | EscapeState::Plain => match *byte {
                        | 0x1b => state.escape = EscapeState::Escape,
                        | b'\n' | b'\t' | 0x20..=0xff => state.bytes.push(*byte),
                        | _ => {}
                    },
                    | EscapeState::Escape => {
                        state.escape = match *byte {
                            | b'[' => EscapeState::Csi,
                            | b']' => EscapeState::Osc,
                            | _ => EscapeState::Plain,
                        };
                    }
                    | EscapeState::Csi => {
                        if (0x40..=0x7e).contains(byte) {
                            state.escape = EscapeState::Plain;
                        }
                    }
                    | EscapeState::Osc => {
                        state.escape = match *byte {
                            | 0x07 => EscapeState::Plain,
                            | 0x1b => EscapeState::OscEscape,
                            | _ => EscapeState::Osc,
                        };
                    }
                    | EscapeState::OscEscape => {
                        state.escape = match *byte {
                            | b'\\' => EscapeState::Plain,
                            | 0x1b => EscapeState::OscEscape,
                            | _ => EscapeState::Osc,
                        };
                    }
                });
                let overflow = state.bytes.len().saturating_sub(DIAGNOSTIC_LOG_LIMIT);
                if overflow > 0 {
                    let boundary = state.bytes[overflow..]
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map_or(overflow, |index| overflow.saturating_add(index).saturating_add(1));
                    state.bytes.drain(..boundary);
                }
                written
            })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl ExecutionStateSymbol for ExecutionState {
    fn symbol(self) -> &'static str {
        match self {
            | ExecutionState::Succeeded => "✓",
            | ExecutionState::Failed => "✕",
            | ExecutionState::Cancelled => "■",
            | ExecutionState::Running => "◆",
            | ExecutionState::Pending => "◇",
            | ExecutionState::Skipped | ExecutionState::SkippedExisting => "↷",
            | ExecutionState::NotStarted => "○",
        }
    }
}
impl From<ExecutionState> for ExecutionStateStyle {
    fn from(state: ExecutionState) -> Self {
        let (color, modifier) = match state {
            | ExecutionState::Succeeded => (Color::Green, Modifier::BOLD),
            | ExecutionState::Failed => (Color::Red, Modifier::BOLD),
            | ExecutionState::Cancelled => (Color::Yellow, Modifier::BOLD),
            | ExecutionState::Running => (Color::Cyan, Modifier::BOLD),
            | ExecutionState::Pending => (Color::Blue, Modifier::ITALIC),
            | ExecutionState::Skipped | ExecutionState::SkippedExisting => (Color::Yellow, Modifier::ITALIC),
            | ExecutionState::NotStarted => (Color::DarkGray, Modifier::ITALIC),
        };
        Self(Style::default().fg(theme_color(color)).add_modifier(modifier))
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
    fn next(self) -> Self {
        match self {
            | Self::Overview => Self::Workflow,
            | Self::Workflow => Self::System,
            | Self::System => Self::Logs,
            | Self::Logs => Self::Runs,
            | Self::Runs => Self::Overview,
        }
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}
impl WorkflowCounts {
    fn maximum(self) -> u64 {
        [self.cancelled, self.failed, self.running, self.skipped, self.succeeded, self.waiting]
            .into_iter()
            .max()
            .unwrap_or_default()
            .max(1)
    }
    fn terminal(self) -> u64 {
        self.succeeded
            .saturating_add(self.failed)
            .saturating_add(self.cancelled)
            .saturating_add(self.skipped)
    }
}
impl From<&RunSummary> for WorkflowCounts {
    fn from(summary: &RunSummary) -> Self {
        summary
            .inputs
            .iter()
            .flat_map(|input| &input.modules)
            .fold(Self::default(), |counts, module| match module.state {
                | ExecutionState::Cancelled => Self {
                    cancelled: counts.cancelled.saturating_add(1),
                    ..counts
                },
                | ExecutionState::Failed => Self {
                    failed: counts.failed.saturating_add(1),
                    ..counts
                },
                | ExecutionState::Pending | ExecutionState::Running => Self {
                    running: counts.running.saturating_add(1),
                    ..counts
                },
                | ExecutionState::Skipped | ExecutionState::SkippedExisting => Self {
                    skipped: counts.skipped.saturating_add(1),
                    ..counts
                },
                | ExecutionState::Succeeded => Self {
                    succeeded: counts.succeeded.saturating_add(1),
                    ..counts
                },
                | ExecutionState::NotStarted => Self {
                    waiting: counts.waiting.saturating_add(1),
                    ..counts
                },
            })
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
fn diagnostic_line(value: String) -> Line<'static> {
    let (level, message) = value.split_once(' ').unwrap_or(("", value.as_str()));
    let level_color = match level {
        | "ERROR" => Color::Red,
        | "WARN" => Color::Yellow,
        | "INFO" => Color::Green,
        | "DEBUG" => Color::Blue,
        | "TRACE" => Color::Magenta,
        | _ => Color::DarkGray,
    };
    match level.is_empty() {
        | true => Line::from(Span::styled(
            value,
            Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
        )),
        | false => Line::from(vec![
            Span::styled(
                format!("{level:<5}"),
                Style::default().fg(theme_color(level_color)).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(message.to_string()),
        ]),
    }
}
fn log_lines(run: Option<&StoredRun>, diagnostics: &DiagnosticLog, follow: bool) -> Vec<Line<'static>> {
    let task_lines = run.map_or_else(Vec::new, |run| {
        let mut files = Vec::new();
        collect_files(&run.path.join("logs"), &mut files);
        files.sort();
        files
            .into_iter()
            .flat_map(|path| {
                read_to_string(&path)
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|line| serde_json::from_str::<TaskLogRecord>(line).ok())
                    .map(move |record| {
                        let stream_color = match record.stream.as_str() {
                            | "stderr" => Color::Red,
                            | _ => Color::Green,
                        };
                        Line::from(vec![
                            Span::styled(
                                format!("{} ", path.strip_prefix(&run.path).unwrap_or(&path).display()),
                                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                            ),
                            Span::styled(
                                format!("{:<6} ", record.stream),
                                Style::default().fg(theme_color(stream_color)).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(record.text),
                        ])
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    });
    let lines = task_lines
        .into_iter()
        .chain(diagnostics.lines().into_iter().map(diagnostic_line))
        .collect::<Vec<_>>();
    match (lines.is_empty(), follow) {
        | (true, _) => vec![Line::from(Span::styled(
            "No logs recorded",
            Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
        ))],
        | (false, true) => lines
            .into_iter()
            .rev()
            .take(LOG_LINE_LIMIT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
        | (false, false) => lines.into_iter().take(LOG_LINE_LIMIT).collect(),
    }
}
fn metric_samples(run: Option<&StoredRun>) -> Vec<MetricSample> {
    run.and_then(|run| read_to_string(run.path.join("metrics.jsonl")).ok())
        .map_or_else(Vec::new, |content| {
            content
                .lines()
                .filter_map(|line| serde_json::from_str::<MetricSample>(line).ok())
                .collect()
        })
}
fn panel(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme_color(Color::DarkGray)))
        .title(Span::styled(
            format!(" {} ", title.into().to_uppercase()),
            Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD),
        ))
}
fn projection_text(dashboard: &Dashboard) -> String {
    dashboard.runs.get(dashboard.selected).map_or_else(
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
                                u64::from(projection.eta) / 1000,
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
    let titles = [("1", "Overview"), ("2", "Workflow"), ("3", "System"), ("4", "Logs"), ("5", "Runs")]
        .into_iter()
        .map(|(key, title)| {
            Line::from(vec![
                Span::styled(key, Style::default().fg(theme_color(Color::Magenta)).add_modifier(Modifier::BOLD)),
                Span::raw(format!(" {title}")),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Tabs::new(titles)
            .select(dashboard.tab.index())
            .block(panel("Xylem · control plane"))
            .divider(Span::styled(
                " │ ",
                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
            ))
            .style(Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC))
            .highlight_style(
                Style::default()
                    .fg(theme_color(Color::Cyan))
                    .bg(theme_color(Color::DarkGray))
                    .add_modifier(Modifier::BOLD),
            ),
        vertical[0],
    );
    let content = vertical[1];
    match dashboard.tab {
        | Tab::Overview => render_overview(frame, content, dashboard),
        | Tab::Workflow => render_workflow(frame, content, dashboard.runs.get(dashboard.selected), dashboard.scroll),
        | Tab::System => render_system(frame, content, dashboard.runs.get(dashboard.selected)),
        | Tab::Logs => render_log_lines(
            frame,
            content,
            match dashboard.follow_logs {
                | true => "Logs · following newest",
                | false => "Logs · first records",
            },
            log_lines(dashboard.runs.get(dashboard.selected), &dashboard.diagnostics, dashboard.follow_logs),
            dashboard.scroll,
            dashboard.follow_logs,
        ),
        | Tab::Runs => render_runs(frame, content, dashboard),
    }
    let status = match dashboard.help {
        | true => Line::from(vec![
            Span::styled("Tab/1-5", Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD)),
            Span::raw(" switch · "),
            Span::styled("j/k", Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD)),
            Span::raw(" scroll · n launch · c,c cancel · f follow · q quit"),
        ]),
        | false => Line::from(vec![
            Span::styled("◆ ", Style::default().fg(theme_color(Color::Cyan))),
            Span::styled(
                dashboard.message.clone(),
                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
            ),
        ]),
    };
    frame.render_widget(Paragraph::new(status), vertical[2]);
}
fn render_lines(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'static>>, scroll: u16) {
    frame.render_widget(
        Paragraph::new(lines).block(panel(title)).scroll((scroll, 0)).wrap(Wrap { trim: false }),
        area,
    );
}
fn render_log_lines(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'static>>, scroll: u16, follow: bool) {
    let content_width = usize::from(area.width.saturating_sub(2).max(1));
    let rendered_lines = lines.iter().map(|line| line.width().max(1).div_ceil(content_width)).sum::<usize>();
    let paragraph = Paragraph::new(lines).block(panel(title)).wrap(Wrap { trim: false });
    let visible_height = usize::from(area.height.saturating_sub(2));
    let offset = match follow {
        | true => u16::try_from(rendered_lines.saturating_sub(visible_height)).unwrap_or(u16::MAX),
        | false => scroll,
    };
    frame.render_widget(paragraph.scroll((offset, 0)), area);
}
fn render_metric_chart(frame: &mut Frame<'_>, area: Rect, title: String, values: &[u64], maximum: u64, color: Color) {
    let observed = values.iter().copied().max().unwrap_or_default();
    frame.render_widget(
        Sparkline::default()
            .block(panel(title))
            .data(values.iter().copied())
            .max(maximum.max(observed).max(1))
            .style(Style::default().fg(theme_color(color)).add_modifier(Modifier::BOLD)),
        area,
    );
}
fn render_overview(frame: &mut Frame<'_>, area: Rect, dashboard: &Dashboard) {
    let run = dashboard.runs.get(dashboard.selected);
    let summary = run.and_then(|run| RunJournal::read(run.path.join("journal.jsonl")).ok());
    let counts = summary.as_ref().map_or_else(WorkflowCounts::default, WorkflowCounts::from);
    let total = counts.terminal().saturating_add(counts.running).saturating_add(counts.waiting);
    let ratio = match total {
        | 0 => 0.0,
        | total => counts.terminal() as f64 / total as f64,
    };
    let eta = projection_text(dashboard);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(3), Constraint::Min(3)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(27), Constraint::Percentage(38), Constraint::Percentage(35)])
        .split(rows[0]);
    let status = run.map_or_else(
        || {
            vec![
                Line::from(Span::styled(
                    "NO RUN",
                    Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                )),
                Line::from(Span::styled(
                    "waiting for history",
                    Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                )),
            ]
        },
        |run| {
            vec![
                Line::from(Span::styled(run.status.to_string().to_uppercase(), BundleStatusStyle::from(run.status).0)),
                Line::from(Span::styled(
                    run.run_id.clone(),
                    Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::ITALIC),
                )),
            ]
        },
    );
    frame.render_widget(Paragraph::new(status).alignment(Alignment::Center).block(panel("Run")), top[0]);
    let (succeeded, failed, unfinished, skipped) = summary.as_ref().map_or((0u64, 0u64, 0u64, 0u64), |summary| {
        summary.inputs.iter().fold((0u64, 0u64, 0u64, 0u64), |counts, input| match input.state {
            | ExecutionState::Succeeded => (counts.0.saturating_add(1), counts.1, counts.2, counts.3),
            | ExecutionState::Failed => (counts.0, counts.1.saturating_add(1), counts.2, counts.3),
            | ExecutionState::NotStarted | ExecutionState::Pending | ExecutionState::Running => {
                (counts.0, counts.1, counts.2.saturating_add(1), counts.3)
            }
            | ExecutionState::Cancelled | ExecutionState::Skipped | ExecutionState::SkippedExisting => {
                (counts.0, counts.1, counts.2, counts.3.saturating_add(1))
            }
        })
    });
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format!("✓ {succeeded} succeeded"),
                    Style::default().fg(theme_color(Color::Green)).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    format!("✕ {failed} failed"),
                    Style::default().fg(theme_color(Color::Red)).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("◇ {unfinished} unfinished"),
                    Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    format!("↷ {skipped} skipped"),
                    Style::default().fg(theme_color(Color::Yellow)).add_modifier(Modifier::ITALIC),
                ),
            ]),
        ])
        .block(panel("Inputs")),
        top[1],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(theme_color(if dashboard.active.is_some() { Color::Green } else { Color::DarkGray })),
                ),
                Span::styled(
                    if dashboard.active.is_some() { "LIVE" } else { "IDLE" },
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {} runs", dashboard.runs.len()),
                    Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                ),
            ]),
            Line::from(vec![
                Span::styled("ETA  ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(eta, Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC)),
            ]),
        ])
        .block(panel("Session")),
        top[2],
    );
    frame.render_widget(
        Gauge::default()
            .block(panel("Progress"))
            .ratio(ratio)
            .label(Span::styled(
                format!("{} / {total} steps", counts.terminal()),
                Style::default().add_modifier(Modifier::BOLD),
            ))
            .gauge_style(Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD))
            .use_unicode(true),
        rows[1],
    );
    let mut activity = run
        .map(|run| {
            vec![Line::from(Span::styled(
                run.configuration_path.clone(),
                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
            ))]
        })
        .unwrap_or_default();
    if let Some(error) = summary.as_ref().and_then(|summary| summary.error.as_ref()) {
        activity.extend(error.lines().map(|line| {
            Line::from(vec![
                Span::styled("ERROR ", Style::default().fg(theme_color(Color::Red)).add_modifier(Modifier::BOLD)),
                Span::styled(line.to_string(), Style::default().fg(theme_color(Color::Red))),
            ])
        }));
    }
    let recent = dashboard
        .diagnostics
        .lines()
        .into_iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    activity.extend(recent.into_iter().map(diagnostic_line));
    if activity.is_empty() {
        activity.push(Line::from(Span::styled(
            "No activity recorded",
            Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
        )));
    }
    frame.render_widget(
        Paragraph::new(activity)
            .block(panel("Activity"))
            .scroll((dashboard.scroll, 0))
            .wrap(Wrap { trim: false }),
        rows[2],
    );
}
fn render_runs(frame: &mut Frame<'_>, area: Rect, dashboard: &Dashboard) {
    let rows = dashboard
        .runs
        .iter()
        .map(|run| {
            Row::new([
                Cell::from(run.run_id.clone()).style(Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD)),
                Cell::from(run.status.to_string()).style(BundleStatusStyle::from(run.status).0),
                Cell::from(run.configuration_path.clone()).style(Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC)),
            ])
        })
        .collect::<Vec<_>>();
    let header = Row::new(["Run", "Status", "Configuration"])
        .style(Style::default().fg(theme_color(Color::Magenta)).add_modifier(Modifier::BOLD))
        .bottom_margin(1);
    let mut state = TableState::default().with_selected((!dashboard.runs.is_empty()).then_some(dashboard.selected));
    frame.render_stateful_widget(
        Table::new(rows, [Constraint::Length(12), Constraint::Length(13), Constraint::Min(20)])
            .header(header)
            .block(panel("Runs · newest first"))
            .column_spacing(2)
            .row_highlight_style(
                Style::default()
                    .fg(theme_color(Color::Cyan))
                    .bg(theme_color(Color::DarkGray))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ "),
        area,
        &mut state,
    );
}
fn render_system(frame: &mut Frame<'_>, area: Rect, run: Option<&StoredRun>) {
    let samples = metric_samples(run);
    match samples.last() {
        | None => frame.render_widget(
            Paragraph::new(Span::styled(
                "No metrics recorded",
                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
            ))
            .alignment(Alignment::Center)
            .block(panel("System · 1 second samples")),
            area,
        ),
        | Some(latest) => {
            let host_cpu = samples
                .iter()
                .map(|sample| sample.host_cpu_percent.unwrap_or_default().max(0.0).round() as u64)
                .collect::<Vec<_>>();
            let process_memory = samples
                .iter()
                .map(|sample| {
                    sample
                        .process_memory
                        .as_ref()
                        .and_then(|memory| memory.checked_bytes())
                        .unwrap_or_default()
                        / 1024
                        / 1024
                })
                .collect::<Vec<_>>();
            let network = samples
                .iter()
                .map(|sample| {
                    (sample
                        .host_network_received
                        .as_ref()
                        .and_then(|memory| memory.checked_bytes())
                        .unwrap_or_default()
                        / 1024)
                        .saturating_add(
                            sample
                                .host_network_transmitted
                                .as_ref()
                                .and_then(|memory| memory.checked_bytes())
                                .unwrap_or_default()
                                / 1024,
                        )
                })
                .collect::<Vec<_>>();
            let disk = samples
                .iter()
                .map(|sample| {
                    (sample
                        .host_disk_read
                        .as_ref()
                        .and_then(|memory| memory.checked_bytes())
                        .unwrap_or_default()
                        / 1024)
                        .saturating_add(
                            sample
                                .host_disk_written
                                .as_ref()
                                .and_then(|memory| memory.checked_bytes())
                                .unwrap_or_default()
                                / 1024,
                        )
                })
                .collect::<Vec<_>>();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[2]);
            let host_memory = latest
                .host_memory_used
                .as_ref()
                .and_then(|memory| memory.checked_bytes())
                .unwrap_or_default()
                / 1024
                / 1024;
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("LOAD ", Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("{:.2}", latest.host_load_one.unwrap_or_default()),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "   PROCESSES ",
                        Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        latest.process_count.unwrap_or_default().to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "   HOST MEMORY ",
                        Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{host_memory} MiB used"),
                        Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                    ),
                ]))
                .block(panel("Latest sample")),
                rows[0],
            );
            render_metric_chart(
                frame,
                top[0],
                format!("Host CPU · {:.1}%", latest.host_cpu_percent.unwrap_or_default()),
                &host_cpu,
                100,
                Color::Cyan,
            );
            render_metric_chart(
                frame,
                top[1],
                format!("Xylem RSS · {} MiB", process_memory.last().copied().unwrap_or_default()),
                &process_memory,
                1,
                Color::Magenta,
            );
            render_metric_chart(
                frame,
                bottom[0],
                format!("Network I/O · {} KiB/s", network.last().copied().unwrap_or_default()),
                &network,
                1,
                Color::Green,
            );
            render_metric_chart(
                frame,
                bottom[1],
                format!("Disk I/O · {} KiB/s", disk.last().copied().unwrap_or_default()),
                &disk,
                1,
                Color::Yellow,
            );
        }
    }
}
fn render_workflow(frame: &mut Frame<'_>, area: Rect, run: Option<&StoredRun>, scroll: u16) {
    let summary = run.map(|run| RunJournal::read(run.path.join("journal.jsonl")));
    match summary {
        | None => render_lines(
            frame,
            area,
            "Workflow",
            vec![Line::from(Span::styled(
                "No retained run selected",
                Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
            ))],
            scroll,
        ),
        | Some(Err(why)) => render_lines(
            frame,
            area,
            "Workflow",
            vec![Line::from(Span::styled(why.to_string(), Style::default().fg(theme_color(Color::Red))))],
            scroll,
        ),
        | Some(Ok(summary)) => {
            let counts = WorkflowCounts::from(&summary);
            let total = counts.terminal().saturating_add(counts.running).saturating_add(counts.waiting);
            let ratio = match total {
                | 0 => 0.0,
                | total => counts.terminal() as f64 / total as f64,
            };
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(7), Constraint::Min(3)])
                .split(area);
            frame.render_widget(
                Gauge::default()
                    .block(panel("Workflow completion"))
                    .ratio(ratio)
                    .label(Span::styled(
                        format!("{} / {total} steps terminal", counts.terminal()),
                        Style::default().add_modifier(Modifier::BOLD),
                    ))
                    .gauge_style(Style::default().fg(theme_color(Color::Cyan)).add_modifier(Modifier::BOLD))
                    .use_unicode(true),
                areas[0],
            );
            let bars = vec![
                Bar::with_label("done", counts.succeeded)
                    .style(Style::default().fg(theme_color(Color::Green)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
                Bar::with_label("live", counts.running)
                    .style(Style::default().fg(theme_color(Color::Cyan)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
                Bar::with_label("fail", counts.failed)
                    .style(Style::default().fg(theme_color(Color::Red)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
                Bar::with_label("stop", counts.cancelled)
                    .style(Style::default().fg(theme_color(Color::Yellow)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
                Bar::with_label("skip", counts.skipped)
                    .style(Style::default().fg(theme_color(Color::Blue)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
                Bar::with_label("wait", counts.waiting)
                    .style(Style::default().fg(theme_color(Color::DarkGray)))
                    .value_style(Style::default().add_modifier(Modifier::BOLD)),
            ];
            frame.render_widget(
                BarChart::vertical(bars)
                    .block(panel("Step states"))
                    .bar_width(6)
                    .bar_gap(2)
                    .max(counts.maximum())
                    .label_style(Style::default().add_modifier(Modifier::ITALIC)),
                areas[1],
            );
            let rows = summary
                .inputs
                .into_iter()
                .flat_map(|input| {
                    let input_name = input.value.unwrap_or(input.input_id);
                    let input_state = input.state;
                    let input_row = Row::new([
                        Cell::from(input_name).style(Style::default().add_modifier(Modifier::BOLD)),
                        Cell::from(Span::styled(
                            "<input>",
                            Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                        )),
                        Cell::from(Line::from(vec![
                            Span::styled(format!("{} ", input_state.symbol()), ExecutionStateStyle::from(input_state).0),
                            Span::styled(input_state.to_string(), ExecutionStateStyle::from(input_state).0),
                        ])),
                        Cell::from(input.error.unwrap_or_default()).style(Style::default().fg(theme_color(Color::Red))),
                    ]);
                    let module_count = input.modules.len();
                    core::iter::once(input_row)
                        .chain(input.modules.into_iter().enumerate().map(move |(position, module)| {
                            let connector = if position.saturating_add(1) == module_count {
                                "└─"
                            } else {
                                "├─"
                            };
                            Row::new([
                                Cell::from(""),
                                Cell::from(Line::from(vec![
                                    Span::styled(
                                        format!("{connector} "),
                                        Style::default().fg(theme_color(Color::DarkGray)).add_modifier(Modifier::ITALIC),
                                    ),
                                    Span::styled(module.module_path.join("::"), Style::default().add_modifier(Modifier::BOLD)),
                                ])),
                                Cell::from(Line::from(vec![
                                    Span::styled(format!("{} ", module.state.symbol()), ExecutionStateStyle::from(module.state).0),
                                    Span::styled(module.state.to_string(), ExecutionStateStyle::from(module.state).0),
                                ])),
                                Cell::from(module.error.unwrap_or_default()).style(Style::default().fg(theme_color(Color::Red))),
                            ])
                        }))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let header = Row::new(["Input", "Module", "State", "Details"])
                .style(Style::default().fg(theme_color(Color::Magenta)).add_modifier(Modifier::BOLD))
                .bottom_margin(1);
            let mut state = TableState::default().with_offset(usize::from(scroll));
            frame.render_stateful_widget(
                Table::new(
                    rows,
                    [
                        Constraint::Length(20),
                        Constraint::Percentage(35),
                        Constraint::Length(18),
                        Constraint::Min(12),
                    ],
                )
                .header(header)
                .block(panel("Execution table"))
                .column_spacing(2),
                areas[2],
                &mut state,
            );
        }
    }
}
/// Open the dashboard and delegate workflow launches to the caller.
pub fn run<F>(config: Option<PathBuf>, launcher: F) -> Result<(), DashboardError>
where
    F: Fn(PathBuf) -> LaunchHandle,
{
    run_with_diagnostics(config, DiagnosticLog::default(), launcher)
}
/// Open the dashboard with a live diagnostic event destination.
pub fn run_with_diagnostics<F>(config: Option<PathBuf>, diagnostics: DiagnosticLog, launcher: F) -> Result<(), DashboardError>
where
    F: Fn(PathBuf) -> LaunchHandle,
{
    let store = Store::system()?;
    let mut dashboard = Dashboard::new(config, store, diagnostics)?;
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(stdout(), EnterAlternateScreen)?;
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
fn theme_color(color: Color) -> Color {
    match env::var_os("NO_COLOR") {
        | Some(_) => Color::Reset,
        | None => color,
    }
}

#[cfg(test)]
mod tests;
