---
type: Plan
title: "Add a workflow dashboard and durable run history"
description: "Add a Ratatui dashboard over the existing fixed-concurrency workflow runtime, with live execution, logs, system metrics, cancellation, and retained history."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli, xylem-tui, workspace]
tags: [feature, tui, dashboard, observability, history, telemetry, config]
status: complete
---

# Goal

Make the initial workflow dashboard Xylem's first implementation priority.
Deliver one local terminal interface that launches workflows, follows live
progress, supports cancellation, and inspects retained runs. The TUI and
headless CLI execute through the same command runner and write the same durable
evidence.

This first release includes live workflow state, attributed module logs,
one-second system metrics, run history, and duration-based completion
projections. It preserves the current `--threads` and `options.batch`
execution semantics. The dashboard reads the canonical files on each refresh,
so those files remain the sole source of truth for live and historical views.

# Non-goals

This plan does not implement memory-aware admission, scheduler reservations,
oversized-input policy, or estimator-backed forecasting. It does not implement
the broader workflow environment/concurrency refactor, including immutable
per-input contexts, isolated workspaces, configurable failure policy, or a new
admission algorithm.

The first version also excludes a web server, background daemon, remote
attachment, detached execution, pause, task retry, workflow editing, automatic
run pruning, interactive log queries, historical filters, and chart widgets.
Those presentation refinements do not block the initial operational dashboard.

# Delivered baseline

- All five ACORN crates are pinned to v0.3.4 revision
  `36cd74041eaa4669636b7743071f3c334cdb3b7d`.
- `xylem-cli::commands::run` remains the single orchestration path used by both
  the headless command and the dashboard launcher.
- ACORN managed commands stream stdout and stderr into per-module durable logs
  and provide forced process-group cancellation.
- `RunJournal` and the canonical run store retain workflow state, summaries,
  logs, and metrics independently of report exports and temporary workspaces.
- Ratatui provides five live and historical views behind the default-enabled
  `tui` feature; the headless build remains available without default features.
- Memory-aware scheduling is separately blocked. The dashboard reports
  ordinary system memory without consuming its estimator or admission model.

# Dependencies and priority

ACORN v0.3.4 is the only runtime prerequisite. Upgrade all ACORN versions and
Git revisions together; do not add sibling path dependencies.

The dashboard does not wait for workflow environment/concurrency or
memory-aware scheduling. Those plans must later preserve the runner path and
versioned run-store contracts established here. Homebrew activation and
unrelated execution targets resume after dashboard acceptance.

The dashboard owned the manifest, CLI, run-state, run-store, and TUI
integration locks during implementation; those locks are released now that
the plan is complete.

# User-facing CLI contract

Use an optional first positional configuration argument:

```text
xylem run [CONFIG] [OPTIONS]
xylem tui [CONFIG] [OPTIONS]
```

Remove `--config` without a compatibility alias. Both subcommands accept an
explicit `.json`, `.jsonc`, `.yaml`, or `.yml` path. An explicit path always
wins, and a missing explicit path reports that path.

When `CONFIG` is omitted, pass these names to ACORN's ordered resolver:

1. `.xylem.json`
2. `.xylem.jsonc`
3. `.xylem.yaml`
4. `.xylem.yml`

`xylem run` returns a usage error listing the four names when none exists.
`xylem tui` still opens the run browser; a discovered default preselects the
New Run action. Supplying `CONFIG` to the TUI preloads that action and still
requires `n` confirmation.

JSONC uses ACORN's strict parser. Preserve Xylem's YAML merge behavior. Default
discovery applies only to an omitted top-level `CONFIG`; nested configuration
modules retain their location rules and gain JSONC only for an explicit JSONC
extension.

# Shared runner path

Keep preparation, batching, workspace, container, failure, and ordering
behavior in the existing command runner. Both the headless command and the TUI
launcher invoke that exact entry point. `RunCancellation` is the shared control
handle: its first request stops new admission after active work settles, and a
repeated request activates ACORN's process-group cancellation token.

The Indicatif CLI observes in-process progress. The dashboard tails the same
journal, metrics, summary, and log files used for history. This avoids a second
execution implementation or a live-only state channel that could disagree
with recovered evidence. Reporter selection and `--report-output` remain
headless-only because the dashboard renders canonical stored evidence.

Run long-lived children through ACORN's managed command API. Bridge the
byte-oriented `OutputSink` to durable task logs while preserving stdout and
stderr attribution. Decode and sanitize at the log boundary.

A graceful cancellation stops new input admission and lets active work settle.
Forced cancellation uses ACORN's token to terminate and reap active process
groups. Cancellation is a durable input, module, and run state.

# Canonical run store

Resolve the platform cache directory with ACORN's host helper using qualifier
`org`, organization `ornl`, application `xylem`, and namespace `runs`.
Failure to resolve or create it is a startup error. Tests inject an isolated
root through the library constructor or `XYLEM_RUN_STORE`.

Each run owns:

```text
runs/<run-id>/
  manifest.json
  journal.jsonl
  metrics.jsonl
  summary.json
  logs/input-<index>/module-<index>.jsonl
```

The manifest records immutable identity, start time, configuration path and
digest, process ID, thread ceiling, and schema versions. It never persists
resolved environment values or credentials.

The journal remains authoritative workflow state. `metrics.jsonl` stores one
sample per second. Log records contain timestamp, monotonic sequence, stream,
and sanitized UTF-8 text. Maintain partial-line buffers per stream and flush
fragments when a command ends.

Write manifests and summaries atomically. Append and flush journal, metric,
and log records as they occur. History scans independent run directories and
sorts by recorded start time; no shared mutable index is created. Keep all runs
until a later retention plan defines deletion.

