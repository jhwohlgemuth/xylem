---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies rechecked on 2026-09-29. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
lossless process ----+----> secure module acquisition
                     |
                     v
        environment/concurrency -> sandboxed workflow execution
                     |                         |
                     v                         v
       memory-aware scheduling    shell-script workflow input
```

Secure acquisition still waits for its acquisition-location APIs.
Memory-aware production scheduling remains gated on a successful measurement
spike.

## Quality and delivery path

```text
dependency and security gates --+
                                 +----> CI and release pipeline ----> Homebrew activation
hermetic tests -----------------+

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- [Process execution](refactors/process-execution-contract.md) precedes [environment and concurrency](refactors/workflow-environment-and-concurrency.md): per-invocation environment and cancellation attach to a structured child-process result, while scheduling builds on the existing typed thread budget.
- [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md) follows the process and immutable-environment contracts and coordinates with secure acquisition. It owns policy resolution, provider capability negotiation, session lifecycle, and the rule that workflow-controlled subprocesses cannot bypass the sandbox.
- [Shell-script workflow input](features/shell-script-workflow-input.md) follows sandboxed execution so dynamic input discovery cannot introduce an earlier unsandboxed process path.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- [Dependency and security gates](refactors/dependency-surface-and-security-gates.md) owns the accepted dependency graph and advisory policy.
- [Hermetic tests](testing/hermetic-workflow-tests.md) provides the regression gate required by the CI plan and validates every correctness plan as its contracts land.
- [CI and release pipeline](refactors/ci-release-pipeline.md) consumes the hermetic suite and dependency gates.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | Dependency gates, process parsing, testing tools | One manifest owner at a time; regenerate and audit the lockfile after manifest changes. |
| `xylem-lib/src/config/mod.rs` | Environments, acquisition, sandboxing, memory scheduling | Preserve the typed-error, validated-planning, batching, and resume contracts. |
| `xylem-lib/src/script/mod.rs` | Process execution, environments, acquisition, sandboxing | Process execution owns `Script` and child-result contracts; sandboxing consumes that result and removes direct workflow spawn paths. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, sandboxing, memory scheduling | Preserve the batching and resume scheduler seam; later plans do not introduce competing admission loops. |
| `xylem-lib/src/sandbox/` and sandbox conformance fixtures | Sandboxed execution, shell input | Sandboxed execution owns policy, capability, session, and provider contracts; consumers may not add provider-specific bypasses. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Environment/concurrency, memory scheduling, tests | One CLI integration owner. Preserve CLI-owned exit mapping and the resume/reporter surface. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, process execution, acquisition, sandbox policy/session, workflow outcomes, scheduling, journal state | Process, environment, acquisition, sandboxing, shell input, memory scheduling |
| `xylem-cli` | Argument precedence, trusted policy selection, orchestration, presentation, exit status, sandbox and temporary workspace lifetime | Environment, sandboxing, shell input, memory scheduling, tests |
| `workspace` | Dependency graph, CI, releases, package exports, docs, examples | Dependency gates, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
