---
type: Plan
title: "Add fixed input concurrency configuration"
description: "Add a positive batch option that independently caps active top-level workflow inputs while preserving the existing Rayon worker budget."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, configuration, batch, concurrency, scheduler, rayon]
status: planned
priority: next
---

# Goal

Let memory-sensitive workflows set `options.batch` to the maximum number of top-level Xylem inputs that may execute at once. Define this value as a rolling concurrency ceiling rather than a strict wave barrier, and keep it distinct from the existing global Rayon worker limit.

# Current evidence

- `ConfigOptions` has no input-concurrency field.
- `xylem-cli::commands::run::run` eagerly collects configured or CLI inputs and submits all of them through `inputs.par_iter().for_each`.
- `-t`/`--threads` builds one run-local Rayon pool and limits all Xylem-owned Rayon work, including module preparation. It is not a configuration-level contract for active workflow inputs.
- Directory input discovery returns first-level child paths. If a module invocation loops over several image pairs internally, Xylem sees one directory input rather than those individual pairs.
- `fs::read_dir` order is not stable, so the current input order is unsuitable for deterministic scheduling and later resume reports.

# Project code boundaries

- **Update:** `xylem-lib/src/config/mod.rs`, `xylem-lib/src/execution.rs`, `xylem-cli/src/commands/run/mod.rs`, configuration and CLI documentation, representative imagery configuration, and focused tests.
- **Create:** a validated batch/concurrency value if an owned type gives clearer parse errors, a bounded input-dispatch helper, and peak-concurrency test fixtures.
- **Delete:** direct unbounded submission of the complete input vector to `par_iter().for_each` in the CLI run path.

# Dependencies and ordering

This is the immediate implementation tracked by [work item #9](https://code.ornl.gov/research-enablement/xylem/-/work_items/9). It can land against the current `ExecutionContext`, but it holds the configuration and CLI integration locks while doing so. The later [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md) refactor must preserve this public configuration contract and may move the scheduler behind an immutable run context.

If [configuration validation](../refactors/configuration-validation-and-dry-run.md) lands first, batch validation belongs in its semantic validation phase. Otherwise the batch type must reject zero during deserialization without adding another log-and-success failure path.

# Recommendation

## Public contract

Accept a positive integer under workflow options:

```yaml
options:
  batch: 2
```

`batch` counts active top-level workflow inputs. It does not count image pairs hidden inside one child process, threads created by child tools, or modules within an input. Modules for one input remain sequential.

When `batch` is present, effective input concurrency is `min(batch, threads)`. When it is absent, preserve the current thread-limited behavior. Set `batch: 2` explicitly in affected imagery configurations rather than changing the behavior of every existing workflow.

## Scheduling

- Keep module preparation on the existing worker budget.
- Dispatch workflow inputs through a helper that never has more than the effective input limit in flight.
- Use rolling admission so a new input starts when any active input finishes; do not wait for a whole fixed-size chunk.
- Sort directory-discovered inputs before scheduling and preserve that order in returned outcomes.
- Avoid blocking an entire Rayon pool on a second pool of waiting tasks; either use a dedicated input pool or a scheduler primitive whose permit behavior is covered by a starvation test.

# Validation

- Round-trip `batch` through JSON and YAML.
- Reject zero, negative-shaped input, non-numeric input, and overflow with actionable configuration errors.
- Measure active input bodies with atomics and assert the peak for combinations including `(batch=2, threads=10)` and `(batch=2, threads=1)`.
- Assert every input executes exactly once, modules remain ordered within each input, and returned outcomes preserve input order.
- Verify configurations without `batch` retain the current effective behavior.
- Exercise a representative imagery workflow and document whether its scheduling unit is a directory or an image pair.

# Acceptance criteria

- `options.batch` is a documented positive configuration value.
- Active top-level inputs never exceed `min(batch, threads)`.
- Input admission is rolling, deterministic, and lossless.
- Existing configurations without `batch` preserve their current behavior.
- Module ordering within an input is unchanged.
- Documentation explicitly states that Xylem cannot limit work hidden inside one module invocation.
- Unit, integration, and compiled-CLI tests cover parsing, limits, ordering, and backward compatibility.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)

