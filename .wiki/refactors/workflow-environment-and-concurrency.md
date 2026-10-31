---
type: Plan
title: "Isolate workflow environments and define concurrency"
description: "Replace process-global environment mutation and implicit scheduling with immutable per-input contexts, isolated workspaces, and deterministic workflow outcomes."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, environment, concurrency, cancellation, workspace, determinism]
status: planned
priority: next
execution: phased
phase_budget_hours: 5
---

# Goal

Give each workflow run and input an immutable execution context containing its variables, owned paths, limits, failure policy, and cancellation state. Run inputs through one bounded scheduler, preserve configured result order, and keep every module's acquisition, environment preparation, and command execution inside that input's workspace.

Xylem owns workflow policy. Generic child-process mechanics come from ACORN's
[Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md).

# Current evidence

Rechecked on 2026-10-31 against baseline `85c72df`, the current ENV-05/06
implementation diff, ACORN 0.5.1 at
`3e915697d214eda33e0d4c52f03fb09f27d9d3e4`, current schema/help artifacts,
and focused process, configuration, CLI, sandbox, and plan call paths.

- Policy-aware fixed and memory dispatch now return ordered structured reports,
  use the run-local worker pool, cancel active fail-fast siblings, preserve the
  triggering failure, and admit all eligible inputs under `continue`. Legacy
  optional-slot methods remain as compatibility wrappers. Native acquisition,
  local materialization, environment preparation, and execution share that
  bounded admission slot.
- Runtime command construction uses the immutable context resolver. The legacy
  `Config::get_environment_variables` compatibility path still returns a
  mutable prefixed `HashMap` to planning and library callers and remains for
  ENV-09 cleanup.
- `EnvironmentValue::{set_variable,set_variables,set_namespaced_variables}` can mutate the parent process environment. Tests also mutate `_XYLEM_*` variables and depend on process-wide cleanup.
- `Script::execute` and the CLI module executor already use ACORN's structured
  result, separate bounded byte streams, streaming sink, timeout option, and
  cancellation token. `ProcessError` now retains ACORN's complete checked result
  or partial operational context through one host-and-sandbox adapter; focused
  tests pin truncation, duration, termination, timeout, cancellation, sink or
  cleanup context, and non-UTF-8 behavior. The legacy `run_checked`
  compatibility method still combines lossy text into `Vec<String>`.
- `Instruction::ShellFormat` now uses ACORN's shell-word parser, preserves
  quoted, escaped, and empty arguments, and reports malformed templates through
  deterministic execution planning.
- Configuration now accepts `options.failure_policy` with a fail-fast default
  and positive `timeout_seconds` on executable modules across JSON, JSONC,
  YAML, and YML. Configuration wrappers reject timeouts.
- `xylem run --failure-policy` accepts `fail-fast` and `continue`; an explicit
  CLI value overrides configuration while omission preserves the configured or
  default policy. Checked-in run help records the accepted spelling.
- Remote configuration resolution now uses the run-owned `configuration/` directory. Each input receives separate `modules/`, `environments/`, and `scratch/` paths, including repeated input values.
- The CLI now owns `${TMPDIR}/xylem/workspaces/<run-id>` through `RunWorkspace`. It removes the tree after success, failure, or unwind; `--retain-workspace` transfers ownership and reports the path. Durable evidence remains independently owned by the canonical run store.
- `RunCancellation` and durable `cancelled`/`not_started` states are landed.
  TUI-launched runs force the shared cancellation token. Headless runs install
  termination-signal handling: the first signal cancels for normal journal and
  workspace cleanup, a second exits immediately with code 130, and externally
  cancelled reports also map to exit 130.
- `RunContext` now owns the run workspace, invocation and configuration
  directories, effective thread and batch limits, failure policy, cancellation,
  artifact override, and retention choice. Each admitted input constructs a
  `WorkflowContext` with distinct owned module, environment, and scratch paths.
- Context variable resolution is a pure ordered map. Workflow defaults, module
  overrides, the CLI artifact override, and derived runtime values have tested
  precedence; reserved names and non-Unicode runtime paths fail before command
  construction. Zero-input and repeated-input contexts are covered.
- Input workspaces are created only after scheduler admission. Native module
  materialization and Conda, Mamba, and Pixi preparation remain inside that
  slot, with distinct module and environment roots for repeated inputs.
- Volatile `RUN_ID` and `WORK_DIRECTORY` values reach child commands but are
  excluded from completion fingerprints, preserving resume compatibility.
