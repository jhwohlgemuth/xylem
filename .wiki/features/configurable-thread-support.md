---
type: Plan
title: "Add configurable thread support"
description: "Add an ACORN-compatible -t/--threads control for current Rayon work and make it the first explicit execution limit for later orchestration."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, cli, threads, rayon, concurrency, orchestration]
status: planned
---

# Goal

Add a global `-t N`/`--threads N` option, defaulting to 10 like ACORN, and apply one validated thread budget to Xylem's existing Rayon-backed module preparation and parallel input execution. Treat this as the first execution-limit primitive for later orchestration, not as a complete scheduler.

# Current evidence

- Xylem directly depends on Rayon in both `xylem-lib` and `xylem-cli`, but exposes no way to control its worker count.
- `Config::prepare_modules` uses `par_iter` for module preparation, while the run command separately uses `par_iter` for configured or command-line inputs. Both implicitly use Rayon's process-global default pool.
- The current run path has no typed execution-limit object that later process, network, cancellation, or resource scheduling policies can extend.
- ACORN exposes `-t`/`--threads` with value name `N` and a default of 10, uses the value to size Rayon work, and carries it in common runtime options. Its asynchronous workflow planning also uses the value as a buffered-concurrency limit while preserving source order.
- ACORN gives zero two different meanings: its CLI skips custom global-pool initialization, while asynchronous planning coerces zero to one. Xylem should not inherit that ambiguity.
- Rayon is already in the dependency graph, so this initial feature requires no new runtime dependency.

# Project code boundaries

- **Update:** `xylem-cli/src/cli.rs`, `xylem-cli/src/main.rs`, `xylem-cli/src/commands/run/mod.rs`, the Rayon call sites in `xylem-lib/src/config/mod.rs`, generated/help documentation, and focused CLI/library tests.
- **Create:** a small typed execution-limit value in `xylem-lib` and test helpers that measure peak concurrent work without timing-only assertions.
- **Delete:** direct reliance on the implicit global Rayon pool in Xylem-owned orchestration. No dependency deletion is required.

# Dependencies and ordering

Land after [typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md) so invalid limits and pool-construction failures use the shared error boundary. Land before [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md), which expands this initial CPU/blocking-work limit into deterministic multi-input orchestration, cancellation, and failure policy.

This plan shares the CLI and configuration integration locks with both plans. It does not depend on the full [process execution contract](../refactors/process-execution-contract.md), but that contract and this plan must both land before advanced orchestration begins.

# Recommendation

## CLI contract

- Add a top-level `threads` argument with short form `-t`, long form `--threads`, value name `N`, and default 10 to match ACORN's user-facing behavior.
- Accept positive integers only. Reject zero with a clear parse error instead of copying ACORN's context-dependent zero semantics.
- Document the option as the maximum number of Xylem Rayon workers, not as the number of threads used inside child tools or third-party runtimes.
- Resolve the value once at startup and pass the typed result into run orchestration. Do not reread it from global state or an environment variable at individual call sites.

## Execution boundary

- Represent the resolved value as a non-zero typed limit, preferably inside an extensible `ExecutionLimits` value rather than passing an unqualified `usize` through public APIs.
- Build one scoped Rayon pool for a run and execute Xylem-owned parallel phases inside it. Prefer a scoped/local pool over `build_global` so embedding Xylem or initializing Rayon elsewhere does not create a one-time global-pool conflict.
- Apply the same pool to module preparation and input execution. Because those phases are currently sequential, `N` is a run-wide ceiling for each Rayon phase rather than two independent budgets.
- Keep current module ordering and workflow semantics unchanged in this slice. Do not introduce nested module/input parallelism merely because a configurable pool exists.
- Record the resolved limit in the future run context and structured outcome so diagnostics and reporters can explain how a run was scheduled.

## Orchestration boundary

For this initial feature, `threads` controls only Xylem work currently submitted to Rayon. It does not promise to limit:

- threads created internally by child commands, Conda, containers, or downloaded tools;
- asynchronous download or network request concurrency;
- simultaneous child-process count as a separately configurable resource;
- workflow DAG dependencies, priorities, retries, fail-fast behavior, or cancellation;
- CPU, memory, GPU, or per-module resource requests.

The later environment/concurrency plan should reuse `ExecutionLimits` while adding distinct policies where one integer would be misleading. A future scheduler may derive a default process-slot limit from `threads`, but it must expose and document that relationship rather than silently redefining `-t`.

# Validation

- Add parser tests for the default of 10, both flag spellings, explicit one and larger values, zero, non-numeric values, and missing values.
- Snapshot or assert help output so the short flag, long flag, value name, default, and scope remain stable.
- Use atomics and a barrier-controlled test worker to prove `threads = 1` is serial and that peak module-preparation and input concurrency never exceeds `N`; avoid tests that infer concurrency only from elapsed time.
- Exercise more work items than workers and verify every item runs exactly once.
- Verify the scoped pool works when Rayon's global pool has already been initialized by the test host or an embedding application.
- Run the existing workflow suite at one thread and multiple threads to detect accidental ordering or state-sharing assumptions.
- Confirm the feature adds no dependency and does not change child-tool environment or internal thread settings.

# Acceptance criteria

- `xylem -t N run ...` and `xylem --threads N run ...` select a positive run-wide Rayon worker limit, with a default of 10.
- Zero and malformed values fail during argument parsing with actionable diagnostics.
- All Xylem-owned `par_iter` work in module preparation and input execution runs under the selected scoped pool.
- Peak measured work does not exceed the selected limit, including when a global Rayon pool already exists.
- The resolved limit is carried through a typed execution boundary that later orchestration can extend without changing the CLI contract.
- Documentation clearly distinguishes this initial worker limit from future scheduling, cancellation, process, network, and resource controls.

# Related plans

- [Typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md)
- [Process execution contract](../refactors/process-execution-contract.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Structured run reporters](run-reporters.md)
