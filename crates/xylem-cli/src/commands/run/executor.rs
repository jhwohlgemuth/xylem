use acorn_cmd::{CaptureLimits, ExecutionOptions, OutputSink, OutputStream};
use acorn_core::time::Seconds;
use acorn_host::terminal::Label;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use owo_colors::OwoColorize;
use std::io;
use std::path::PathBuf;
use xylem_lib::config::ModuleExecutor;
use xylem_lib::run::store::{Bundle, TaskLogSink};
use xylem_lib::run::RunCancellation;
use xylem_lib::sandbox::{Prepared, SandboxInput};
use xylem_lib::script::Script;

struct BoundedTaskLogSink {
    inner: TaskLogSink,
    limit: Option<u64>,
    stderr: AtomicU64,
    stdout: AtomicU64,
}
pub(super) struct LoggedModuleExecutor {
    bundle: Bundle,
    cancellation: RunCancellation,
    sandbox: Option<SandboxInput<Prepared>>,
}
impl BoundedTaskLogSink {
    fn new(inner: TaskLogSink, limit: Option<u64>) -> Self {
        Self {
            inner,
            limit,
            stderr: AtomicU64::new(0),
            stdout: AtomicU64::new(0),
        }
    }
}
impl OutputSink for BoundedTaskLogSink {
    fn write(&self, stream: OutputStream, bytes: &[u8]) -> io::Result<()> {
        let counter = match stream {
            | OutputStream::Stderr => &self.stderr,
            | OutputStream::Stdout => &self.stdout,
        };
        self.limit
            .map_or(Ok(()), |limit| {
                let bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                counter
                    .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                        current.checked_add(bytes).filter(|updated| *updated <= limit)
                    })
                    .map(|_| ())
                    .map_err(|_| io::Error::other(format!("sandbox output exceeded the {limit}-byte per-stream limit")))
            })
            .and_then(|()| self.inner.write(stream, bytes))
    }
}
impl LoggedModuleExecutor {
    pub(super) fn new(bundle: Bundle, cancellation: RunCancellation, sandbox: Option<SandboxInput<Prepared>>) -> Self {
        Self {
            bundle,
            cancellation,
            sandbox,
        }
    }
}
impl ModuleExecutor for LoggedModuleExecutor {
    fn execute(
        &self,
        script: &Script,
        input_index: usize,
        module_index: usize,
        timeout: Option<Seconds>,
    ) -> Result<(), xylem_lib::error::ProcessError> {
        let sink = TaskLogSink::module(self.bundle.clone(), input_index, module_index);
        let output_limit = self.sandbox.as_ref().map(|sandbox| sandbox.resources().captured_output_bytes);
        let options = ExecutionOptions::new()
            .with_cancellation(self.cancellation.token())
            .with_sink(BoundedTaskLogSink::new(sink.clone(), output_limit));
        let options = self.sandbox.as_ref().map_or(options.clone(), |sandbox| {
            let resources = sandbox.resources();
            let captured_output_bytes = usize::try_from(resources.captured_output_bytes).unwrap_or(usize::MAX);
            options.with_capture_limits(CaptureLimits::new(Some(captured_output_bytes), None))
        });
        let sandbox_timeout = self
            .sandbox
            .as_ref()
            .map(|sandbox| Duration::from_secs(sandbox.resources().wall_time_seconds));
        let timeout = timeout.map(Duration::from).into_iter().chain(sandbox_timeout).min();
        let options = match timeout {
            | Some(timeout) => options.with_timeout(timeout),
            | None => options,
        };
        let execution = self.sandbox.as_ref().map_or_else(
            || script.execute_checked(&options),
            |sandbox| sandbox.execute(script, module_index, &options),
        );
        let result = execution.map(|execution| {
            let stdout = String::from_utf8_lossy(execution.stdout().bytes());
            let stderr = String::from_utf8_lossy(execution.stderr().bytes());
            if !stdout.is_empty() {
                tracing::trace!(target: "xylem_cli::output", "=> {} {}", Label::output(), stdout.trim_end().dimmed());
            }
            if !stderr.is_empty() {
                tracing::trace!(target: "xylem_cli::output", "=> {} {}", Label::output(), stderr.trim_end().dimmed());
            }
        });
        let flushed = sink.flush().map_err(|source| xylem_lib::error::ProcessError::Output {
            script: script.to_string(),
            working_directory: PathBuf::from(&script.current_dir),
            source: io::Error::other(source),
        });
        match (result, flushed) {
            | (Ok(()), Ok(())) => Ok(()),
            | (Err(primary), _) => Err(primary),
            | (Ok(()), Err(output)) => Err(output),
        }
    }
    fn is_cancellation_requested(&self) -> bool {
        self.cancellation.is_requested()
    }
}
