---
type: Plan
title: "Build a hermetic workflow test suite"
description: "Replace host-specific assertions with isolated fixtures and add regression coverage for process, workflow, security, and compiled CLI behavior."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli, workspace]
tags: [testing, hermetic, cli, process, fixtures, cross-platform]
status: planned
---

# Execution

Implement this plan in separately landable phases. The fixture foundation can
start immediately. Process, environment, acquisition, CLI, and CI assertions
wait for their owning contracts and consume those interfaces without redefining
them.

Baseline commit `c189a4d` passes
`cargo test --workspace --all-targets --locked` with 171 tests and no ignored
tests.

# Goal

Make the default test suite deterministic on supported developer and CI hosts, then use it to prove the new error, process, configuration, environment, acquisition, reporter, and exit-status contracts.

# Current evidence

- Most compiled-CLI workflow tests are still gated off on Windows because their fixtures depend on `sh`; batching, journal, ordering, resume, and managed-container coverage therefore has no Windows equivalent.
- Library tests still probe installed Git, Conda, and Mamba commands instead of controlled helpers.
- No schema validation ties JSON reports and execution plans to the checked-in schemas.
- Template tests mutate shared `_XYLEM_*` process variables without restoring them.
- Tests construct PID-named paths under the shared system temporary directory and remove them manually, so panic and concurrent-suite cleanup is unreliable.
- CLI workflow tests depend on the repository working directory and inherit most of the developer's environment.
- Process deadlock, output bounds, cancellation, non-UTF-8 output, remote acquisition, and parallel environment isolation remain uncovered.

# Project code boundaries

- **Update:** current tests in `xylem-lib/src/tests/`, `config/tests/mod.rs`, `script/tests/mod.rs`, and `xylem-cli/tests/`; workspace development dependencies; dependency-policy baselines; Make and CI test commands.
- **Create:** the unpublished `xylem-test-support` crate at `tests/support`, a portable child-process probe, isolated temporary workspaces, schema assertions, fake loopback services, and focused fixtures.
- **Delete:** checkout-specific assumptions, shared environment mutation, conditional assertions based on installed tools, broad Windows exclusions, manual temporary-directory cleanup, and tracked Python bytecode if any appears.

# Dependencies and ordering

The support crate and fixture migration take the manifest lock but do not need
the process, configuration, CLI, or delivery locks. Later phases follow the
accepted [`acorn-cmd` process execution](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/process-execution-contract.md),
[workflow environment](../refactors/workflow-environment-and-concurrency.md),
and [acquisition](../refactors/secure-module-acquisition.md) contracts. CI
wiring lands through [CI/release repair](../refactors/ci-release-pipeline.md).

Stop and reschedule a phase if it needs to change a production contract owned
by another plan.

# Implementation

## Phase 1: test-support foundation

- Add `tests/support` to the workspace as `xylem-test-support` with `publish = false`.
- Add `tempfile 3.27.0` and `jsonschema 0.58.4` as workspace development dependencies. Disable `jsonschema` default features because the checked-in draft 2020-12 schemas use only local references. Regenerate `Cargo.lock`, then update dependency-policy package and duplicate counts to the measured values.
- Provide a `TestWorkspace` backed by `TempDir`. Each instance owns `bin`, home, temporary, state, artifact, and report directories and removes them through RAII.
- Provide a compiled-CLI command builder that clears inherited variables, uses the test-local `PATH`, assigns home and temporary variables inside the workspace, sets `CI=1` and `NO_COLOR=1`, and retains only required Windows variables such as `SystemRoot`, `ComSpec`, and `PATHEXT`.
- Provide an RAII loopback server. Dropping it must close the listener, signal its worker, and join the thread so tests cannot leak ports or children.
- Provide schema helpers that compile and validate the current execution-plan and run-summary schemas. Each schema test must also reject a deliberately invalid instance so a disconnected validator cannot pass.

Build one dependency-free Rust probe into Cargo's `OUT_DIR` from the support
crate's build script using Cargo's `RUSTC`. The support library copies that
probe into a workspace under the requested executable name. It is never an
installed Xylem binary.

The probe supports deterministic modes for:

- success, a selected non-zero exit, stdout, and stderr;
- exact argument and environment recording as JSON Lines;
- cross-process concurrency accounting with atomic directory locks;
- large, interleaved, and non-UTF-8 output;
- hanging, descendant spawning, and cancellation cleanup;
- report-destination obstruction after discovering the journal run ID;
- Docker/Podman, `nvidia-smi`, Conda/Mamba, and fake Git command responses.

Verify this phase with `cargo test -p xylem-test-support --locked`.

## Phase 2: migrate existing tests