- Execution plans are at version 14 and journals/summaries are at version 13.
  The landed schemas include selection, sandbox, artifact, memory, cancellation,
  and lifecycle evidence that the next runtime revision must preserve.
- Sandboxed execution already creates per-input preparation/execution
  boundaries with clean environments and staged artifacts. Those contracts do
  not fix the shared mutable host path and must remain intact during this
  refactor.
- Validation passed on 2026-10-31: all 312 xylem-lib tests (2 ignored), all 12
  executed xylem-lib doctests (4 ignored), all 68 xylem-cli tests (3 ignored),
  strict all-target/all-feature Clippy for both packages, no-default-feature CLI
  checking, formatting, and rustdoc generation. Dependency-policy validation
  accepts the updated lockfile thresholds and stops at the pre-existing expired
  `RUSTSEC-2026-0097` exception, whose review was due 2026-10-28.

# Ownership boundary

## ACORN `acorn-cmd`

The upstream crate owns process groups or Windows Job Objects, concurrent stdout/stderr draining, bounded byte capture, streaming sinks, timeouts, cancellation tokens, termination metadata, typed checked errors, shell-word parsing, diagnostic rendering, and guaranteed reap behavior. It does not install signal handlers or decide workflow failure policy.

## Xylem

Xylem adapts `Script` to the ACORN API and owns configuration, variable precedence, path resolution, workspace layout, input admission, sequential module order, failure policy, journal state, result ordering, CLI signal handling, and exit codes. Xylem must not fork a second process supervisor.

# Project code boundaries

- **Update:** configuration, execution, script, plan, journal, run store,
  reporter, TUI, and CLI run modules; the dependency lockfile, current schemas,
  focused tests, and user documentation.
- **Create:** `DispatchReport<T>`, focused process fixtures, and the next
  coordinated plan and summary schemas.
- **Delete:** production parent-environment mutation APIs, the combined
  `Vec<String>` script result, and whitespace-only shell-format parsing.

# Dependencies and landing order

1. Preserve the pinned ACORN 0.5.1 managed-process contract while adapting the
   remaining checked-error and sandbox paths. The matching 0.5.1 crates are
   published on crates.io; Xylem currently pins Git revision
   `3e915697d214eda33e0d4c52f03fb09f27d9d3e4`.
2. Keep Xylem's ACORN versions and Git revisions together. Published Xylem crates must resolve the same process contract from crates.io; a Git-only prerequisite is insufficient.
3. Land the Xylem context, scheduler, journal, and CLI changes as
   coordinated phases. `crates/xylem-lib/src/script/mod.rs`, run-state files,
   manifests, and `crates/xylem-cli/src/commands/run/` remain single-writer
   integration surfaces.
4. Allocate the next execution-plan and journal/run-summary versions from v14
   and v13. Preserve selection, sandbox, artifact, memory, lifecycle,
   cancellation, and run-store compatibility; coordinate version allocation
   with Kubernetes.

This plan follows the typed workflow-outcome contract and preserves the
existing `options.batch` ceiling plus landed memory-aware admission. Its
durable outcomes feed the journal, reporters, TUI, sandbox lifecycle, and
run-store projections.

# Public configuration and CLI contract

- Add `options.failure_policy` with `fail_fast` and `continue` values. Omission means `fail_fast` for backward compatibility.
- Add `--failure-policy <fail-fast|continue>` as a CLI override. CLI spelling uses kebab case; serialized configuration uses snake case.
- Add positive `timeout_seconds` to each executable module. Omission means no timeout. Expose a lower-level library execution option so callers that do not deserialize a module can still supply a timeout.
- Preserve the landed CLI-only `--retain-workspace` behavior while constructing
  immutable contexts; do not add retention to reusable workflow configuration.
- Keep `--report-output` as an additional report-export directory; canonical journals and summaries remain in the run store, separate from the workspace tree.
- Reject zero timeouts, unknown policies, and attempts to configure retained workspaces in workflow files.

# Context contracts

`RunContext` owns the run identifier, invocation directory, configuration source directory, workspace owner, journal path, resolved thread budget, batch ceiling, failure policy, cancellation token, and diagnostic retention choice.

`WorkflowContext` is immutable and input-specific. It owns the input index and value, resolved variables, input workspace paths, and references to immutable run-level policy. A new context is built even for a zero-input workflow and for each repeated input value.

`DispatchReport<T>` extends the current ordered optional-slot behavior. It
returns one slot per configured input, the effective ceiling, whether admission
stopped early, and the cancellation reason. Completion order must never
determine result order, and a missing slot must have an explicit
`not_started` or `cancelled` disposition rather than relying on `None` alone.

