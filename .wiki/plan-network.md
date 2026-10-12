---
okf_version: "0.1"
---

# Plan dependency network

Rechecked on 2026-10-11. An arrow means the left side establishes a contract
required by the right side. Landed baselines are not active plans.

## Delivery paths

```text
ACORN 0.3.4 process contract --> environment/concurrency --> Kubernetes target
                              \-> secure acquisition

ACORN 0.4.0 embedder interface --> MCP-enabled tenant tools

macOS runner + required tap jobs --> Homebrew activation

A/P/O rollout decision + credentials --> A/P/O memory validation
```

The sandbox policy/provider boundary, memory admission seam, execution-plan
v12, journal/run-summary v11, run store, cancellation vocabulary, and split run
orchestration are landed constraints. Active plans extend them; they do not
redefine them incidentally.

## Direct dependency edges

- [MCP-enabled tenant tools](features/mcp-tenant-tools.md) consumes the ACORN
  0.4.0 narrow `mcp-client`, direct server-map constructor, public backend
  trait, and cancellation-aware operations landed in `aa36e93f`. Xylem must
  qualify one accepted revision before changing its manifests.
- [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md)
  consumes the published ACORN process contract and must preserve current
  sandbox, memory, journal, reporter, resume, and run-store behavior.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes
  typed errors, managed process execution, configuration validation, and the
  eventual immutable context/workspace boundary. Its planner, bounded
  executors, and cache have landed; host-run integration waits for that
  workspace boundary.
- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md)
  follows immutable contexts, owned workspaces, structured cancellation, and a
  reusable local execution boundary. It also requires qualified shared storage,
  cluster runners, and an immutable worker-image pipeline.
- [A/P/O memory validation](features/memory-aware-scheduling.md) has no runtime
  implementation dependency. It waits for a rollout decision, the pinned
  corpus, and owner-provided Earthdata credentials.
- [Homebrew release activation](features/homebrew-export.md) waits for a
  compatible macOS runner and required two-platform tap validation.

## Shared integration boundaries

| Boundary | Active plans | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock`, `deny.toml` | MCP, acquisition, Kubernetes | One manifest owner at a time; inspect the feature tree and audit the regenerated lockfile. |
| `crates/xylem-lib/src/config/` | environment, acquisition, Kubernetes, MCP | Preserve typed validation, `options.batch`, memory fallback, resume, source nesting, and sandbox opt-in. |
| `crates/xylem-lib/src/script/` | environment, acquisition, Kubernetes | Environment/concurrency owns the host adapter changes; acquisition and Kubernetes consume the accepted process contract. |
| `crates/xylem-lib/src/execution.rs` and `crates/xylem-cli/src/commands/run/dispatch.rs` | environment, Kubernetes, MCP | Keep one ordered admission seam and preserve fixed and memory-aware limits. |
| `crates/xylem-lib/src/sandbox/` and provider fixtures | landed baseline | No active plan owns this boundary. Changes require a focused plan and must preserve provider conformance and lifecycle evidence. |
| Plan, journal, run store, projection, telemetry, and reporter modules | environment, Kubernetes, MCP | Allocate schema revisions serially and update writers, recovery, projections, reporters, schemas, and TUI together. |
| `crates/xylem-cli/src/commands/run/`, CLI roots, and generated help | environment, Kubernetes, MCP | One CLI integration owner at a time; preserve CLI-owned exit mapping and dry-run behavior. |
| `crates/xylem-tui/` | MCP | Add only projected durable state; never render arbitrary MCP payloads. |
| `Makefile`, `.gitlab-ci.yml`, release exporters, runner images | Kubernetes, Homebrew | Kubernetes images consume immutable binaries; Homebrew activation changes the tap only after both validation jobs pass. |
| Top-level and `docs/` Markdown plus `examples/` | every user-visible plan | Final names and behavior land before documentation integration. |

## Target ownership

| Package | Boundary | Active plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, acquisition, execution policy, outcomes, scheduling, journals, run storage, telemetry, projections | environment, acquisition, Kubernetes, MCP |
| `xylem-cli` | Argument precedence, orchestration, execution targets, presentation, exit status, cancellation, workspace lifetime | environment, acquisition, Kubernetes, MCP |
| `xylem-tui` | Durable run projection and interaction | MCP |
| `workspace` | Dependency graph, CI, releases, images, exports, documentation, examples | acquisition, Kubernetes, MCP, Homebrew, memory validation |

## Handoff checklist

Before implementation, record the plan, exact files, claimed locks, public
contracts, baseline commit, prerequisite revision, focused validation, and
landing order. Before handoff, report changed files, schema or public-contract
changes, generated artifacts, validation results, and any lock still required.
