---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies found while auditing Xylem 1.0.0 on 2026-09-17. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
ACORN 0.2 upgrade -------------------------> secure module acquisition
                                              |
                                              v
                                      configuration modules
lossless process --------------------------> environment/concurrency
                                                       |
                                                       v
                                             memory-aware scheduling
lossless process --------------------------> managed container lifecycle
```

Landed and removed from the graph: typed errors and workflow outcomes,
resumable execution, structured run reporters, the durable execution journal,
the maintained YAML parser, configuration validation and side-effect-free
planning, completion declarations and markers, the ACORN command environment
sync, and fixed input concurrency. The ACORN 0.2 version migration remains only
as feature narrowing, so its node is still open.

The typed-error, fixed-concurrency, validated-planning, and resume contracts
have landed. Secure acquisition still waits for its acquisition-location and
ACORN APIs. Memory-aware production scheduling remains gated on a successful
measurement spike.

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
- The landed [typed errors and workflow outcomes](refactors/typed-error-and-outcome-boundary.md) contract is the baseline for process and acquisition work; neither may encode failure as logging plus an empty result or process exit.
- [Process execution](refactors/process-execution-contract.md) precedes [environment and concurrency](refactors/workflow-environment-and-concurrency.md): per-invocation environment and cancellation attach to a structured child-process result, while scheduling builds on the existing typed thread budget.
- The landed `options.batch` contract defines the rolling active-input ceiling that environment and concurrency work must consume rather than replace.
- The landed [resumable execution](features/resumable-execution.md) contract requires explicit completion declarations and never infers completion from tracing logs.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- The landed [configuration validation and planning](refactors/configuration-validation-and-dry-run.md) contract precedes secure acquisition and reporters.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- [Configuration modules](refactors/configuration-modules.md) consumes the landed planning, resume, outcome, reporter, and batching contracts and coordinates remote source resolution with secure acquisition.
- The landed [run reporters](features/run-reporters.md) contract consumes structured workflow outcomes and durable journal state without parsing logs.
- [Dependency and security gates](refactors/dependency-surface-and-security-gates.md) follows both dependency migrations so its accepted graph is not immediately invalidated.
- [Hermetic tests](testing/hermetic-workflow-tests.md) provides the regression gate required by the CI plan and validates every correctness plan as its contracts land.
- [CI and release pipeline](refactors/ci-release-pipeline.md) consumes the hermetic suite and dependency gates.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.
- [Documentation and examples](refactors/documentation-and-examples.md) follows user-facing contract decisions, the repaired build/release workflow, and the verified Homebrew installation contract.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | ACORN, ACORN command sync, YAML, dependency gates, process parsing, testing tools | One manifest owner at a time. Land upstream ACORN work before pinning its commit, then regenerate and audit the final lockfile once. |
| `xylem-lib/src/config/mod.rs` | Environments, acquisition, configuration modules, memory scheduling | Preserve the landed typed-error, validated-planning, batching, and resume contracts. |
| `xylem-lib/src/script/mod.rs` | Errors, process execution, environments, acquisition, configuration modules, ACORN command sync | Process execution owns `Script` and child-result contracts; configuration modules owns typed module locations and recursive source resolution. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, memory scheduling | Preserve the landed batching and resume scheduler seam; later plans do not introduce competing admission loops. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Environment/concurrency, configuration modules, memory scheduling, tests | One CLI integration owner. Preserve CLI-owned exit mapping and the landed resume/reporter surface. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, process execution, acquisition, workflow outcomes, scheduling, journal state | ACORN, process, environment, acquisition, configuration modules, memory scheduling |
| `xylem-cli` | Argument precedence, orchestration, presentation, exit status, temporary workspace lifetime | Environment, configuration modules, memory scheduling, tests |
| `workspace` | Dependency graph, CI, releases, package exports, docs, examples | Dependency gates, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
