---
type: Plan
title: "Introduce typed errors and workflow outcomes"
description: "Replace library exits, panic placeholders, and log-only failures with composable errors and structured workflow results."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, errors, outcomes, exit-status, reliability]
status: planned
priority: next
---

# Goal

Make every reusable Xylem operation return success or a classified failure to its caller. Restrict process exit-code selection and terminal rendering to `xylem-cli`, and give workflow execution a structured outcome that later reporters and concurrency logic can aggregate.

# Current evidence

- `xylem-lib/src/config/mod.rs` calls `std::process::exit` from configuration loading, requirement checks, module preparation, and workflow execution.
- The library reaches `unimplemented!` for a serialized shell input, configuration modules, several module languages, unknown extensions and schemes, and remote generic scripts.
- `Script::run` returns `Vec<String>`; spawn failures, non-zero exit status, pipe errors, and wait errors all become an empty vector.
- Module preparation and execution discard `Script::run` results. `xylem-cli::commands::run` returns `Ok(())` after logged errors and has no aggregate failure model.
- `PyProjectToml::read` converts file-read failures into parsing `{}`, losing I/O context.
- A source scan found 60 production `process::exit`, `unimplemented!`, `unwrap`, or `expect` sites across `config/mod.rs`, `script/mod.rs`, `lib.rs`, and the CLI run path.

# Project code boundaries

- **Update:** public fallible functions in `xylem-lib/src/{lib,config/mod,script/mod}.rs`; CLI dispatch and exit mapping in `xylem-cli/src/{main,commands/run/mod}.rs`; focused tests and docs.
- **Create:** `xylem-lib/src/error.rs`, structured workflow/module outcome types in an appropriate owned module, and CLI exit-code mapping tests.
- **Delete:** library `process::exit` calls, reachable `unimplemented!` branches for accepted input, the unused `CommandResult` alias if superseded, and log-only fallback branches that pretend failure is success.

# Dependencies and ordering

Apply after the [ACORN](../migrations/acorn-0-2-upgrade.md) and [YAML](../migrations/maintained-yaml-parser.md) error surfaces are known. This plan precedes process execution, configuration validation, secure acquisition, concurrency, and reporters.

# Recommendation

## Error taxonomy

Define capability-owned errors rather than one stringly catch-all:

| Owner | Example classifications |
|---|---|
| Configuration | read, syntax, unsupported extension, semantic validation, unsupported declared capability |
| Planning/acquisition | invalid location, offline denial, unpinned source, download, checksum, materialization |
| Process | spawn, I/O capture, timeout, non-zero exit, signal termination, invalid working directory |
| Workflow | preparation failure, module failure, cancellation, partial multi-input failure |

Errors retain source chains and relevant paths/module names. They do not contain terminal color policy or call `tracing` as a substitute for returning failure.

## Workflow outcome contract

Introduce a result model that records input identity, ordered module outcomes, command status, bounded output references, duration, and dry-run state. A workflow is successful only when every required stage succeeds. Partial parallel failure remains inspectable and produces a non-zero CLI exit.

Keep `color-eyre` at the executable boundary for presentation and context. Prefer an owned error derive such as `thiserror` for library contracts after checking MSRV and dependency policy.

## Migration sequence

1. Add owned errors and conversions without changing behavior.
2. Convert configuration read and semantic failures from exit/panic to `Result`.
3. Convert `Script::run` through the [process execution plan](process-execution-contract.md).
4. Convert preparation and workflow iteration to collect and propagate results.
5. Map top-level classifications to stable CLI exit codes and render one diagnostic chain.
6. Deny `process::exit` outside the binary boundary and add Clippy lints for production `unwrap`, `expect`, and panic paths.

# Validation

- Unit-test every error classification, display message, source chain, and relevant context field.
- Add compiled-binary tests for invalid configuration, missing requirements, invalid module location, spawn failure, non-zero module exit, and partial multi-input failure.
- Assert exit status and stable diagnostic content without relying on ANSI color or absolute host paths.
- Run `cargo clippy --workspace --all-targets` with the selected panic/unwrap lints and the full test suite.

# Acceptance criteria

- `xylem-lib` contains no `std::process::exit` and no reachable `unimplemented!` for deserializable public configuration.
- Fallible library APIs return owned, contextual errors.
- Child, module, and input failures cannot be represented as an empty successful result.
- The CLI returns non-zero for any required workflow failure and owns the exit-code mapping.
- Multi-input outcomes preserve every completed failure deterministically.
- Reporter work can consume structured outcomes without scraping logs.

# Related plans

- [Process execution contract](process-execution-contract.md)
- [Configuration validation and dry-run](configuration-validation-and-dry-run.md)
- [Structured run reporters](../features/run-reporters.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
