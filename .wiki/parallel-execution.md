---
okf_version: "0.1"
---

# Parallel execution guide

This guide overlays write ownership on the [plan dependency network](plan-network.md). Plans in different lanes are concurrent only when their declared files are disjoint and they avoid the listed integration locks.

## Integration locks

| Lock | Files | Owner rule |
|---|---|---|
| Manifest lock | `Cargo.toml`, `Cargo.lock`, `xylem-*/Cargo.toml`, `deny.toml` | One remaining migration or dependency-gate owner at a time. |
| Configuration lock | `xylem-lib/src/config/mod.rs` and its future extracted modules | Preserve the landed typed-error, validation, `options.batch`, and resume contracts while acquisition or memory work extends them. |
| Process lock | `xylem-lib/src/script/mod.rs` and process fixtures | Build the process contract on the landed contextual errors before environment or lifecycle integration. |
| Scheduler lock | `xylem-lib/src/execution.rs`, input admission in `xylem-cli/src/commands/run/mod.rs` | Preserve the landed fixed-batching and resume boundary; environment and memory work consume it serially rather than adding parallel admission loops. |
| Sandbox lock | `xylem-lib/src/sandbox/`, sandbox policy and conformance fixtures | Sandboxed execution owns policy resolution, capability negotiation, session lifecycle, and provider adapters; input discovery and managed containers consume those contracts. |
| Run-state lock | `xylem-lib/src/progress.rs`, journal/outcome modules, reporter adapters | Preserve the landed journal, reporter, and resume vocabulary without redefining durable states. |
| CLI lock | `xylem-cli/src/{main,cli}.rs`, `xylem-cli/src/commands/run/mod.rs` | One owner for argument registration, pool/scheduler setup, journal lifecycle, CLI-owned exit mapping, and generated help. |
| Delivery lock | `Makefile`, `.gitlab-ci.yml`, release exporters | Security commands land before CI/release wiring; package export consumes the immutable release contract. |
| Documentation lock | top-level Markdown and `examples/` | Final updates occur after behavior and command names are stable. |

## Work lanes

The typed-error, validated-planning, reporter, and resumable-execution
contracts have landed.
The lanes below contain only remaining work.

### Lane A: dependency migrations

1. [ACORN 0.3.1 upgrade](migrations/acorn-0-3-1-upgrade.md), including removal of the unused `acorn-lib` analysis dependency
2. [Dependency and security gates](refactors/dependency-surface-and-security-gates.md)

The remaining ACORN work owns the manifest lock independently of the completed parser and completion phases.

### Lane B: configuration and acquisition

1. [Secure module acquisition](refactors/secure-module-acquisition.md), consuming the landed validation contract
2. [Configuration modules](refactors/configuration-modules.md), consuming landed planning, resume, reporter, and batching contracts while coordinating remote source acquisition

Configuration extraction and validated execution plans are landed contracts. The
configuration-modules plan owns recursive source resolution and flattened leaf
identity; secure acquisition retains ownership of generic transport behavior.

### Lane C: execution runtime

1. [Process execution contract](refactors/process-execution-contract.md)
2. [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md), after the process contract lands
3. [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md), after process and immutable environment contracts land
4. [Shell-script workflow input](features/shell-script-workflow-input.md), consuming the sandbox discovery boundary
5. [Managed container lifecycle](refactors/managed-container-lifecycle.md), consuming a backend-advertised private runtime for sandboxed runs
6. [Managed-container keepalive](features/container-keep-alive.md), consuming the managed lifecycle startup and readiness boundary
7. [Memory-aware scheduling](features/memory-aware-scheduling.md), after its spike and the fixed scheduler contract

The landed `options.batch` rolling input ceiling is the baseline these steps must preserve, along with the peak-concurrency tests in `xylem-lib/src/execution.rs`. Advanced orchestration preserves both limits and consumes the process result contract. Shell input and managed containers may prototype against the fake provider, but their integration waits for the sandbox lock and cannot add direct host process or host runtime-socket paths.

### Lane D: CLI and verification

1. [Hermetic workflow tests](testing/hermetic-workflow-tests.md), fixture/helper slice only

Typed error-path and resume close-out coverage has landed. Remaining fixture
cleanup and fake-command helpers can proceed without the CLI lock until they
need a new runtime contract.

### Lane E: delivery and communication

1. CI/release analysis and isolated script prototypes
2. [CI and release pipeline](refactors/ci-release-pipeline.md) after required checks exist
3. [Homebrew release export](features/homebrew-export.md) after immutable release artifacts and checksums exist
4. [Documentation and examples](refactors/documentation-and-examples.md) after public behavior and installation commands stabilize

## Safe initial batch

The following slices have disjoint primary write sets:

| Slice | Primary ownership | Restriction |
|---|---|---|
| ACORN compatibility inventory | temporary diagnostics and `migrations/acorn-0-3-1-upgrade.md` notes | Do not edit manifests until holding the manifest lock. |
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
