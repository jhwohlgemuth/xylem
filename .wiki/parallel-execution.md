---
okf_version: "0.1"
---

# Parallel execution guide

This guide overlays write ownership on the [plan dependency network](plan-network.md). Plans in different lanes are concurrent only when their declared files are disjoint and they avoid the listed integration locks.

## Integration locks

| Lock | Files | Owner rule |
|---|---|---|
| Manifest lock | `Cargo.toml`, `Cargo.lock`, `xylem-*/Cargo.toml`, `deny.toml` | One remaining migration or dependency-gate owner at a time. |
| Configuration lock | `xylem-lib/src/config/mod.rs` and its future extracted modules | Preserve typed errors, validation, `options.batch`, and resume while acquisition, Kubernetes targeting, sandboxing, or memory work extends them. |
| Process adapter lock | `xylem-lib/src/script/mod.rs` and process fixtures | Environment/concurrency owns Xylem's adapter to the published `acorn-cmd` contract; later Kubernetes worker and sandbox work consume it. |
| Scheduler lock | `xylem-lib/src/execution.rs`, input admission in `xylem-cli/src/commands/run/mod.rs` | Preserve fixed batching and resume; environment, Kubernetes, sandbox, and memory work consume the boundary serially rather than adding competing admission loops. |
| Sandbox lock | `xylem-lib/src/sandbox/`, sandbox policy and conformance fixtures | Sandboxed execution owns policy resolution, capability negotiation, session lifecycle, and provider adapters. |
| Kubernetes target lock | target/profile/protocol modules, native client adapter, Job fixtures, and runner image | Kubernetes execution owns the outer controller/worker and shared-PVC contracts; it may not redefine sandbox or module-container semantics. |
| Run-state lock | `xylem-lib/src/progress.rs`, journal/outcome modules, reporter adapters | Preserve the journal, reporter, resume, and execution-target vocabulary without redefining durable states. |
| Run-store lock | runner, run-store, telemetry, task-log, and projection modules plus current schemas | The dashboard baseline has landed. Later runtime and scheduler plans must preserve or explicitly version its bundle contracts. |
| CLI lock | `xylem-cli/src/{main,cli}.rs`, `xylem-cli/src/commands/run/mod.rs` | One owner for argument registration, pool/scheduler setup, journal lifecycle, CLI-owned exit mapping, and generated help. |
| TUI lock | `xylem-tui/` and dashboard state | One dashboard owner at a time; rendering work may split by tab only after the app-state and shared-widget contracts land. |
| Delivery lock | `Makefile`, `.gitlab-ci.yml`, release exporters | Security commands land before CI/release wiring; package and Kubernetes image export consume the immutable release contract. |
| Documentation lock | top-level Markdown and `examples/` | Final updates occur after behavior and command names are stable. |
| MCP integration lock | MCP configuration, ACORN client adapter, tool fixtures, and plan/journal evidence | MCP tenant tools own the allowlist and policy bridge; they may not bypass the runner, sandbox, or durable evidence contracts. |

## Work lanes

The lanes below contain only remaining work.

### Lane A: configuration and acquisition

1. [Secure module acquisition](refactors/secure-module-acquisition.md), consuming the validation contract
2. [MCP-enabled tenant tools](features/mcp-tenant-tools.md), after ACORN's narrow `mcp-client`, direct server-map constructor, and cancellation-aware operations are qualified

Secure acquisition owns generic transport behavior.

### Lane B: execution runtime

1. [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md), preserving the landed CEL selection boundary and following the published [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md)
2. [Kubernetes workflow execution backend](features/kubernetes-workflow-backend.md), after process and immutable environment contracts land
3. [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md), after process and immutable environment contracts land
4. [Memory-aware scheduling](features/memory-aware-scheduling.md), whose scoped estimator gates, planning, and runtime admission are complete
The `options.batch` rolling input ceiling and landed CEL selection snapshot are baselines these steps must preserve, along with the peak-concurrency tests in `xylem-lib/src/execution.rs`. Memory-aware scheduling next claims journal and report schema ownership for durable estimator evidence. A new fifth imagery case may qualify the reviewed imagery profiles for concurrent admission only after fresh individual and mix measurements.

### Lane C: CLI and verification


Fixture cleanup and fake-command helpers can proceed without the CLI lock until
they need a new runtime contract.

### Lane D: delivery and communication

1. CI/release analysis and isolated script prototypes
2. [Homebrew release export](features/homebrew-export.md) after immutable release artifacts and checksums exist

## Safe initial batch

The following slices have disjoint primary write sets:

| Slice | Primary ownership | Restriction |
|---|---|---|
| Hermetic helper preparation | new test-support module and new fixtures | Do not edit CLI roots or rewrite assertions owned by later contracts. |
| Memory durable evidence | journal/report schemas, projections, and recovery tests | Preserve execution-plan v10 and runtime admission; do not combine optional peak observation with the portable reservation contract. |
| CI rule validation | isolated CI lint inputs and plan notes | Do not edit `Makefile` or `.gitlab-ci.yml` until dependency-gate commands are accepted. |

## Agent start checklist

- Name the plan and atomic slice.
- List exact files to update, create, and delete.
- Claim required locks.
- List public contracts changed.
- Record baseline commit and prerequisite landing commit.
- Name the focused validation command.
- Stop and reschedule if the slice expands into another lane's lock.
