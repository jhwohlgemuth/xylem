---
type: Plan
title: "Finish typed execution foundations and resumable execution"
description: "Completed typed-error hardening, resume edge-case coverage, and release documentation."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, resume, output, completion, fingerprint, idempotency, foundation]
status: complete
---

# Outcome

Completed the typed execution foundation and explicit resume behavior. This is
a historical implementation record, not a schedulable work item.

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

# Close-out evidence

- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo deny check` completes with the repository's existing warning set.
- `cargo audit` was executed and reports transitive `quick-xml`, `quinn-proto`,
  and `rsa` advisories. Those dependency updates remain owned by the dependency
  surface/security-gates plan and ACORN feature-narrowing work.
- Compiled-CLI coverage proves invalid configuration and location handling,
  missing requirements, spawn and non-zero failures, deterministic partial
  multi-input failure, and the early-complete/later-incomplete resume case.
- Unit coverage verifies error display text, source chains, contextual fields,
  duplicate elimination, and Linux non-Unicode directory-entry rejection.
- Production library builds deny process exit, panic, `unwrap`, and `expect`.
- A representative imagery dry run plans each provided test-case directory as
  one top-level resume and batching unit.

# Landed acceptance criteria

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