# Variable and path resolution

Build variables as a pure `BTreeMap<String, String>` with this precedence, from lowest to highest:

1. workflow variables;
2. the top-level output directory default;
3. module variables;
4. the CLI artifact override;
5. derived reserved values.

Reserve `INPUT`, `MODULE_LIST`, `PARENT_DIRECTORY`, `RUN_ID`, and `WORK_DIRECTORY`. Reject these names in workflow or module variables instead of silently overwriting them. Keep names unprefixed during interpolation and plan rendering; add `_XYLEM_` only while constructing one child command's environment.

Resolve configuration-owned input, output, module, template, and environment paths relative to the configuration file that declares them. Resolve CLI path arguments relative to the invocation directory captured by `RunContext`. Reject a non-Unicode path before converting it to a template value or environment string; do not apply a lossy conversion.

Zero-input workflows receive workflow values, the output default, module overrides, `MODULE_LIST`, `RUN_ID`, and `WORK_DIRECTORY`, but no `INPUT` or `PARENT_DIRECTORY`.

# Remaining workspace and preparation work

The RAII run workspace, per-input module copy, remote configuration directory,
and CLI retention option have landed. Keep their contracts while replacing
shared or process-global environment state. Conda/Mamba prefixes and Pixi
projects must remain input-local, including repeated input values; a manager
that cannot demonstrate an input-local path fails validation. A mutable
preparation step must run inside the same bounded admission slot as its input's
modules. Existing workspace cleanup and journal lifetime stay regression gates.

# Scheduling, failure, and cancellation

Use one admission ceiling equal to `min(--threads, options.batch)`. Extend or
replace the current dispatcher so one slot covers input-local acquisition,
local copying, environment preparation, sequential module execution, and
completion observation. Run-level configuration resolution and immutable cache
lookup may occur before dispatch; mutable preparation may not. Do not create a
second preparation or input worker pool.

Modules within one input remain sequential. Independent inputs may run concurrently, but the returned report and journal summary remain in configuration order.

The policies are:

- `fail_fast`: after the first failed input, stop admitting inputs, cancel already-active sibling inputs, and leave never-admitted inputs as `not_started`. The input that triggered fail-fast remains `failed`; cancelled siblings are `cancelled`.
- `continue`: admit every input subject to the ceiling. One input failure does not cancel siblings.
- external cancellation: stop admission, cancel every active input, mark active and never-admitted inputs `cancelled`, flush the journal, and return CLI exit code 130.

CLI signal handlers belong only in `xylem-cli`. Add `ctrlc` with its termination feature there, and use the first supported termination signal to cancel the shared token so normal cleanup can run. A second signal may use the CLI's documented forced-exit path. Library callers cancel through the token and receive structured outcomes; `xylem-lib` must not install global handlers.

# Process adapter and output

`Script` constructs an `acorn_cmd::CommandSpec`, applies the context's current directory and environment, and calls the new execution API with the module timeout and run cancellation token. Legacy shell-format instructions use ACORN's public shell-word parser; exec-format arrays remain exact and preferred.

Keep stdout and stderr as separate byte tails. Xylem may stream chunks to tracing or reporter adapters, but it must preserve ACORN's per-stream ordering, truncation metadata, duration, exit code or signal, and termination reason. Decode text only in presentation code. A nonzero exit, timeout, cancellation, spawn failure, sink failure, and wait failure map to distinct Xylem process outcomes.

The caller applies workflow failure policy. `Script` never logs an error and returns an empty success sentinel.

# Journal and schema revision

Use the landed `cancelled` and `not_started` durable vocabulary. Advance the
execution plan from v14 and journal/recovered summary from v13 only when the
new context, failure-policy, timeout, and workspace evidence is stable. Keep
the current exact-version runtime policy: retain historical schema documents,
but reject non-current runtime artifacts without normalization or migration.

The new versions preserve all current selection, sandbox, artifact, memory,
lifecycle, and cancellation evidence. Add effective failure policy, timeout,
run and input workspace identities without persisting disposable absolute paths
as reusable configuration, ordered input disposition, cancellation reason,
process termination metadata, duration, and output truncation counts. Never
persist captured output beyond the journal's existing bounded diagnostic
policy.

Dry-run validates and renders the effective policy, timeout, reserved variables, declared runtime paths, and scheduler ceiling. It creates no workspace or journal, performs no acquisition or environment preparation, and represents runtime-generated values such as run and work directory with stable declarations rather than random concrete paths.

# Recommendation

