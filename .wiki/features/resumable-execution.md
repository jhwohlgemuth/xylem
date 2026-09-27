---
type: Plan
title: "Finish typed execution foundations and resumable execution"
description: "Complete the remaining typed-error hardening, resume edge-case coverage, and release documentation."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, resume, output, completion, fingerprint, idempotency, foundation]
status: in-progress
priority: next
---

# Goal

Finish the remaining hardening around the validated execution plan and explicit
resume behavior. Completed phases have been removed from this plan so it lists
only work that still needs an owner.

# Landed contracts

- JSON and YAML use the maintained parser contract with strict unknown-field handling.
- Configuration follows parse, validate, plan, and execute stages. Validation
  returns path-qualified multi-error diagnostics before host mutation.
- Plans resolve deterministic inputs, CLI precedence, artifact roots, effective
  variables, acquisition intent, inline command arguments, and completion paths.
- Dry-run is write-free and serializes the deterministic versioned plan.
- Top-level `output` supplies the default `OUTPUT_DIRECTORY`;
  `--artifact-output` overrides configured artifact destinations, while
  `--report-output` owns journal and report placement.
- Completion declarations use explicit markers and sorted output globs. Empty,
  unresolved, invalid, colliding, and outside-root declarations are rejected
  through the planning diagnostic path.
- Canonical completion identity changes with the module, template, variables,
  or declaration. Completion requires every pattern to match non-empty files.
- Atomic markers, `skipped_existing`, journal schema version 2, progress and
  reporter vocabulary, `options.resume`, and `--resume` are implemented.
- A completed module is excluded from preparation and is not spawned during an
  explicit resume run.

# Remaining work

## Phase 2: Finish typed errors and workflow outcomes

Consolidate `ConfigReadError`, `JournalError`, completion, planning,
acquisition, process, and workflow failures under capability-owned library
error types with source chains and relevant path or module context. Remove
remaining production panic, `unwrap`, and `expect` paths where failure is
possible. Keep process exit mapping and terminal presentation at the CLI
boundary.

Add lint enforcement that denies library `process::exit` and production panic
shortcuts while allowing test assertions. Cover every classification's display
text, source chain, and context fields, plus compiled-binary cases for invalid
configuration, missing requirements, invalid module location, spawn failure,
non-zero exit, and partial multi-input failure.

## Phase 7: Close resume edge cases

Add the multi-module integration fixture in which an early module is complete
and a later module is incomplete, proving only the early module is skipped.
Add deterministic duplicated-input and non-Unicode directory-entry coverage,
and log the planned reason when a module has no completion declaration.

## Phase 9: Reconcile release documentation

Document the final error taxonomy after Phase 2 lands. Reconcile the feature
index, plan network, parallel execution order, and absorbed-plan statuses so
none describe already completed phases as schedulable work. Exercise the
representative imagery workflow at its actual top-level input granularity.

# Remaining validation

- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo audit` and `cargo deny check`
- Compiled-CLI proof for the early-complete/later-incomplete resume case
- Repository diff audit confirming no completed phase remains actionable

# Remaining acceptance criteria

- Every fallible reusable API returns an owned contextual error and no
  deserializable public configuration can reach a panic or process exit.
- The early-complete/later-incomplete case skips only completed work and never
  prepares or spawns that module.
- Documentation and planning indexes describe only the current contracts and
  remaining work.

# Related plans

- [Typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md)
- [Process execution contract](../refactors/process-execution-contract.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
- [Secure module acquisition](../refactors/secure-module-acquisition.md)