- Replace PID-named paths and explicit `remove_*` cleanup with `TestWorkspace`. Use absolute paths, and generate workflow configurations inside the workspace instead of relying on the checkout as the current directory.
- Replace `sh`, `echo`, `pwd`, Docker, and GPU shell fixtures with the portable probe. Keep POSIX scripts that remain part of public examples, but stop using them as cross-platform test infrastructure.
- Replace direct `_XYLEM_*` mutation with explicit variable maps such as `expand_arguments_from`. Tests may set variables on child commands; they may not change the parent process environment.
- Replace installed Conda/Mamba assertions with pure parser tests over controlled output. Move the installed-Git checkout test to the acquisition phase instead of conditionally passing when Git is absent.
- Move ad hoc HTTP listeners to the RAII loopback helper. A required loopback test fails if it cannot bind rather than returning early.
- Remove `cfg(not(target_os = "windows"))` from workflow tests after their fixtures are portable. Retain narrow target gates for Unix signal delivery, symlink semantics, and non-Unicode path construction.
- Keep independent helpers and tests in deterministic, case-insensitive lexical order within their subject modules.

Verify this phase with `cargo test -p xylem-lib --locked` and
`cargo test -p xylem-cli --test workflow --locked` on Linux, macOS, and
Windows.

## Phase 3: contract coverage

After the process contract lands, use the probe to test:

- exact argument preservation for empty values, spaces, quotes, escapes, and Unicode;
- concurrent draining when both output pipes exceed their capacity;
- separate stdout and stderr, configured byte/line bounds, and truncation metadata;
- silent success, non-zero status, missing executable, bad working directory, timeout, cancellation, signal termination, and non-UTF-8 bytes;
- termination and reaping of the complete owned child-process tree.

After the immutable workflow-context contract lands, run inputs with conflicting
values in parallel. Assert that every child receives only its own context, that
the parent environment is byte-for-byte unchanged, and that run workspaces are
removed after success, failure, unwind, and cancellation unless retention was
requested.

After the acquisition contract lands, use injected fake Git and loopback HTTP
transports for revisions, redirects, malformed and truncated responses,
offline cache hit/miss behavior, and dry-run zero-I/O behavior. Default tests
must not contact non-loopback addresses.

## Phase 4: compiled CLI and schema contracts

- Make tracing honor `NO_COLOR` under the CLI lock; do not add a test-only CLI flag.
- Cover no command, help, successful execution, unsupported/invalid/missing configuration, missing requirements, invalid allowlists, dry-run/report-output conflicts, preparation failure, process failure, reporter failure, and interruption.
- Assert status, stdout, and stderr separately. Diagnostics stay on stderr, machine-readable JSON stays on stdout, and neither stream contains ANSI escapes.
- Produce reporter failure deterministically by having the probe discover the active journal ID and create a directory at the future JSON report destination before rendering.
- Validate dry-run JSON against `execution-plan-v5.schema.json` and successful and failed reports against `run-summary-v5.schema.json`.
- Exercise cancellation through the common cancellation interface on every platform. Gate only operating-system signal delivery where the standard library cannot provide a portable trigger.

## Phase 5: commands and CI

- Make `cargo test --workspace --all-targets --locked` the canonical required test command. Run `cargo test --workspace --doc --locked` separately, and remove unconditional clean or rewrite steps from test targets.
- Run the suite once serially and once with normal or high test-thread concurrency. Put bounded timeouts around deadlock and cancellation cases.
- Require zero `#[ignore]` attributes in the default suite. External-tool integration tests belong in explicitly named opt-in jobs with an owner and documented dependency.
- Preserve test summaries as CI artifacts so test and ignored counts are visible per platform.
- Add required native Linux, macOS, and Windows workflow-test jobs through the CI/release plan. Use non-root accounts and run Cargo offline after dependencies are fetched. Portable tests do not by themselves change release-support claims.

# Validation

- Run focused support, library, and compiled-workflow tests after their owning phase.
- Run the full workspace suite and documentation tests on every native CI platform.
- Repeat with one and many test threads; use randomized order when the runner supports it.
- Exercise child CLIs with the minimal test-local `PATH`, a non-root account, no external network, and non-Unicode paths where the platform supports constructing them.
- Bound deadlock and cancellation tests and confirm that no child or descendant remains.
- Run dependency policy, audit, and feature-tree checks after adding the development dependencies.

# Acceptance criteria

- The default suite passes from a clean checkout as a non-root user without Conda, Docker, Git network access, or developer-specific paths.
- Tests do not leak environment variables, children, ports, files, or temporary directories.
- `xylem-cli` has compiled-binary coverage for success and every stable user-visible failure class.
- Process deadlock, argument quoting, output bounds, byte handling, cancellation, descendant cleanup, and non-zero status are regression-tested.
- Configuration, offline, dry-run, acquisition, concurrency, and reporter contracts have deterministic coverage.
- Emitted version-5 plans and reports validate against their checked-in schemas.
- The default suite has no ignored tests and no broad operating-system exclusions.
- Required CI runs the hermetic suite on every declared support tier.

# Related plans

- [ACORN process execution contract](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/process-execution-contract.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Secure module acquisition](../refactors/secure-module-acquisition.md)
- [CI and release pipeline](../refactors/ci-release-pipeline.md)
