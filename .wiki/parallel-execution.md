---
okf_version: "0.1"
---

# Parallel execution guide

This guide overlays write ownership on the [plan dependency
network](plan-network.md). Work is parallel only when exact write sets are
disjoint and neither slice holds the same integration lock.

## Integration locks

| Lock | Files | Owner rule |
|---|---|---|
| Manifest | `Cargo.toml`, `Cargo.lock`, crate manifests, `deny.toml` | MCP, acquisition, and Kubernetes serialize dependency changes. |
| Configuration | `crates/xylem-lib/src/config/` | Environment, acquisition, Kubernetes, and MCP preserve current validation, batching, memory, resume, and sandbox contracts. |
| Process adapter | `crates/xylem-lib/src/script/` and process fixtures | Environment/concurrency owns host-adapter changes. Other plans consume the accepted boundary. |
| Scheduler | `crates/xylem-lib/src/execution.rs`, `commands/run/dispatch.rs` | Preserve ordered fixed and memory-aware admission. One owner changes dispatch at a time. |
| Sandbox baseline | `crates/xylem-lib/src/sandbox/`, policy/schema/provider fixtures | No active owner. A change requires a focused plan and conformance rerun. |
| Run state | plan, journal, run-store, projection, telemetry, reporter, schema, and TUI files | One schema owner at a time; all durable readers and views move together. |
| CLI | CLI roots, `commands/run/`, generated help | One integration owner for argument registration, orchestration, cancellation, and exit mapping. |
| Delivery | `Makefile`, `.gitlab-ci.yml`, images, release exporters | Serialize release and image wiring; external runner administration does not claim repository files. |
| Documentation | top-level and `docs/` Markdown, `examples/` | Integrate after behavior and names stabilize. |

## Work lanes

### Lane A: tenant integration and acquisition

1. [MCP-enabled tenant tools](features/mcp-tenant-tools.md), beginning with
   qualification of ACORN `aa36e93f` or a newer accepted 0.4.0 revision.
2. [Secure module acquisition](refactors/secure-module-acquisition.md). Its
   planner, executors, and cache are implemented. Manifest, configuration,
   run-state, and CLI integration waits for an owned workspace and serializes
   with MCP.

### Lane B: execution runtime

1. [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md),
   preserving landed sandbox providers, memory admission, run-store evidence,
   and split run orchestration.
2. [Kubernetes workflow execution](features/kubernetes-workflow-backend.md),
   only after the immutable context/workspace and cancellation prerequisites
   plus the operator-demand, PVC, image, and cluster gates are accepted.

### Lane C: operational validation

1. [Homebrew release activation](features/homebrew-export.md): assign the macOS
   runner, rerun validation, require both jobs, merge, and verify installs.
2. [A/P/O memory validation](features/memory-aware-scheduling.md): run only
   after rollout selection, corpus access, and credentials are available.

These operational slices can run beside repository implementation while they
do not edit shared CI, documentation, or evidence files.

## Safe initial batch

| Slice | Primary ownership | Restriction |
|---|---|---|
| ACORN MCP revision qualification | temporary feature-tree/compatibility evidence | Do not change manifests until the revision passes the recorded gates. |
| Homebrew macOS runner assignment | tap project settings and runner host | Do not merge or weaken required validation. |
| Secure acquisition planner design | new pure planner/cache modules and fixtures | Do not integrate configuration, CLI, process, or journal paths while MCP owns them. |

## Agent start checklist

- Name the plan and atomic slice.
- List exact files to update, create, and delete.
- Claim required locks.
- List public contracts changed.
- Record baseline and prerequisite commits.
- Name the focused validation command.
- Stop and reschedule if the slice expands into another lane's lock.
