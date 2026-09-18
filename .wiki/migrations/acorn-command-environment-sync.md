---
type: Plan
title: "Synchronize Xylem with ACORN command updates"
description: "Coordinate ACORN command API changes and Xylem adoption in parallel, then pin both workspaces to one verified upstream commit."
difficulty: medium
impact: high
package: [workspace, xylem-lib]
tags: [migration, acorn, process, environment, dependencies, coordination]
status: in-progress
priority: next
---

# Goal

Let ACORN and Xylem evolve the command-execution contract concurrently without committing temporary path dependencies, losing per-process environment isolation, or leaving Xylem on a hybrid execution path after the upstream API lands.

# Current evidence

- Xylem pins `acorn-core` and `acorn-lib` to ACORN revision `a8f474b46d9de89e6d5c902f5768cae3483d8a4a` and enables `acorn-lib`'s `analysis` feature, which includes `cmd`.
- `Script::run_checked` uses `cmd!(binary, arguments; dir: working_dir, env: environment)` for every invocation, including environment-bearing module commands.
- Xylem's workflow tests already exercise namespaced child variables and failure propagation, and the Pixi example exercises environment creation plus module execution.
- The pinned ACORN revision provides child-only `Command::envs` semantics without mutating the parent process.
- The current integration demonstrates the handoff pattern: develop and validate both worktrees in parallel, then advance Xylem only after the ACORN commit is reachable from the authoritative remote.

# Project code boundaries

- **Update:** workspace `Cargo.toml`, `Cargo.lock`, `xylem-lib/src/script/mod.rs`, focused command tests, and cross-repository handoff notes.
- **Create:** temporary local validation configuration only when needed; keep it untracked and remove it before handoff.
- **Delete:** the environment-dependent `std::process::Command` fallback and its explanatory compatibility comment after the ACORN pin advances.

# Dependencies and ordering

This plan follows the base [ACORN 0.2 upgrade](acorn-0-2-upgrade.md) and coordinates with [process execution](../refactors/process-execution-contract.md) and [workflow environment isolation](../refactors/workflow-environment-and-concurrency.md). ACORN owns the upstream macro contract; Xylem owns workflow semantics. Manifest and lockfile changes wait for a reviewed ACORN commit and hold the manifest lock.

# Parallel work lanes

## ACORN lane

1. Add the direct command `env:` modifier without changing existing macro forms.
2. Apply variables with `Command::envs`, preserving inherited variables and never mutating the parent process.
3. Test child visibility, values containing spaces, parent isolation, and combined `dir:` plus `env:` behavior on supported platforms.
4. Land the change and publish its immutable commit SHA with the focused and workspace validation results.

## Xylem lane

1. Keep the compatibility fallback operational while the pinned ACORN revision lacks `env:` support.
2. Maintain integration coverage for namespaced variables, recursive interpolation, module ordering, child failure propagation, and Pixi execution.
3. Validate the proposed one-line macro path against `../acorn` using an uncommitted path override or Cargo patch; do not commit a local filesystem reference.
4. Record any upstream API or feature discrepancy immediately so ACORN can adjust before its commit is finalized.

## Integration handoff

1. Pin both `acorn-core` and `acorn-lib` to the same reviewed ACORN commit and regenerate the lockfile.
2. Replace the conditional execution branch with `cmd!(command, args![..arguments]; dir: current_dir, env: envs)`.
3. Remove only the now-obsolete fallback code and compatibility comment; retain Xylem's output and failure mapping.
4. Re-run both repositories' required checks and record the ACORN SHA, Xylem commit, feature graph, and validation results together.

# Validation

- In ACORN, run the command-macro tests, formatting check, Clippy for `acorn-lib`, and the workspace test target required by its contributor guide.
- In Xylem, run formatting, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` against the new pin.
- Run the environment-bearing CLI fixture and confirm variables reach only the intended child process.
- Run the documented Pixi example from the repository root and confirm both `pixi install` and the module command use the ACORN path successfully.
- Inspect `cargo tree -e features -p xylem-lib` and confirm the pin does not broaden the intended ACORN feature set.
- Verify no path override, process-global environment mutation, fallback branch, or generated cache is present in the final diff.

# Acceptance criteria

- ACORN exposes and tests the direct-command `env:` modifier with child-only semantics.
- Xylem pins a durable ACORN revision containing that API and uses `cmd!` for environment-bearing module execution.
- `acorn-core` and `acorn-lib` resolve from the same upstream revision.
- Existing Xylem environment, failure, and Pixi scenarios pass without a `std::process::Command` execution fallback.
- Both repositories retain clean, independently reviewable diffs with their unrelated worktree changes untouched.
- The handoff records enough commit and validation evidence to repeat the same synchronized-update process for later ACORN changes.

# Related plans

- [ACORN 0.2 dependency upgrade](acorn-0-2-upgrade.md)
- [Process execution contract](../refactors/process-execution-contract.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
