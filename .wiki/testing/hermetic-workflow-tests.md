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

The prioritized item is [resumable execution](../features/resumable-execution.md), which holds the configuration, process, run-state, and CLI locks. Only host-independent fixture and helper work may proceed alongside it, and only in files that plan does not touch. Assertions that depend on the error, validation, or completion contracts land after the phase that introduces them.

# Goal

Make the default test suite deterministic on supported developer and CI hosts, then use it to prove the new error, process, configuration, environment, acquisition, reporter, and exit-status contracts.

# Current evidence

Rechecked against `005aa73`. The host-specific defects are fixed; the coverage the plan set out to add is not there.

- Landed: the suite is green. `cargo test --workspace --all-targets` passes 68 tests with no failures, replacing the 2-of-24 failure this plan was written against.
- Landed: the hard-coded developer paths are gone. `test_get_children` reads `../tests/fixtures/input` and asserts the sorted `one, three, two`; `test_script_run` uses `std::env::temp_dir()` instead of `/root` and is `cfg`-gated for Windows; `test_command` resolves `std::env::current_exe()` instead of assuming `conda` implies `python` on `PATH`. No `/root` reference remains in any test.
- Open: ten of the fourteen tests in `xylem-cli/tests/workflow.rs` are behind `#[cfg(not(target_os = "windows"))]`, so a Windows developer or runner executes only four. The gated set is where the durable-journal, batching, ordering, and module-ordering assertions live.
- Open: `xylem-lib/src/tests/mod.rs` still contains host-probing tests such as the Git clone case, and several tests shell out to `echo` and `sh` fixtures that are skipped rather than replaced on Windows.
- Open: there are no tests for the error surface, and no compiled-binary test asserts a specific exit code with stable diagnostic text.
- Open: the fixture modules under `tests/e2e/modules/` are `sh` scripts, so the compiled-CLI path has no Windows equivalent.
- Open: no snapshot or schema validation ties the JSON summary to `docs/schema/run-summary-v1.schema.json`.
- Template tests call `std::env::set_var` for shared `_XYLEM_*` names and do not restore them.
- Tests conditionally call installed `conda`, `mamba`, `cargo`, `ls`, `pwd`, and shell commands instead of injecting capabilities or using controlled helper binaries.
- `xylem-cli` has zero unit tests and no integration-test files despite declaring `assert_cmd`.
- Existing fixtures live outside the owning crate and are referenced through working-directory-sensitive `../tests/fixtures` constants.
- High-risk paths—non-zero exit, stderr saturation, invalid configuration, remote acquisition, offline policy, parallel input isolation, dry-run mutation, and CLI exit status—have no coverage.

# Project code boundaries

- **Update:** current tests in `xylem-lib/src/tests.rs`, `config/tests/mod.rs`, and `script/tests/mod.rs`; fixture lookup; dev dependencies; Make/CI test commands.
- **Create:** workspace test-support helpers, isolated temporary directories, tiny helper executables/scripts, `xylem-cli/tests/` compiled-binary tests, fake HTTP/Git services, and focused fixtures.
- **Delete:** `/root` and checkout-specific assumptions, unscoped environment mutation, conditional assertions based on installed tools, tracked Python bytecode fixtures, and unused `assert_cmd` declarations if compiled CLI coverage is not adopted.

# Dependencies and ordering

Host-independent fixture and helper work can begin immediately. Final assertions follow their owning contracts: [typed outcomes](../refactors/typed-error-and-outcome-boundary.md), [process execution](../refactors/process-execution-contract.md), configuration, environment, acquisition, and reporters. The green suite is required by [CI/release repair](../refactors/ci-release-pipeline.md).

# Recommendation

## Testing conventions

- Every test creates its own temporary directory and cleans it through RAII.
- Environment changes use a scoped helper and are serialized only when unavoidable; production execution should instead accept an explicit context.
- External tools are represented by injected capability resolvers or helper commands placed on a test-local `PATH`.
- Network behavior uses loopback fake servers and explicit offline defaults.
- Compiled CLI tests assert exit status, stdout, and stderr separately with ANSI disabled.
- Platform-specific tests gate only the assertion that genuinely differs; core behavior remains common.

## Coverage phases

## Phase 1: make the baseline green

Replace `/root`, checkout-layout, Conda/Python, and global-environment assumptions. Centralize fixture resolution from `CARGO_MANIFEST_DIR` or an explicit workspace helper.

## Phase 2: contract tests

Add child-process stress tests, parse/validate/plan tables, variable isolation, deterministic concurrency, offline/dry-run spies, and acquisition fakes as their production contracts land.

## Phase 3: compiled CLI and platform matrix

Cover help, no command, valid dry-run, invalid config, missing requirement, child non-zero exit, partial parallel failure, reporter selection/write failure, and interrupt/cancellation. Run on declared Linux, macOS, and Windows tiers.

# Validation

- Run the suite repeatedly with different test thread counts and randomized execution order where supported.
- Run with an empty/minimal `PATH`, non-root user, non-UTF-8-capable path fixtures where supported, and no network.
- Add a timeout around deadlock/cancellation tests and verify no child remains after completion.
- Run `cargo test --workspace --all-targets --locked` and documentation tests on every supported platform.
- Track test count and ignored tests; require a named external dependency and owner for any ignore.

# Acceptance criteria

- The default suite passes from a clean checkout as a non-root user without Conda, Docker, Git network access, or developer-specific paths.
- Tests do not leak environment variables, children, ports, files, or temporary directories.
- `xylem-cli` has compiled-binary coverage for success and every stable failure class.
- Process deadlock, argument quoting, output bounds, cancellation, and non-zero status are regression-tested.
- Configuration, offline, dry-run, acquisition, concurrency, and reporter contracts have deterministic coverage.
- Required CI runs the hermetic suite on every declared support tier.

# Related plans

- [Process execution contract](../refactors/process-execution-contract.md)
- [Configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md)
- [CI and release pipeline](../refactors/ci-release-pipeline.md)
