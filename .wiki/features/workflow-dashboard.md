---
type: Plan
title: "Add a workflow dashboard and durable run history"
description: "Add a Ratatui dashboard that launches workflows, renders live scheduler and system state, projects completion, and reads canonical run bundles from the system cache."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli, xylem-tui, workspace]
tags: [feature, tui, dashboard, observability, history, telemetry, scheduler, config]
status: planned
---

# Goal

Add an engaging terminal dashboard for launching Xylem workflows, following
their live progress, and inspecting retained runs. The TUI and headless CLI
must execute through one runner and write the same durable evidence.

The dashboard reports workflow, scheduler, process, host, log, and projection
state. It reads run bundles from Xylem's platform cache directory so a later
session can inspect the same data without a separate database.

# Non-goals

The first version does not add a web server, background daemon, remote
attachment, pause, task retry, workflow editing, or automatic run pruning. It
does not build a second process supervisor, memory admission loop, or JSONC
parser inside Xylem.

# Current evidence

- The CLI has one `run` subcommand. Its optional `--config` flag falls back to
  `config.json` in the current directory.
- `Config` reads JSON, YAML, and YML. It does not read JSONC or discover the
  canonical hidden Xylem filenames.
- `xylem-cli::commands::run` owns configuration selection, preparation,
  workspace and journal creation, container lifecycle, input dispatch,
  reporting, and exit mapping in one function. A TUI cannot reuse that
  orchestration without invoking CLI presentation code.
- `ProgressObserver` exposes preparation and module state, but it does not
  provide scheduler reservations, process output, host metrics, projections,
  or a complete run lifecycle.
- `RunJournal` is append-only and recoverable, but journals currently follow
  report or temporary paths rather than one system run store. Child stdout and
  stderr are buffered through `CommandSpec::output()` until exit.
- The in-progress [memory-aware scheduler](memory-aware-scheduling.md) owns
  memory estimation and admission. Its event and simulation seams are the
  source of truth for dashboard memory state and projected admission.
- The adjacent ACORN worktree provides design examples through `acorn-tui`,
  but its reusable widgets and runtime are private. ACORN's public command,
  host, color, and GPU schema types are the useful dependency boundary.

# Project code boundaries

- **Update:** workspace and crate manifests, CLI argument and dispatch types,
  run orchestration, configuration reading, script execution adaptation,
  progress and journal events, reports, schemas, help, documentation, and
  workflow tests.
- **Create:** the `xylem-tui` crate; reusable runner, run-store, task-log,
  system-sampling, history, and projection modules in `xylem-lib`; the current
  run-bundle schemas; dashboard tests and deterministic telemetry fixtures.
- **Delete:** the `--config` flag, `config.json` fallback, older journal and
  summary readers and fixtures, direct CLI-owned workflow orchestration, and
  Xylem-local process capture or cancellation code superseded by ACORN.

# Dependencies and landing order

1. Land and publish ACORN's
   [Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md).
   Update ACORN versions and Git revisions together; do not use sibling path
   dependencies.