Keep each remaining runtime contract independently reviewable. The accepted
workspace and acquisition path is a baseline, not a new phase. Do not combine
this refactor with Kubernetes behavior.

# Validation

- Test precedence, reserved-name rejection, zero-input behavior, non-Unicode paths, duplicate inputs, configuration-relative paths, and invocation-relative CLI paths.
- Run conflicting variables through many concurrent inputs and prove each child observes only its own `_XYLEM_*` environment while the parent environment remains byte-for-byte unchanged.
- Preserve the existing rolling-admission and ordered-slot tests, then measure peak work across input-local acquisition, preparation, and execution; it must never exceed `min(--threads, options.batch)`.
- Test deterministic ordering, sequential modules, fail-fast admission, continue behavior, external cancellation, exit 130, module timeout, and process-tree cleanup.
- Prove fail-fast cancels already-active siblings rather than merely waiting for them, and distinguish the triggering failure, cancelled active work, and never-admitted work.
- Keep the landed workspace-copy, containment, permission, retention, and
  cleanup tests as regression gates while checking input-local environment
  prefixes and Pixi projects.
- Round-trip the newly allocated plan, journal, and summary versions; reject
  non-current fixtures; assert explicit `cancelled`, `failed`, and
  `not_started` states.
- Prove dry-run performs no filesystem writes, process spawn, network access, environment mutation, or random identifier generation.
- Run focused library and CLI tests on Linux, macOS, and Windows, then run the workspace lint and test gates.

# Acceptance criteria

- Production workflow execution never mutates the parent process environment.
- Every input, including a repeated or zero-input case, has a complete immutable context and isolated owned paths.
- Configuration and CLI precedence, reserved variables, failure policy, timeout, and path ownership are explicit and tested.
- One ceiling and one worker/admission mechanism bound the complete input lifecycle, modules remain sequential, and reports are ordered deterministically.
- Mutable environment prefixes and preparation state remain isolated per input
  under the accepted workspace boundary.
- Fail-fast, continue, timeout, and external cancellation have distinct durable outcomes; external cancellation exits 130.
- Xylem delegates process supervision to the published ACORN contract and preserves separate bounded stdout/stderr evidence.
- The new schemas represent policy, timeout, context, and workspace evidence;
  preserve all current provenance and lifecycle evidence; and reject every
  non-current runtime artifact without normalization or migration.
- Dry-run creates no workspace or journal and performs no execution-side mutation.

# Related plans

- [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md)
- [Memory-aware scheduling](../features/memory-aware-scheduling.md)

# Execution phases

## ENV-07 — Version plan and dry-run policy evidence

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: none; reserve execution-plan v15 after v14
- Exact write set: `crates/xylem-lib/src/plan.rs`, `crates/xylem-lib/src/tests/plan.rs`, `docs/schema/execution-plan-v15.schema.json`
- Primary integration lock: run state
- Outcome: The current deterministic plan records effective policy, timeout, context identity, and ordered admission intent without disposable paths.
- Focused validation: plan/schema round trips, old-version rejection, and write-free dry-run tests.
- Handoff: Record v15 schema and consumer evidence; remove ENV-07 when accepted.

## ENV-08 — Version journal and summary outcomes

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: ENV-07; reserve journal/run-summary v14 after v13
- Exact write set: `crates/xylem-lib/src/journal/mod.rs`, `crates/xylem-lib/src/run/store.rs`, `crates/xylem-lib/src/journal/tests/mod.rs`, `crates/xylem-lib/src/run/tests.rs`, `crates/xylem-tui/src/lib.rs`, `crates/xylem-cli/src/commands/run/reporting.rs`, `docs/schema/run-summary-v14.schema.json`
- Primary integration lock: run state
- Outcome: Writers, recovery, reporters, and TUI retain ordered disposition, cancellation reason, timeout, and context evidence.
- Focused validation: summary round trips, recovery, reporter/TUI projections, and historical-version rejection.
- Handoff: Record v14 durable readers and remove ENV-08 when accepted.

## ENV-09 — Remove parent-environment mutation and document the contract

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: ENV-08
- Exact write set: `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `docs/src/configuration.md`, `docs/src/cli-reference.md`
- Primary integration lock: process adapter
- Outcome: Production workflow execution never changes the parent environment,
  configured module timeouts reach the ACORN execution options, and user docs
  match the accepted CLI and result behavior.
- Focused validation: concurrent conflicting-variable tests, module timeout
  execution, dry-run purity, and workspace lint and test gates.
- Handoff: Record platform results and durable API documentation, then retire this plan and its index, network, and lane entries.
