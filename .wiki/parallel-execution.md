---
okf_version: "0.1"
---

# Parallel execution guide

This guide overlays write ownership on the [plan dependency network](plan-network.md). Plans in different lanes are concurrent only when their declared files are disjoint and they avoid the listed integration locks.

## Integration locks

| Lock | Files | Owner rule |
|---|---|---|
| Manifest lock | `Cargo.toml`, `Cargo.lock`, `xylem-*/Cargo.toml`, `deny.toml` | One migration or dependency-gate owner at a time; ACORN command sync may claim it only after the upstream commit is fixed. |
| Configuration lock | `xylem-lib/src/config/mod.rs` and its future extracted modules | Error signatures land first; thread support scopes existing Rayon calls; validation then owns extraction. |
| Process lock | `xylem-lib/src/script/mod.rs` and process fixtures | The process contract lands before environment or lifecycle integration; ACORN command sync changes its adapter only during final handoff. |
| CLI lock | `xylem-cli/src/{main,cli}.rs`, `xylem-cli/src/commands/run/mod.rs` | One owner for argument registration, thread-pool setup, exit mapping, and generated help. |
| Delivery lock | `Makefile`, `.gitlab-ci.yml`, release exporters | Security commands land before CI/release wiring; package export consumes the immutable release contract. |
| Documentation lock | top-level Markdown and `examples/` | Final updates occur after behavior and command names are stable. |

## Work lanes

### Lane A: dependency migrations

1. [ACORN 0.2 upgrade](migrations/acorn-0-2-upgrade.md)
2. [ACORN command environment sync](migrations/acorn-command-environment-sync.md), with upstream API work and downstream fixtures proceeding in parallel while the manifest pin waits
3. [Maintained YAML parser](migrations/maintained-yaml-parser.md)
4. [Dependency and security gates](refactors/dependency-surface-and-security-gates.md)

ACORN and YAML investigation can run concurrently, but their manifest and lockfile edits are serialized.

### Lane B: configuration and acquisition

1. [Typed errors and outcomes](refactors/typed-error-and-outcome-boundary.md)
2. [Configuration validation and dry-run](refactors/configuration-validation-and-dry-run.md)
3. [Secure module acquisition](refactors/secure-module-acquisition.md)

This lane owns configuration extraction and validated execution plans.

### Lane C: execution runtime

1. [Typed errors and outcomes](refactors/typed-error-and-outcome-boundary.md)
2. [Configurable thread support](features/configurable-thread-support.md) and [process execution contract](refactors/process-execution-contract.md), in either order while respecting the CLI and process locks
3. [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md), after both step 2 contracts land

Thread support and process execution have different primary implementation boundaries after typed errors, but their CLI integration must be serialized. Advanced orchestration consumes both contracts.

### Lane D: CLI and verification

1. [Hermetic workflow tests](testing/hermetic-workflow-tests.md), fixture/helper slice only
2. [Run reporters](features/run-reporters.md)
3. Final error-path and compiled-binary test slice

Tests that assert new outcomes wait for the owning contract. Fixture cleanup and fake-command helpers can start without the CLI lock.

### Lane E: delivery and communication

1. CI/release analysis and isolated script prototypes
2. [CI and release pipeline](refactors/ci-release-pipeline.md) after required checks exist
3. [Homebrew release export](features/homebrew-export.md) after immutable release artifacts and checksums exist
4. [Documentation and examples](refactors/documentation-and-examples.md) after public behavior and installation commands stabilize

## Safe initial batch

The following slices have disjoint primary write sets:

| Slice | Primary ownership | Restriction |
|---|---|---|
| ACORN compatibility inventory | temporary diagnostics and `migrations/acorn-0-2-upgrade.md` notes | Do not edit manifests until holding the manifest lock. |
| Error taxonomy | new `xylem-lib/src/error.rs` and focused tests | Do not change existing public signatures until the configuration/process owners are ready. |
| Hermetic helper preparation | new test-support module and new fixtures | Do not edit CLI roots or rewrite assertions owned by later contracts. |
| CI rule validation | isolated CI lint inputs and plan notes | Do not edit `Makefile` or `.gitlab-ci.yml` until dependency-gate commands are accepted. |

## Agent start checklist

- Name the plan and atomic slice.
- List exact files to update, create, and delete.
- Claim required locks.
- List public contracts changed.
- Record baseline commit and prerequisite landing commit.
- Name the focused validation command.
- Stop and reschedule if the slice expands into another lane's lock.
