---
type: Plan
title: "Implement structured run reporters"
description: "Make the existing reporter and output options emit deterministic console, stdout, and JSON summaries from structured workflow outcomes."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, reporter, json, cli, output, summary]
status: complete
---

# Outcome

Completed. Console and JSON reporters render the recovered versioned summary,
and the unused function-pointer reporter, `Output`, and `Details` scaffolding
has been removed. This document is a historical implementation record.

# Goal

Complete the public `--reporter` and `--output` contract so users and automation can consume stable workflow results without parsing tracing logs.

# Current evidence

- The CLI advertises `--reporter` and `--output`, but `commands::run::run` names both parameters with leading underscores and never reads them.
- `ReporterType`, `Reporter`, `Details`, `Parameters`, and `Output` exist in `xylem-lib`, but the run path constructs none of them.
- The help text says console is the default and JSON is available, while `ReporterType` also contains `Stdout`; accepted CLI values are untyped free-form strings.
- The current execution path discards module results and returns `Ok(())`, so a reporter has no reliable success/failure input.
- `Details::PartialEq` ignores its `error` field while its derived ordering includes all fields, which can violate intuitive equality/ordering expectations for deduplication and reporting.
- The configuration `output` field and CLI `--output` destination have no documented precedence or file/directory semantics.

# Project code boundaries

- **Update:** reporter types in `xylem-lib/src/lib.rs`, CLI arguments in `xylem-cli/src/cli.rs`, run orchestration, documentation, and tests.
- **Create:** reporter modules for console and versioned JSON output, atomic output-file handling, schemas/snapshots, and compiled-CLI tests.
- **Delete:** unused function-pointer reporter scaffolding if the typed reporter interface supersedes it, untyped reporter strings, and inconsistent manual equality if it is not part of a deliberate identity contract.

# Dependencies and ordering

Consumes [typed workflow outcomes](../refactors/typed-error-and-outcome-boundary.md), [process results](../refactors/process-execution-contract.md), [environment/concurrency aggregation](../refactors/workflow-environment-and-concurrency.md), the existing durable execution journal, and validated dry-run plans. The journal owns persistent transitions; reporters own presentation and final export.

# Recommendation

- Parse reporter names with Clap `ValueEnum` and define whether multiple reporters are allowed, deduplicated, and ordered.
- Use console as the human default. Define `stdout` as a machine-oriented stream only if it differs materially from JSON; otherwise remove the ambiguous variant.
- Give JSON a versioned schema containing run ID, configuration identity, dry-run flag, ordered input/module outcomes, status, duration, bounded output metadata, error classifications, and acquisition provenance.
- Keep tracing diagnostics on stderr and reporter data on stdout or the named file so pipelines can separate them.
- Define CLI/config output precedence. Treat `--output` as a file for single-document reporters or require an explicit directory convention for multiple outputs.
- Write files atomically and refuse accidental overwrite unless the CLI contract includes `--force`.
- Map overall outcome to exit status independently of reporter write success, while making reporter write failure itself non-zero.

# Validation

- Snapshot console output with colors disabled and JSON output with nondeterministic fields injected or normalized.
- Test reporter selection, duplicates, invalid names, stdout/stderr separation, output precedence, existing destinations, write failure, atomicity, and broken pipes.
- Test success, dry-run, silent child, non-zero child, partial multi-input failure, acquisition failure, and cancellation.
- Validate serialized JSON against its checked-in schema and test backward compatibility for the supported version.

# Acceptance criteria

- Every advertised reporter is implemented and selected through typed CLI values.
- Console is readable, JSON is stable/versioned, and tracing never corrupts machine output.
- `--output` has documented precedence and safe atomic-write behavior.
- Overall success and exit status derive from structured outcomes, not log messages.
- Reporter failures are visible and non-zero.
- Equality, ordering, and deduplication semantics for result details are consistent and tested.

# Related plans

- [Typed errors and outcomes](../refactors/typed-error-and-outcome-boundary.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Documentation and examples](../refactors/documentation-and-examples.md)