Canonical evidence always goes to the configured run store, which is the
system cache unless `XYLEM_RUN_STORE` is set. `--report-output` writes
additional requested reports and a journal copy; it cannot relocate the
canonical bundle. Artifact output semantics remain unchanged.

A bundle without a terminal summary is `running` while owned by a live local
process and `interrupted` afterward. Readers may ignore one incomplete final
JSONL record while the writer is live. A malformed complete record marks the
bundle corrupt.

# Schemas and compatibility

Advance the execution-plan, journal, and summary formats to schema version 7
and add run-bundle version 1. These are the only readable schemas after the
plan lands. Delete older readers, normalizers, schemas, and successful
compatibility fixtures. Do not add a migration command.

The history browser may list an incompatible directory as `unsupported` from
its outer manifest, but it must not replay or infer state from older data.

# Metrics and duration projections

Sample once per second and persist:

- host CPU and one-minute load, memory, swap, network deltas, disk I/O deltas,
  and aggregate filesystem capacity;
- CPU, RSS, I/O deltas, and process count for Xylem and its descendants;
- existing NVIDIA GPU utilization and memory readings when GPU monitoring was
  explicitly enabled and `nvidia-smi` is available.

Ordinary memory observations do not imply memory admission.
When GPU monitoring is explicitly requested, a missing initial probe fails
before module execution; a later probe failure degrades monitoring without
changing an otherwise successful workflow result.

Build duration history from the latest 50 successful runs with the same
configuration path. Use at most the latest 20 successful samples per module.
Exclude failed, cancelled, skipped, corrupt, and unsupported observations.

Project remaining work with the current fixed batch/thread ceiling and P50
module durations. P10 and P90 histories provide confidence bounds. A cold
start with completed work uses current-run throughput and is labeled `low
confidence`; otherwise the overview reports that it is collecting samples.
This projection must not duplicate or predict the deferred memory-aware
scheduler.

# Dashboard design

Use Ratatui 0.30 and Crossterm behind a default-enabled `tui` feature. A
`--no-default-features` build retains the headless CLI. Honor `NO_COLOR`, and
pair every color state with text or a symbol.

Provide five tabs:

1. `Overview` shows run status, retained count, active state, configuration,
   and fixed-concurrency ETA with its confidence.
2. `Workflow` renders each input and its ordered modules with pending, running,
   succeeded, failed, skipped, skipped-existing, or cancelled state.
3. `System` renders recent persisted host, process-tree, disk, network, and
   filesystem samples.
4. `Logs` combines recent attributed module stdout and stderr for live or
   historical viewing.
5. `Runs` lists every bundle newest first with status and configuration;
   selection drives the other read-only views.

At 80 by 24 and above, show the selected primary panel and a compact status
bar. Text wrapping and bounded recent samples keep every view usable in the
compact layout.

Controls are `1` through `5` for tabs, arrows or `j` and `k` for navigation,
`f` for the log-follow preference, `n` for New Run, `c` twice for confirmed
cancellation, `?` for help, and `q` to exit. `/` reserves the future search
entry point and explains its current selected-run scope.
Exiting during a live run requires staying open or cancelling and waiting for
cleanup because the first release has no detached runner.

# Implementation phases

1. Reconcile portfolio order and dependency edges; pin ACORN v0.3.4.
2. Add positional discovery and strict JSONC, then adapt `Script` to managed
   ACORN execution.
3. Reuse the existing command runner from both adapters and add shared
   two-stage cancellation.
4. Add the canonical run store, version-7 state, version-1 bundle, logs,
   metrics, recovery, and duration projections.
5. Add `xylem-tui`, five responsive tabs, launch and cancellation controls,
   history, and terminal restoration.
6. Remove superseded readers, then update user and architecture documentation.

# Validation

- Compile the workspace against the pinned ACORN v0.3.4 revision with every
  feature, and compile the headless configuration without default features.
- Test positional configuration parsing, rejection of `--config`, strict
  JSONC, YAML merging, and generated help.
- Preserve the existing workflow integration suite for ordering, batching,
  reporting, containers, resume, selection, and failure behavior.
- Test exclusive bundle creation, running/interrupted/corrupt history states,
  stream attribution, partial-line flushing, duration projections, and
  two-stage cancellation.
- Render all five tabs at 80 by 24 with Ratatui `TestBackend` and verify the
  terminal guard restores raw mode and the alternate screen by construction.
- Run formatting, Clippy with warnings denied, workspace tests, documentation,
  dependency policy, and diff hygiene before handoff.

# Acceptance criteria

- `xylem run [CONFIG]` and `xylem tui [CONFIG]` implement positional discovery
  and strict JSONC.
- Every started run writes a canonical configured-store bundle independent of
  report exports and temporary workspaces.
- CLI and TUI use one runner and produce equivalent durable outcomes.
- The dashboard updates live workflow, logs, and system state within one
  second and shows fixed-concurrency ETA with stated confidence.
- Xylem reuses ACORN managed execution, JSONC, config discovery, cache
  resolution, and cancellation.
- The current batching, preparation, workspace, container, ordering, and
  failure behavior remains unchanged except for explicit cancellation.
- Only schema 7 and run-bundle version 1 are readable.
- The TUI works at 80 by 24, supports monochrome terminals, restores terminal
  state, and inspects all retained runs.

# Deferred integration

When memory-aware scheduling is unblocked, it adds reservation and admission
records to the durable journal or metrics stream and adds optional dashboard
fields. It may not replace the runner, store, or TUI execution path.

The workflow environment/concurrency refactor may later change runner
internals, but it must preserve cancellation, journal, summary, and bundle
behavior or version those contracts explicitly.
