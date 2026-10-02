---
type: Plan
title: "Isolate workflow environments and define concurrency"
description: "Replace process-global environment mutation and implicit scheduling with immutable per-input contexts, isolated workspaces, and deterministic workflow outcomes."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, environment, concurrency, cancellation, workspace, determinism]
status: planned
---

# Goal

Give each workflow run and input an immutable execution context containing its variables, owned paths, limits, failure policy, and cancellation state. Run inputs through one bounded scheduler, preserve configured result order, and keep every module's acquisition, environment preparation, and command execution inside that input's workspace.

Xylem owns workflow policy. Generic child-process mechanics belong to ACORN's [`acorn-cmd` process execution contract](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/process-execution-contract.md).

# Current evidence

- `Config::get_environment_variables` merges workflow and module values into a mutable `HashMap`, expands them, and emits `_XYLEM_*` names before an immutable run context exists.
- `EnvironmentValue::{set_variable,set_variables,set_namespaced_variables}` can mutate the parent process environment. Tests also mutate `_XYLEM_*` variables and depend on process-wide cleanup.
- `Script` now carries per-command environment overrides, but it still returns combined text lines rather than the structured, bounded process result needed for timeout and cancellation policy.
- `Instruction::ShellFormat` still loses quoted and empty argument boundaries through whitespace splitting.
- `options.batch` provides rolling admission and ordered outcomes through `ExecutionContext`, while the CLI separately creates a Rayon pool from `--threads`. Preparation and command execution do not yet share one explicit admission contract.
- Configuration resolution, module acquisition, virtual-environment preparation, and execution can reuse host paths across inputs. Duplicate inputs can therefore collide even when their variable maps differ.
- The CLI creates a random temporary root without an explicit layout, retention policy, or cleanup owner. The execution journal is durable run evidence and must not share that disposable lifetime.
- Execution-plan, journal, and summary schemas are at version 5. Cancellation is not a durable terminal state.

# Ownership boundary

## ACORN `acorn-cmd`

The upstream crate owns process groups or Windows Job Objects, concurrent stdout/stderr draining, bounded byte capture, streaming sinks, timeouts, cancellation tokens, termination metadata, typed checked errors, shell-word parsing, diagnostic rendering, and guaranteed reap behavior. It does not install signal handlers or decide workflow failure policy.

## Xylem

Xylem adapts `Script` to the ACORN API and owns configuration, variable precedence, path resolution, workspace layout, input admission, sequential module order, failure policy, journal state, result ordering, CLI signal handling, and exit codes. Xylem must not fork a second process supervisor.

# Project code boundaries

- **Update:** `Cargo.toml`, `Cargo.lock`, `xylem-lib/Cargo.toml`, configuration, plan, progress, outcome, execution, and script modules in `xylem-lib/src/`; run orchestration and CLI arguments in `xylem-cli/src/`; schemas, fixtures, tests, and user documentation.
- **Create:** immutable `RunContext` and `WorkflowContext`; `FailurePolicy`, `CancellationReason`, `RunWorkspace`, and `DispatchReport<T>`; schema-v6 files; per-input workspace and fake-process fixtures.
- **Delete:** production process-environment mutation APIs, the combined `Vec<String>` script result, whitespace-only shell-format parsing, leaked temporary roots, and any Xylem-local child-process lifecycle implementation superseded by `acorn-cmd`.

# Dependencies and landing order

1. Land and publish the additive ACORN process contract as `acorn-cmd` 0.3.3. Preserve its current APIs so ACORN consumers can migrate separately.
2. Update Xylem's version and Git revision together. Published Xylem crates must resolve the same `acorn-cmd` contract from crates.io; a Git-only prerequisite is insufficient.
3. Land the Xylem context, workspace, scheduler, journal, and CLI changes as one coordinated runtime boundary. `xylem-lib/src/script/mod.rs`, run-state files, manifests, and `xylem-cli/src/commands/run/mod.rs` remain single-writer integration surfaces.
4. Move Kubernetes plan, journal, and summary changes to version 7 so this plan exclusively owns version 6.

This plan follows the typed workflow-outcome contract and preserves the existing `options.batch` rolling ceiling. Its durable outcomes feed the journal and reporters. Memory-aware scheduling remains a follow-on policy at the admission seam.

# Public configuration and CLI contract

- Add `options.failure_policy` with `fail_fast` and `continue` values. Omission means `fail_fast` for backward compatibility.
- Add `--failure-policy <fail-fast|continue>` as a CLI override. CLI spelling uses kebab case; serialized configuration uses snake case.
- Add positive `timeout_seconds` to each executable module. Omission means no timeout. Expose a lower-level library execution option so callers that do not deserialize a module can still supply a timeout.
- Add CLI-only `--retain-workspace`. Do not add retention to reusable workflow configuration because it is an operator diagnostic choice, not workflow behavior.
- Keep existing journal-path configuration or CLI selection. When no path is supplied, use `${TMPDIR}/xylem/journals/<run-id>.journal.jsonl`, separate from the workspace tree.
- Reject zero timeouts, unknown policies, and attempts to configure retained workspaces in workflow files.

# Context contracts

`RunContext` owns the run identifier, invocation directory, configuration source directory, workspace owner, journal path, resolved thread budget, batch ceiling, failure policy, cancellation token, and diagnostic retention choice.

