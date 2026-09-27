---
type: Plan
title: "Isolate workflow environments and define concurrency"
description: "Replace process-global environment mutation and implicit Rayon behavior with immutable per-input contexts and deterministic workflow results."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, environment, concurrency, rayon, determinism, cleanup]
status: planned
---

# Goal

Give each workflow input an immutable execution context containing its variables, paths, options, and cancellation state. Build on the initial `-t`/`--threads` worker limit to run inputs through an explicit, deterministic orchestration policy without mutating the parent process environment or losing result order.

# Current evidence

- `Config::set_environment_variables` writes configuration values and `MODULE_LIST` into the process-global environment.
- The CLI runs inputs through `dispatch_inputs_fail_fast`, which builds a dedicated input pool, so concurrent inputs still inherit shared mutable environment state and cannot be isolated.
- `get_environment_variables` returns no configuration or module variables when input is `None`; module variables therefore disappear for zero-input workflows.
- Per-input and module variables are now attached to each script through `with_envs`, but globally set configuration values still leak to every child and to any later workflow in the same process.
- Variable precedence is implicit: module values overwrite configuration values, then `INPUT` and `PARENT_DIRECTORY` overwrite both. Reserved-name collisions are not validated.
- Template tests mutate `_XYLEM_*` process variables and do not restore them, making parallel test execution order-dependent.
- The landed `options.batch` ceiling gives rolling admission, fail-fast, and ordered outcomes, but those semantics live in `ExecutionContext` rather than behind an immutable run context, and no cancellation or memory signal reaches the admission decision.
- The CLI creates a random temporary root but never gives it an explicit retention or cleanup lifetime.

# Project code boundaries

- **Update:** environment and workflow methods in `xylem-lib/src/{config/mod,script/mod}.rs`, orchestration in `xylem-cli/src/commands/run/mod.rs`, tests, and configuration documentation.
- **Create:** immutable `RunContext`/`WorkflowContext`, variable-resolution tests, concurrency-policy types, and an RAII workspace owner.
- **Delete:** production `set_var`/global Xylem environment mutation, `EnvironmentValue::get_all` as an execution dependency, unbounded result-discarding `par_iter().for_each`, and leaked temporary roots.

# Dependencies and ordering

Follows [typed outcomes](typed-error-and-outcome-boundary.md) and the [process execution contract](process-execution-contract.md). It extends the existing single Rayon budget with scheduling, failure, and cancellation semantics rather than redefining `-t`. The `options.batch` rolling input ceiling has already landed, so this refactor preserves it and moves its admission helper behind the immutable context rather than introducing a competing limit. Its aggregate results feed the existing durable execution journal and [run reporters](../features/run-reporters.md).

# Recommendation

## Variable resolution

Resolve variables once per input with a documented precedence:

1. workflow variables;
2. module variables;
3. reserved derived values such as input path, parent path, output path, module list, and run ID.

Reject user attempts to override reserved names unless an explicit override policy is documented. Keep template keys unprefixed inside the context, and apply the `_XYLEM_` prefix only when constructing child environment keys. Pass child variables through `Command::envs`; do not alter the parent environment.

Support zero-input workflows with the same workflow/module variables and no input-derived keys. Resolve relative input and output paths against the configuration file's directory, not whichever directory invoked the CLI.

## Concurrency

- Reuse the resolved `ExecutionLimits` thread budget as the initial maximum for current CPU/blocking input work; introduce separate limits when process or asynchronous I/O concurrency needs different semantics.
- Preserve input order in the returned outcome even when execution completes out of order.
- Define `continue`, `fail-fast`, and cancellation behavior. Fail-fast stops scheduling new inputs and terminates owned running children through the process contract.
- Keep modules within one input sequential unless the configuration explicitly grows a dependency graph.
- Create one run workspace with one input-specific subdirectory per input to prevent download, clone, and output collisions.

## Workspace lifetime

Use an RAII temporary workspace. Remove it after success by default, retain it only through an explicit diagnostic option, and report the retained path. Dry-run planning should not create it.

# Validation

- Test variable precedence, reserved-name rejection, zero-input behavior, non-Unicode paths, duplicate inputs, and path resolution.
- Run many inputs with conflicting variable values and assert that child processes observe only their own context.
- Test bounded concurrency, deterministic outcome ordering, fail-fast, continue-on-error, cancellation, and child cleanup.
- Verify the parent environment is byte-for-byte unchanged before and after library execution.
- Verify temporary workspaces clean up on success, failure, panic unwind, and cancellation, with an explicit retain mode.

# Acceptance criteria

- Production workflow execution never mutates the parent process environment.
- Every child receives the complete resolved environment for its input and module, including zero-input workflows.
- Variable precedence and reserved names are explicit and tested.
- Concurrency is bounded and returns deterministic ordered outcomes.
- Failure/cancellation policy is configurable and no child or temporary workspace is orphaned.
- Dry-run creates no workspace and performs no execution-side environment mutation.

# Related plans

- [Process execution contract](process-execution-contract.md)
- [Configuration validation and dry-run](configuration-validation-and-dry-run.md)
- [Memory-aware scheduling](../features/memory-aware-scheduling.md)
- [Structured run reporters](../features/run-reporters.md)
