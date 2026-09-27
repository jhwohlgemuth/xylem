---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies found while auditing Xylem 1.0.0 on 2026-09-17. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
ACORN 0.2 upgrade -------------------------> secure module acquisition
                                                    ^
                                                    |
typed errors and workflow outcomes --> lossless process
          |                                  |
          |                                  v
          |                         environment/concurrency
          |                                  |
          +--> resumable close-out           +--> memory-aware scheduling
          |
          +--> run reporters
```

Landed and removed from the graph: the durable execution journal, the maintained YAML parser, configuration validation and side-effect-free planning, completion declarations and markers, the ACORN command environment sync, fixed input concurrency, and most of run reporters. The ACORN 0.2 version migration remains only as feature narrowing, so its node is still open.

The fixed-concurrency and validated-planning contracts have landed. Secure acquisition still waits for the remaining typed-error, acquisition-location, and ACORN APIs. Memory-aware production scheduling remains gated on a successful measurement spike.

## Quality and delivery path

```text
ACORN upgrade ----+
YAML migration ---+----> dependency and security gates
                             |
hermetic tests ---------------+----> CI and release pipeline ----> Homebrew export

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- [ACORN 0.2 upgrade](migrations/acorn-0-2-upgrade.md) precedes acquisition changes because `download_binary` is asynchronous, checksum values are typed, portable location types moved to `acorn-core`, and SSH is no longer an ACORN `Scheme` variant.
- The ACORN command environment sync has landed and removed Xylem's environment-bearing command fallback: `Script::run_checked` calls `cmd!` with a child-only `env:` override and no `std::process::Command` path remains.
- [Maintained YAML parser](migrations/maintained-yaml-parser.md) and [configuration validation](refactors/configuration-validation-and-dry-run.md) are completed records consumed by later work.
- [Typed errors and workflow outcomes](refactors/typed-error-and-outcome-boundary.md) precedes process, configuration, acquisition, and reporter work; none of those plans may encode failure as logging plus `Vec::new()` or `process::exit`.
- [Process execution](refactors/process-execution-contract.md) precedes [environment and concurrency](refactors/workflow-environment-and-concurrency.md): per-invocation environment and cancellation attach to a structured child-process result, while scheduling builds on the existing typed thread budget.
- The landed `options.batch` contract defines the rolling active-input ceiling that environment and concurrency work must consume rather than replace.
- [Resumable execution](features/resumable-execution.md) is the prioritized owner for remaining typed-error hardening, resume edge cases, and release reconciliation; completion is never inferred from tracing logs.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- The landed [configuration validation and planning](refactors/configuration-validation-and-dry-run.md) contract precedes secure acquisition and reporters.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- [Run reporters](features/run-reporters.md) consumes structured workflow outcomes, the existing durable journal, and the dry-run plan; it does not parse logs to infer success.
- [Dependency and security gates](refactors/dependency-surface-and-security-gates.md) follows both dependency migrations so its accepted graph is not immediately invalidated.
- [Hermetic tests](testing/hermetic-workflow-tests.md) provides the regression gate required by the CI plan and validates every correctness plan as its contracts land.
- [CI and release pipeline](refactors/ci-release-pipeline.md) consumes the hermetic suite and dependency gates.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.
- [Documentation and examples](refactors/documentation-and-examples.md) follows user-facing contract decisions, the repaired build/release workflow, and the verified Homebrew installation contract.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | ACORN, ACORN command sync, YAML, dependency gates, process parsing, testing tools | One manifest owner at a time. Land upstream ACORN work before pinning its commit, then regenerate and audit the final lockfile once. |
| `xylem-lib/src/config/mod.rs` | Errors, validation, environments, acquisition, fixed batching, resume, memory scheduling, reporters | Typed errors own signature conversion first; validation owns parse/plan separation; fixed batching owns its public option; resume and memory work extend only validated extracted configuration types. |
| `xylem-lib/src/script/mod.rs` | Errors, process execution, environments, acquisition, ACORN command sync | Process execution owns `Script` and child-result contracts; the ACORN sync changes only the execution adapter after the upstream commit is fixed. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, fixed batching, resume, memory scheduling | Fixed batching establishes the scheduler seam; later plans consume it serially and do not introduce competing admission loops. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Errors, environment/concurrency, fixed batching, resume, memory scheduling, reporters, tests | One CLI integration owner. Unit/fixture work may proceed without changing command arguments or top-level dispatch. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, process execution, acquisition, workflow outcomes, scheduling, journal state | ACORN, ACORN command sync, YAML, errors, process, environment, validation, acquisition, fixed batching, resume, memory scheduling |
| `xylem-cli` | Argument precedence, orchestration, presentation, exit status, temporary workspace lifetime | Errors, environment, fixed batching, resume, memory scheduling, reporters, tests |
| `workspace` | Dependency graph, CI, releases, package exports, docs, examples | Dependency gates, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
