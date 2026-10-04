---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies rechecked on 2026-10-04. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
ACORN v0.3.4 ----> environment/concurrency -+-> kubernetes workflow backend
               |                               +-> sandboxed workflow execution
               +----> secure module acquisition

ACORN MCP client --later integration--> MCP-enabled tenant tools

memory-aware scheduling --later extension--> durable workflow state
```

Secure acquisition still waits for its acquisition-location APIs. The landed
runtime preserves current fixed concurrency; the environment/concurrency
refactor and blocked memory-aware scheduler later consume or extend its runner
path and durable contracts.

## Quality and delivery path

```text
dependency and security gates --+
                                 +----> CI and release pipeline ----> Homebrew activation
hermetic tests -----------------+

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) follows the process and immutable-environment contracts. It owns the outer local-controller/in-cluster-worker target, shared-PVC protocol, Kubernetes lifecycle commands, and runner-image contract; it does not implement sandbox policy or module-container translation.
- [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md) follows the process and immutable-environment contracts and coordinates with secure acquisition. It owns policy resolution, provider capability negotiation, session lifecycle, and the rule that workflow-controlled subprocesses cannot bypass the sandbox.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- [MCP-enabled tenant tools](features/mcp-tenant-tools.md) consumes the upstream ACORN MCP-client contract. It must not bypass Xylem planning, effect policy, journaling, or the existing module runner.
- Required CI owns enforcement of the accepted dependency graph and advisory policy.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | Dependency gates, published ACORN upgrade, Kubernetes client, testing tools | One manifest owner at a time; regenerate and audit the lockfile after manifest changes. |
| `xylem-lib/src/config/mod.rs` | Environments, acquisition, Kubernetes targeting, sandboxing, scripted input selection, memory scheduling | Preserve the typed-error, validated-planning, batching, resume, YAML merge, and explicit nested-source contracts. |
| `xylem-lib/src/script/mod.rs` | ACORN process adaptation, environments, acquisition, sandboxing | Environment/concurrency owns the `Script` adapter to `acorn-cmd`; sandboxing consumes that result and removes direct workflow spawn paths. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, Kubernetes targeting, sandboxing, memory scheduling | Preserve the batching and resume scheduler seam. |
| Runner, journal, run store, and telemetry modules | Environment/concurrency, memory scheduling, Kubernetes targeting | Land schema changes serially and preserve the versioned durable evidence. |
| `xylem-lib/src/sandbox/` and sandbox conformance fixtures | Sandboxed execution | Sandboxed execution owns policy, capability, session, and provider contracts; consumers may not add provider-specific bypasses. |
| Kubernetes target/profile/protocol modules and fixtures | Kubernetes targeting, sandboxing | Kubernetes owns outer Job execution and shared-PVC transport; sandboxing owns inner policy and neither may reinterpret the other's contract. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Environment/concurrency, Kubernetes targeting, memory scheduling, tests | One CLI integration owner. Preserve CLI-owned exit mapping and the resume/reporter surface. |
| MCP configuration, client adapter, and plan/journal evidence | MCP-enabled tenant tools, environment/concurrency, sandboxing | MCP integration owns the allowlist and invocation boundary; it may not create a second process supervisor or weaken offline, effect, timeout, or output limits. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, Kubernetes image/tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Kubernetes images consume immutable release binaries; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, ACORN process adaptation, acquisition, execution-target and sandbox policy/session types, workflow outcomes, scheduling, journal state, run storage, telemetry, projections | Environment, acquisition, Kubernetes targeting, sandboxing, scripted input selection, memory scheduling |
| `xylem-cli` | Argument precedence, trusted target/policy selection, orchestration, presentation, exit status, Kubernetes control, sandbox and temporary workspace lifetime | Environment, Kubernetes targeting, sandboxing, scripted input selection, memory scheduling, tests |
| `xylem-tui` | Local dashboard state, rendering, interaction, and terminal lifecycle over the command runner and run store | Retained runtime surface |
| `workspace` | Dependency graph, CI, releases, runner images, package exports, docs, examples | Dependency gates, Kubernetes targeting, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
