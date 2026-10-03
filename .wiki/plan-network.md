---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies rechecked on 2026-10-03. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
ACORN Xylem prerequisites --+----> secure module acquisition
                            |
                            v
         environment/concurrency -+-> kubernetes workflow backend
                      |            |
                      |            +-> sandboxed workflow execution
                      |
                      +------------+-> CEL workflow input
                      |
                      v
        memory-aware scheduling ------> workflow dashboard
```

Secure acquisition still waits for its acquisition-location APIs. Memory-aware
scheduling is authorized only for the measured workflow boundary recorded by
its plan. The workflow dashboard follows both the shared runner boundary and
the landed memory scheduler.

## Quality and delivery path

```text
dependency and security gates --+
                                 +----> CI and release pipeline ----> Homebrew activation
hermetic tests -----------------+

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- The upstream [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md) precede [environment and concurrency](refactors/workflow-environment-and-concurrency.md): Xylem consumes the published managed process contract without adding another supervisor.
- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) follows the process and immutable-environment contracts. It owns the outer local-controller/in-cluster-worker target, shared-PVC protocol, Kubernetes lifecycle commands, and runner-image contract; it does not implement sandbox policy or module-container translation.
- [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md) follows the process and immutable-environment contracts and coordinates with secure acquisition. It owns policy resolution, provider capability negotiation, session lifecycle, and the rule that workflow-controlled subprocesses cannot bypass the sandbox.
- [CEL workflow input](features/cel-workflow-input.md) follows the immutable input-scheduling contract and coordinates with dependency gates for interpreter qualification. It does not depend on sandboxed execution because evaluation has no process or I/O capability.
- [Memory-aware scheduling](features/memory-aware-scheduling.md) builds on the fixed scheduler boundary and journal telemetry. Only its measurement spike may proceed before an estimator is accepted.
- The [workflow dashboard](features/workflow-dashboard.md) follows environment/concurrency and memory-aware scheduling. It consumes ACORN's JSONC, config discovery, project-cache, and managed process contracts, then owns the canonical run store, runner event stream, live telemetry, projections, and TUI.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- Required CI owns enforcement of the accepted dependency graph and advisory policy.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | Dependency gates, published ACORN upgrade, Kubernetes client, dashboard/TUI, testing tools | One manifest owner at a time; regenerate and audit the lockfile after manifest changes. |
| `xylem-lib/src/config/mod.rs` | Environments, acquisition, Kubernetes targeting, sandboxing, CEL input, memory scheduling, dashboard config discovery | Preserve the typed-error, validated-planning, batching, resume, YAML merge, and explicit nested-source contracts. |
| `xylem-lib/src/script/mod.rs` | ACORN process adaptation, environments, acquisition, sandboxing | Environment/concurrency owns the `Script` adapter to `acorn-cmd`; sandboxing consumes that result and removes direct workflow spawn paths. |
| `xylem-lib/src/execution.rs` and input scheduling in `commands/run/mod.rs` | Environment/concurrency, Kubernetes targeting, sandboxing, memory scheduling, dashboard runner extraction | Preserve the batching and resume scheduler seam; the dashboard consumes scheduler events and simulation without introducing another admission loop. |
| Runner, journal, run store, and telemetry modules | Environment/concurrency, memory scheduling, Kubernetes targeting, dashboard | Land schema and event changes serially. The dashboard takes this lock only after the runtime and scheduler contracts land. |
| `xylem-lib/src/sandbox/` and sandbox conformance fixtures | Sandboxed execution | Sandboxed execution owns policy, capability, session, and provider contracts; consumers may not add provider-specific bypasses. |
| Kubernetes target/profile/protocol modules and fixtures | Kubernetes targeting, sandboxing | Kubernetes owns outer Job execution and shared-PVC transport; sandboxing owns inner policy and neither may reinterpret the other's contract. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Environment/concurrency, Kubernetes targeting, memory scheduling, dashboard, tests | One CLI integration owner. Preserve CLI-owned exit mapping and the resume/reporter surface while the dashboard adds positional config and `tui`. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, Kubernetes image/tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Kubernetes images consume immutable release binaries; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, ACORN process adaptation, acquisition, execution-target and sandbox policy/session types, workflow outcomes, scheduling, journal state, run storage, telemetry, projections | Environment, acquisition, Kubernetes targeting, sandboxing, CEL input, memory scheduling, dashboard |
| `xylem-cli` | Argument precedence, trusted target/policy selection, orchestration, presentation, exit status, Kubernetes control, sandbox and temporary workspace lifetime | Environment, Kubernetes targeting, sandboxing, CEL input, memory scheduling, dashboard, tests |
| `xylem-tui` | Local dashboard state, rendering, interaction, and terminal lifecycle over the shared runner and run store | Workflow dashboard |
| `workspace` | Dependency graph, CI, releases, runner images, package exports, docs, examples | Dependency gates, Kubernetes targeting, dashboard, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