2. Land [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
   so execution has immutable contexts, isolated workspaces, managed process
   results, and shared cancellation.
3. Land [memory-aware scheduling](memory-aware-scheduling.md), including a
   read-only scheduler snapshot/event contract and a pure simulation function.
   The dashboard consumes these interfaces and cannot supply its own admission
   policy.
4. Implement the shared runner and canonical run store before the TUI. Keep
   the existing CLI progress display as the first event-stream adapter.
5. Add telemetry, history, projections, and the TUI after CLI parity and run
   bundle recovery tests pass.

Configuration discovery can land after the ACORN host prerequisite without
waiting for memory scheduling. Runner, journal, scheduler, CLI, manifest, and
generated-help changes remain serialized with their existing plan owners.

# User-facing CLI contract

Replace the config flag with an optional first positional argument:

```text
xylem run [CONFIG] [OPTIONS]
xylem tui [CONFIG] [OPTIONS]
```

Remove `--config` without a compatibility alias. Both subcommands accept an
explicit `.json`, `.jsonc`, `.yaml`, or `.yml` path. An explicit path always
wins and a missing explicit path reports that path; discovery never masks the
error.

When `CONFIG` is omitted, pass these names to ACORN's generic ordered resolver:

1. `.xylem.json`
2. `.xylem.jsonc`
3. `.xylem.yaml`
4. `.xylem.yml`

`xylem run` returns a usage error listing the four names when none exists.
`xylem tui` still opens the run browser; if a default exists, the New Run form
preselects it. Supplying `CONFIG` to the TUI preloads the launch form and still
requires confirmation.

JSONC uses ACORN's strict parser. It accepts line comments, block comments,
and trailing commas. It rejects loose property names, missing commas, single
quotes, hexadecimal numbers, and unary-plus numbers. Preserve Xylem's YAML
merge behavior. Default discovery applies only to the top-level omitted
`CONFIG`; nested configuration modules retain their existing location rules
and gain JSONC only when their explicit extension identifies it.

Factor workflow launch flags into shared Clap types. The TUI accepts inputs,
offline and resume behavior, GPU monitoring, container policy, artifact
output, allowlists, and verification overrides. Reporter selection and
`--report-output` remain headless `run` options because the dashboard renders
its own summary from the canonical store.

# Shared runner contract

Move run orchestration into `xylem-lib` behind these public concepts:

- `RunRequest` contains the selected configuration path, inputs, execution
  limits, scheduler settings, and launch options.
- `RunEngine::execute` owns planning, preparation, container lifecycle,
  scheduling, module execution, finalization, and terminal outcome.
- `RunEventSink` receives preparation, scheduler, input, module, output,
  container, metric, projection, and terminal events.
- `RunCancellation` exposes graceful cancellation and an internal forced child
  cancellation path.
- `RunResult` and `RunError` retain stable outcome and exit classifications.

Represent runner state as events reduced into immutable snapshots. The CLI's
Indicatif progress display and the TUI are adapters over the same event
stream. No adapter may call a separate workflow execution path.

Run every long-lived child through ACORN's managed command API. Bridge the
byte-oriented `OutputSink` to durable task logs while preserving stdout and
stderr attribution. Retain bounded output tails and ACORN truncation metadata
in memory. Do not decode or sanitize bytes until a presentation or log-writing
boundary requires text.

The first cancellation request stops new admissions and lets active modules
finish. It marks queued work cancelled and releases scheduler reservations as
inputs reach terminal states. A second process signal or unrecoverable shutdown
uses ACORN's cancellation token to terminate and reap active process groups.

# Canonical run store

Resolve the platform cache directory with ACORN's generic host helper using
qualifier `org`, organization `ornl`, application `xylem`, and namespace
`runs`. Failure to resolve or create this directory is a run startup error;
there is no temporary fallback. Tests inject an isolated store root through
the library constructor rather than changing production CLI precedence.

Each run owns this layout:

```text
runs/<run-id>/
  manifest.json
  journal.jsonl
  metrics.jsonl
  summary.json
  logs/run.jsonl
  logs/input-<index>/module-<index>.jsonl
```

The manifest records immutable identity, start time, selected config path and
digest, sanitized launch options, scheduler and estimator identity, and file
schema versions. Never persist resolved environment values, credentials, or
credential-bearing URLs.

The journal remains the authoritative workflow and scheduler event stream.
`metrics.jsonl` stores one sample per second. Task log records contain a
timestamp, monotonic sequence, stream, and sanitized UTF-8 text. Maintain a
partial-line buffer per stream and flush remaining fragments when the command
ends. Run-level preparation and infrastructure output goes to `logs/run.jsonl`.

Write the manifest and summary atomically. Append and flush journal, metric,
and log records as they occur. Independent run directories avoid a shared
mutable index; history scans manifests and sorts by recorded start time. Keep
all runs until a later retention plan defines deletion.

Canonical data always goes to the system run store. `--report-output` writes
additional requested reports and a journal copy; it cannot relocate the
canonical bundle. Artifact output semantics remain unchanged.

A bundle without a terminal summary is `running` while owned by a live local
process and `interrupted` afterward. Readers may ignore one incomplete final
JSONL line while its writer is live. A malformed complete record marks the
bundle corrupt.

# Journal and run-bundle compatibility

Make a clean break at the schema current when this plan lands. Keep one
`CURRENT_JOURNAL_SCHEMA` and one current run-bundle version. Delete older
readers, normalization code, schemas, and compatibility fixtures. Do not add a
migration command.

The history browser may list an incompatible directory as `unsupported` from
its outer manifest, but it must not replay, normalize, or infer state from an
older journal. Every execution-plan, journal, summary, metric, and manifest
test targets the current schema only.

# Scheduler telemetry and projections

Consume scheduler events containing:

- scheduler mode and estimator identity;
- configured budget, headroom, reserved bytes, and available bytes;
- batch ceiling, active inputs, and waiting inputs;
- each input's estimate, observed peak, queue position, admission, release,
  cancellation, oversized state, and memory-wait reason.

Use the landed scheduler's pure simulator to forecast future admissions. The
TUI and projection code never duplicate its budget, fairness, batch, or
oversized-input rules.

Build duration history from the latest 50 successful runs with the same
normalized configuration fingerprint. Use at most the latest 20 successful
samples per module. Exclude failed, cancelled, skipped, corrupt, and
unsupported-schema observations.

The live progress chart uses cumulative terminal module-input steps. Solid
points show observed completions. Future completion points come from the
scheduler simulator using P50 module durations; P10 and P90 histories provide
the displayed confidence bounds. Show ETA, projected finish time, sample
count, and confidence. A cold start uses completed work from the current run
and equal-weight remaining tasks, labeled `low confidence`.

# System telemetry

Sample once per second and persist:

- host CPU and load, memory, swap, network totals and rates, disk I/O, and
  filesystem capacity;
- CPU, RSS, I/O, and process count for Xylem and its descendants;
- scheduler reservations, estimates, and observed memory peaks;
- existing NVIDIA GPU utilization and memory readings when GPU monitoring was
  explicitly enabled and `nvidia-smi` is available.

Keep live sampling in Xylem because ACORN's diagnostic snapshot is not a
continuous process-tree monitor. Reuse ACORN schema GPU inventory and reading
types where they match the existing Xylem monitor. Missing GPU support is an
ordinary unavailable state and does not fail a run.

# Dashboard design

Use Ratatui 0.30 and Crossterm in a default-enabled `tui` feature. A
`--no-default-features` build retains the headless CLI. Use ACORN's public ORNL
color constants for the default theme, honor `NO_COLOR`, and pair every color
state with text or a symbol.

The layout draws from the dense live metrics in
[Zenith](https://github.com/bvaisvil/zenith) and
[bottom](https://github.com/ClementTsang/bottom), Grafana's
[overview and drill-down guidance](https://grafana.com/docs/grafana/latest/visualizations/dashboards/build-dashboards/best-practices/),
Airflow's [grid and task-instance views](https://airflow.apache.org/docs/apache-airflow/stable/ui.html),
and the local ACORN TUI's compact controls and TestBackend coverage.

Provide five tabs:

1. `Overview` shows run status, elapsed time, ETA and confidence, the live
   progress projection, throughput, memory admission, host and process
   sparklines, optional GPU state, and recent activity.
2. `Workflow` renders input rows by module columns. Each cell shows pending,
   memory-waiting, running, succeeded, failed, skipped, or cancelled state.
   The selected cell shows duration, estimate, observed peak, queue reason,
   and dependencies.
3. `System` renders rolling host, process-tree, scheduler, disk, network, and
   optional GPU charts from persisted metric samples.
4. `Logs` selects run-level or module logs and supports stream filtering,
   search, follow mode, truncation notices, and historical viewing.
5. `Runs` lists every bundle newest first with status, configuration, and time
   filters, then opens a read-only historical detail view.

At 80 by 24, show the selected primary panel and a compact status bar. At 120
by 36, show the standard two-column overview. Wider terminals add details and
longer charts without changing navigation.

Controls are `1` through `5` for tabs, arrows or `j` and `k` for navigation,
`Enter` for details, `/` for filter or search, `f` for log follow, `n` for New
Run, `c` for confirmed graceful cancellation, `?` for help, and `q` to exit.
Exiting during a live run requires staying open or cancelling and waiting for
cleanup because the first release has no detached runner.

# Implementation phases

1. Pin the published ACORN prerequisite, add positional config discovery and
   JSONC, and update CLI help and examples.
2. Extract `RunEngine`, preserve headless behavior through the event adapter,
   and create the canonical run store with the clean current schema.
3. Move child execution to ACORN managed commands and add durable logs,
   one-second host/process metrics, and scheduler event persistence.
4. Add deterministic historical duration statistics and scheduler-backed
   projections.
5. Add `xylem-tui`, responsive tabs, launch and cancellation dialogs, run
   history, and terminal restoration.
6. Remove superseded CLI orchestration and legacy readers, then update user and
   architecture documentation.

# Validation

## ACORN and configuration

- Compile against the published ACORN revision and prove no sibling path
  dependency is present.
- Test positional config before and after flags, rejection of `--config`, all
  default-name combinations and exact precedence, explicit missing paths,
  `run` with no default, and history-only TUI startup.
- Test JSON, strict JSONC, YAML/YML merge behavior, empty files, unsupported
  extensions, and explicitly referenced nested JSONC.

## Runner and storage

- Feed identical requests through CLI and TUI adapters and compare plans,
  scheduler decisions, journals, summaries, exit classifications, and
  cancellation results.
- Test concurrent run writers, atomic manifest and summary replacement,
  live trailing records, interrupted and corrupt runs, secret omission,
  report copies, and all-run retention.
- Reject every non-current journal and bundle schema. Keep no legacy fixture
  that is expected to deserialize successfully.
- Use deterministic child probes for interleaved streams, partial lines,
  non-UTF-8 bytes, large output, bounded tails, sink failures, timeouts,
  non-zero exits, graceful cancellation, forced cleanup, and descendants.

## Scheduler, telemetry, and projection

- Compare every displayed scheduler snapshot and forecast with the landed
  scheduler's event stream and pure simulator.
- With deterministic clocks and histories, verify percentile selection,
  exclusions, sample limits, confidence labels, ETA, and progress chart
  points.
- Test one-second samples with missing processes, short commands, process-tree
  changes, counter resets, unavailable filesystems, and absent or failing GPU
  probes.

## TUI and end-to-end behavior

- Use Ratatui `TestBackend` for every tab at compact, standard, and wide sizes,
  including monochrome, loading, empty, corrupt, unsupported, and long
  untrusted-log states.
- Test all navigation, launch, help, search, follow, cancellation, and exit
  dialogs with a pure application-state reducer.
- Launch a multi-input workflow, observe live state within one second, cancel
  it gracefully, reopen the TUI, and recover the same result from the run
  bundle.
- Verify terminal raw mode, alternate screen, cursor, and signal handlers are
  restored after success, errors, cancellation, and panic unwinding.
- Run workspace tests and lint on Linux, macOS, and Windows; exercise optional
  NVIDIA telemetry on a supported Linux host.

# Acceptance criteria

- `xylem run [CONFIG]` and `xylem tui [CONFIG]` implement the documented
  positional and four-name discovery contract, including strict JSONC.
- Every started run writes a canonical bundle beneath the Xylem system cache
  `runs` directory, independent of report export and temporary workspaces.
- CLI and TUI use one runner and produce equivalent durable outcomes.
- The dashboard updates live workflow, scheduler, log, and system state within
  one second and displays scheduler-backed ETA with a stated confidence.
- The progress chart distinguishes observed work from projected P50 and P10 to
  P90 completion bounds.
- Xylem reuses ACORN managed execution, JSONC policy, config discovery, cache
  resolution, ORNL colors, and GPU schema types where applicable.
- Xylem contains no second process supervisor, JSONC policy literal, or memory
  admission model.
- Only the current journal and run-bundle schema is readable. There is no
  migration or old-version normalization path.
- The TUI remains usable at 80 by 24, supports monochrome terminals, restores
  terminal state on every exit path, and can inspect all retained runs.

# Related plans

- [ACORN runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Memory-aware scheduling](memory-aware-scheduling.md)
- [Kubernetes workflow execution](kubernetes-workflow-backend.md)
