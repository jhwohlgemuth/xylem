---
okf_version: "0.1"
execution: coordination
---

# Plan dependency network

Rechecked on 2026-11-13 at clean `9f85428` with named and positional bindings
committed. Arrows denote a
required contract or gate, not whole-plan serialization. Execution-plan v15,
run-summary v14, immutable contexts, cancellation, verified acquisition,
owned workspaces, sandbox providers, memory admission, and run storage are
accepted baselines. External evidence scope and remaining uncertainty are in
[the root review](index.md#portfolio-review).

## Delivery paths

```text
accepted shared interpolation + input/output/local source expansion -> EXP-05 (remote/nested sources), EXP-06 (command)
EXP-06 -> EXP-07 (template paths) -> EXP-08 (virtual environments)
EXP-07 -> EXP-09 (containers) -> EXP-10 (mounts)
EXP-05 + EXP-08 + EXP-10 -> EXP-11 (combined qualification)
execution-plan v15 + run-summary v14 + immutable contexts -> K8S-03 onward
operator demand + cluster/PVC/image path -> K8S-01 -> K8S-02 onward
assigned Mac runner -> successful macOS job BRW-01 -> BRW-02 -> BRW-03
A/P/O rollout + corpus + credentials -> APO-01 -> APO-02
runner administration + narrow seccomp profile -> RBK-01 -> RBK-02 -> RBK-03
```

SEC-07 remains blocked on native Windows runner support and a Windows host.
Published ACORN CLI v0.6.3 still assumes the POSIX socket for its local Docker
runner; Xylem's library pin remains 0.5.1. The completed environment phases
are not queued dependencies of Kubernetes worker/schema work.

## Direct dependency edges

- [Configuration expansion](features/configuration-variable-expansion.md)
  continues with ready EXP-05 and EXP-06. Accepted local-source behavior
  threads immutable invocation bindings through nested resolution. Separate
  branches in the graph still serialize
  overlapping code, tests, and docs. Accepted acquisition
  protections are prerequisites; SEC-07 Windows qualification is not a blocker.
- [Secure module acquisition](refactors/secure-module-acquisition.md) owns
  only remaining Windows qualification and audit; accepted platform evidence
  remains in `security/acquisition-platform-evidence.md`.
- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md)
  begins with operator demand and storage qualification; executable runtime
  phases preserve the already-landed immutable context and v15/v14 contracts.
- [A/P/O memory validation](features/memory-aware-scheduling.md) has no runtime
  implementation dependency and waits for a rollout decision and credentials.
- [Homebrew platform validation](features/homebrew-export.md) has an assigned
  online Mac runner; BRW-01 qualifies the published v1.0.7 formula, BRW-02
  requires both jobs, and BRW-03 verifies clean consumers. Release MR !5 is merged.
- [Rootless BuildKit runner](refactors/rootless-buildkit-runner.md) owns the
  dedicated runner security boundary and phased re-enablement of four
  temporarily skipped sandbox image jobs.

## Shared integration boundaries

| Boundary | Phase owners | Coordination rule |
|---|---|---|
| Manifests and `Cargo.lock` | K8S-02 | One dependency owner at a time; inspect feature tree and audit the accepted lockfile. |
| `crates/xylem-lib/src/config/` | EXP-05 through EXP-11, K8S-04 | One configuration owner at a time; preserve batching, memory, resume, source nesting, and sandbox opt-in. |
| Script, container, and template preparation | EXP-06 through EXP-10 | Resolve static fields before validation and preparation; reserve overlapping configuration, plan, and sandbox files too. |
| `crates/xylem-lib/src/execution.rs` and `commands/run/dispatch.rs` | K8S-03 | Preserve ordered fixed and memory-aware admission. |
| Plan, journal, run store, projection, telemetry, reporter, and TUI | K8S-08 | Allocate schema revisions serially and update durable readers and views together. |
| CLI roots and `commands/run/` | EXP-11, K8S-03/09/10/11/14 | One CLI integration owner at a time; preserve exit mapping and dry-run purity. |
| Delivery files and release images | SEC-07, RBK-02/03, BRW-02, K8S-15/16/17/18 | Serialize CI/release wiring; RBK-01 owns only the dedicated external runner. |
| User documentation and examples | EXP-05 through EXP-11, BRW-01/02/03, K8S-19 | Serialize shared files; document only accepted behavior and tested commands. |

EXP tests touch existing plan/completion projections without allocating a new
schema version. A discovered schema change needs a separate bounded phase and
Run state ownership. All phases serialize shared wiki reconciliation.

## Handoff checklist

Before a phase starts, record its exact write set, primary lock, baseline
commit, external prerequisites, and focused validation. At handoff, record
changed files, public/schema changes, validation outcomes, and the next
eligible phase. Remove completed phases and retire plans with no work left.
