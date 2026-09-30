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

Remaining hermetic fixture work coordinates with the open process and
environment plans only when it touches those runtime contracts.

# Goal

Make the default test suite deterministic on supported developer and CI hosts, then use it to prove the new error, process, configuration, environment, acquisition, reporter, and exit-status contracts.

# Current evidence

- Most compiled-CLI workflow tests are still gated off on Windows because their fixtures depend on `sh`; batching, journal, ordering, resume, and managed-container coverage therefore has no Windows equivalent.
- Library tests still probe installed Git, Conda, and Mamba commands instead of using controlled helpers.
- No schema validation ties JSON reports and execution plans to the checked-in schemas.
- Template tests mutate shared `_XYLEM_*` process variables without restoring them.
- Process deadlock, output bounds, cancellation, non-UTF-8 output, remote acquisition, and parallel environment isolation remain uncovered.

# Project code boundaries

- **Update:** current tests in `xylem-lib/src/tests/`, `config/tests/mod.rs`, and `script/tests/mod.rs`; fixture lookup; dev dependencies; Make/CI test commands.
- **Create:** workspace test-support helpers, isolated temporary directories, tiny helper executables/scripts, `xylem-cli/tests/` compiled-binary tests, fake HTTP/Git services, and focused fixtures.
- **Delete:** `/root` and checkout-specific assumptions, unscoped environment mutation, conditional assertions based on installed tools, tracked Python bytecode fixtures, and unused `assert_cmd` declarations if compiled CLI coverage is not adopted.

# Dependencies and ordering

Host-independent fixture and helper work can begin immediately. Final
assertions follow their owning contracts: typed outcomes, [process
execution](../refactors/process-execution-contract.md), configuration,
environment, acquisition, and reporters. The green suite is required by
[CI/release repair](../refactors/ci-release-pipeline.md).

# Recommendation

## Testing conventions

- Every test creates its own temporary directory and cleans it through RAII.
- Environment changes use a scoped helper and are serialized only when unavoidable; production execution should instead accept an explicit context.
- External tools are represented by injected capability resolvers or helper commands placed on a test-local `PATH`.
- Network behavior uses loopback fake servers and explicit offline defaults.
- Compiled CLI tests assert exit status, stdout, and stderr separately with ANSI disabled.
- Platform-specific tests gate only the assertion that genuinely differs; core behavior remains common.

## Remaining coverage

### Contract tests

Add child-process stress tests, variable isolation, and acquisition fakes as their production contracts land.

### Compiled CLI and platform matrix

Cover help, no command, missing requirements, reporter write failure, and interrupt/cancellation. Replace shell-only fixtures so the existing workflow cases run on declared Linux, macOS, and Windows tiers.

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
- [CI and release pipeline](../refactors/ci-release-pipeline.md)