`WorkflowContext` is immutable and input-specific. It owns the input index and value, resolved variables, input workspace paths, and references to immutable run-level policy. A new context is built even for a zero-input workflow and for each repeated input value.

`DispatchReport<T>` returns one ordered slot per configured input, plus whether admission stopped early and the cancellation reason. Completion order must never determine result order.

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

# Workspace and preparation

Use an RAII-owned layout:

```text
${TMPDIR}/xylem/workspaces/<run-id>/
  configuration/
  inputs/
    <ordered-input-index>/
      modules/
      environments/
      scratch/
```

Resolve remote configuration sources once under `configuration/`. Materialize every executable module separately for every input under that input's `modules/` directory, even when repeated work costs more. Correct isolation takes precedence over sharing clones, copied trees, or prepared environments in this phase.

For local modules, copy the tree into the input workspace. Canonicalize the source root and every symlink target, reject dangling or escaping links and special files, avoid cycles, and preserve regular-file executable permissions. Materialize accepted internal links as workspace-owned content so later command execution cannot escape through them.

Place Conda and Mamba prefixes under the input's `environments/` directory. Run Pixi against the copied input-local project and keep its environment state inside the same input boundary. Other supported managers must either demonstrate an input-local path or fail validation; they may not fall back to a shared named environment.

Remove the workspace after every terminal path, including success, failure, cancellation, and panic unwind. `--retain-workspace` transfers cleanup responsibility to the operator and prints the retained path. The journal is never removed with the workspace.

# Scheduling, failure, and cancellation

Use one admission ceiling equal to `min(--threads, options.batch)`. A slot covers acquisition, local copying, environment preparation, sequential module execution, and completion observation for one input. Do not create a separate unbounded preparation pool.

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

# Journal and schema version 6

Add `cancelled` to the durable input and module state vocabulary. Advance execution plan, journal, recovered summary, and their JSON schemas to version 6 together. Preserve readers and immutable schema files for versions 1 through 5; normalize version-5 records in memory without rewriting them.

Version 6 records the effective failure policy, timeout, run and input workspace identities without persisting disposable absolute paths as reusable configuration, ordered input disposition, cancellation reason, process termination metadata, duration, and output truncation counts. Never persist captured output beyond the journal's existing bounded diagnostic policy.

Dry-run validates and renders the effective policy, timeout, reserved variables, declared runtime paths, and scheduler ceiling. It creates no workspace or journal, performs no acquisition or environment preparation, and represents runtime-generated values such as run and work directory with stable declarations rather than random concrete paths.

# Implementation phases

1. Upgrade `acorn-cmd`, add Xylem result adaptation, and remove lossy parsing and output conversion.
2. Add configuration and CLI policy, pure variable resolution, owned path resolution, and immutable contexts.
3. Add the RAII workspace and per-input acquisition/preparation layout.
4. Move all preparation and execution behind the single bounded scheduler and implement fail-fast, continue, and external cancellation.
5. Advance plan, journal, recovery, summary, schemas, reporters, and dry-run to version 6.
6. Remove global environment mutation and obsolete helpers, then update documentation and examples.

# Validation

- Test precedence, reserved-name rejection, zero-input behavior, non-Unicode paths, duplicate inputs, configuration-relative paths, and invocation-relative CLI paths.
- Run conflicting variables through many concurrent inputs and prove each child observes only its own `_XYLEM_*` environment while the parent environment remains byte-for-byte unchanged.
- Measure peak work across acquisition, preparation, and execution; it must never exceed `min(--threads, options.batch)`.
- Test deterministic ordering, sequential modules, fail-fast admission, continue behavior, external cancellation, exit 130, module timeout, and process-tree cleanup.
- Test input-local local trees, Git modules, Conda/Mamba prefixes, Pixi projects, symlink containment, executable-bit preservation, duplicate inputs, and cleanup after success, failure, cancellation, and panic unwind.
- Verify `--retain-workspace` reports a usable path while normal runs remove the workspace and retain the journal.
- Round-trip version-6 plans, journals, and summaries; read version-5 fixtures; assert explicit `cancelled`, `failed`, and `not_started` states.
- Prove dry-run performs no filesystem writes, process spawn, network access, environment mutation, or random identifier generation.
- Run focused library and CLI tests on Linux, macOS, and Windows, then run the workspace lint and test gates.

# Acceptance criteria

- Production workflow execution never mutates the parent process environment.
- Every input, including a repeated or zero-input case, has a complete immutable context and isolated owned paths.
- Configuration and CLI precedence, reserved variables, failure policy, timeout, and path ownership are explicit and tested.
- One ceiling bounds the complete input lifecycle, modules remain sequential, and reports are ordered deterministically.
- Fail-fast, continue, timeout, and external cancellation have distinct durable outcomes; external cancellation exits 130.
- Xylem delegates process supervision to the published ACORN contract and preserves separate bounded stdout/stderr evidence.
- Workspaces clean up on every terminal path unless the CLI explicitly retains them; journals have an independent lifetime.
- Version-6 schemas represent cancellation and policy while version-5 artifacts remain readable.
- Dry-run creates no workspace or journal and performs no execution-side mutation.

# Related plans

- [ACORN process execution contract](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/process-execution-contract.md)
- [Memory-aware scheduling](../features/memory-aware-scheduling.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
