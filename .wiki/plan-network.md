---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies found while auditing Xylem 1.0.0 on 2026-09-17. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
secure module acquisition
          |              |
          v              |
configuration modules    |
                         v
lossless process -> environment/concurrency -> sandboxed workflow execution
                            |                                |
                            v                   +------------+------------+
              memory-aware scheduling          |                         |
                                               v                         v
                                  shell-script workflow input   managed container lifecycle
                                                                          |
                                                                          v
                                                               container keep-alive
```

Landed and removed from the graph: typed errors and workflow outcomes,
resumable execution, structured run reporters, the durable execution journal,
the maintained YAML parser, configuration validation and side-effect-free
planning, completion declarations and markers, the ACORN command environment
sync and 0.3.2 dependency migration, and fixed input concurrency. Dependency
narrowing remains open.

The typed-error, fixed-concurrency, validated-planning, and resume contracts
have landed. Secure acquisition still waits for its acquisition-location and
ACORN APIs. Memory-aware production scheduling remains gated on a successful
measurement spike.

## Quality and delivery path

```text
YAML migration --------> dependency and security gates
                             |
hermetic tests ---------------+----> CI and release pipeline ----> Homebrew export

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- The landed ACORN 0.3.2 migration supplies the command, location, checksum, filesystem, validation, and feature contracts consumed by acquisition work.
- The ACORN command environment sync has landed and removed Xylem's environment-bearing command fallback: `Script::run_checked` calls `cmd!` with a child-only `env:` override and no `std::process::Command` path remains.
- The maintained YAML parser and configuration validation contracts have landed and are consumed by later work.
- The landed typed-errors and workflow-outcomes contract is the baseline for process and acquisition work; neither may encode failure as logging plus an empty result or process exit.
- [Process execution](refactors/process-execution-contract.md) precedes [environment and concurrency](refactors/workflow-environment-and-concurrency.md): per-invocation environment and cancellation attach to a structured child-process result, while scheduling builds on the existing typed thread budget.
- [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md) follows the process and immutable-environment contracts and coordinates with secure acquisition. It owns policy resolution, provider capability negotiation, session lifecycle, and the rule that workflow-controlled subprocesses cannot bypass the sandbox.
- [Shell-script workflow input](features/shell-script-workflow-input.md) follows sandboxed execution so dynamic input discovery cannot introduce an earlier unsandboxed process path.
- [Managed container lifecycle](refactors/managed-container-lifecycle.md) follows sandboxed execution for sandboxed runs. It may target only a backend-advertised private container runtime and must never mount a host runtime socket to regain current `docker exec` behavior.
- [Managed-container keepalive](features/container-keep-alive.md) follows the managed lifecycle and adds an opt-in startup strategy without replacing image defaults.
- The landed `options.batch` contract defines the rolling active-input ceiling that environment and concurrency work must consume rather than replace.
- The landed resumable-execution contract requires explicit completion declarations and never infers completion from tracing logs.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- The landed configuration-validation and planning contract precedes secure acquisition and reporters.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- Configuration modules are a landed contract consumed by secure acquisition and reporting work.
- The landed run-reporter contract consumes structured workflow outcomes and durable journal state without parsing logs.
- [Dependency and security gates](refactors/dependency-surface-and-security-gates.md) follows both dependency migrations so its accepted graph is not immediately invalidated.
- [Hermetic tests](testing/hermetic-workflow-tests.md) provides the regression gate required by the CI plan and validates every correctness plan as its contracts land.
- [CI and release pipeline](refactors/ci-release-pipeline.md) consumes the hermetic suite and dependency gates.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.
- [Documentation and examples](refactors/documentation-and-examples.md) follows user-facing contract decisions, the repaired build/release workflow, and the verified Homebrew installation contract.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | ACORN, ACORN command sync, YAML, dependency gates, process parsing, testing tools | One manifest owner at a time. Land upstream ACORN work before pinning its commit, then regenerate and audit the final lockfile once. |
| `xylem-lib/src/config/mod.rs` | Environments, acquisition, configuration modules, sandboxing, container keepalive, memory scheduling | Preserve the landed typed-error, validated-planning, batching, and resume contracts. |
| `xylem-lib/src/script/mod.rs` | Errors, process execution, environments, acquisition, configuration modules, sandboxing, container keepalive, ACORN command sync | Process execution owns `Script` and child-result contracts; sandboxing consumes that result and removes direct workflow spawn paths; configuration modules owns typed module locations and recursive source resolution. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, sandboxing, memory scheduling | Preserve the landed batching and resume scheduler seam; later plans do not introduce competing admission loops. |
| `xylem-lib/src/sandbox/` and sandbox conformance fixtures | Sandboxed execution, shell input, managed container lifecycle, container keepalive | Sandboxed execution owns policy, capability, session, and provider contracts; consumers may not add provider-specific bypasses. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Environment/concurrency, configuration modules, memory scheduling, tests | One CLI integration owner. Preserve CLI-owned exit mapping and the landed resume/reporter surface. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, process execution, acquisition, sandbox policy/session, workflow outcomes, scheduling, journal state | ACORN, process, environment, acquisition, configuration modules, sandboxing, shell input, managed containers, container keepalive, memory scheduling |
| `xylem-cli` | Argument precedence, trusted policy selection, orchestration, presentation, exit status, sandbox and temporary workspace lifetime | Environment, configuration modules, sandboxing, shell input, managed containers, container keepalive, memory scheduling, tests |
| `workspace` | Dependency graph, CI, releases, package exports, docs, examples | Dependency gates, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
