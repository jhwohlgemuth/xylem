---
okf_version: "0.1"
execution: coordination
---

# Plan dependency network

Rechecked on 2026-10-31 against baseline `85c72df` and the current ENV-05/06
implementation diff. Arrows denote a
required contract or gate, not a whole-plan serialization. Current landed
baselines are execution-plan v14, run-summary v13, verified acquisition,
owned workspaces, sandbox providers, memory admission, and run storage.

## Delivery paths

```text
immutable contexts + policy-aware cancellation -> ENV-07/08/09 -> K8S-03 onward
shared script/configuration lock -> OCI-01 -> OCI-02 -> OCI-03 -> OCI-04
operator demand + cluster/PVC/image path -> K8S-01 -> K8S-02 onward
assigned Mac runner -> successful macOS job BRW-01 -> BRW-02 -> BRW-03
A/P/O rollout + corpus + credentials -> APO-01 -> APO-02
```

SEC-01 through SEC-06 are accepted; SEC-07 qualifies Windows and closes the
dependency gate after a published ACORN release and Windows host support
native Docker runner creation. The published ACORN CLI 0.5.1 path still assumes
the POSIX Docker socket. OCI-01 needs exclusive ownership of the shared
script/configuration files, not completion of unrelated platform
qualification. K8S worker and schema phases require the
immutable environment, cancellation, and durable-state contracts from ENV.

## Direct dependency edges

- [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md)
  preserves the landed sandbox, memory, reporter, resume, and run-store
  contracts while completing process results and immutable contexts.
- [Secure module acquisition](refactors/secure-module-acquisition.md) owns
  remaining platform qualification. Registry authorization and bounded OCI
  acquisition are accepted at `de0bd89`; Git transport at `1f5ed65`.
- [Local OCI archive qualification](features/local-oci-bundle-uris.md) extends
  landed directory support after the acquisition/configuration lock is free.
- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md)
  begins with operator demand and storage qualification; executable runtime
  phases follow the environment context and schema phases.
- [A/P/O memory validation](features/memory-aware-scheduling.md) has no runtime
  implementation dependency and waits for a rollout decision and credentials.
- [Homebrew release activation](features/homebrew-export.md) has an assigned
  online Mac runner; BRW-01 must diagnose canceled jobs before required jobs
  and merge.

## Shared integration boundaries

| Boundary | Phase owners | Coordination rule |
|---|---|---|
| Manifests and `Cargo.lock` | SEC-07, K8S-02 | One dependency owner at a time; inspect feature tree and audit the accepted lockfile. |
| `crates/xylem-lib/src/config/` | OCI-01, K8S-04 | Serialize writes while preserving batching, memory, resume, source nesting, and sandbox opt-in. |
| `crates/xylem-lib/src/script/` | ENV-09, OCI-01 | Environment phases own the host process adapter; local URI edits serialize on the exact shared file even when their primary lock differs. |
| `crates/xylem-lib/src/execution.rs` and `commands/run/dispatch.rs` | K8S-03 | Preserve ordered fixed and memory-aware admission. |
| Acquisition executor and cache | OCI-02 | One publication and transport owner at a time. |
| Plan, journal, run store, projection, telemetry, reporter, and TUI | ENV-07/08, K8S-08 | Allocate schema revisions serially and update durable readers and views together. |
| CLI roots and `commands/run/` | K8S-03/09/10/11/14 | One CLI integration owner at a time; preserve exit mapping and dry-run purity. |
| Sandbox providers | OCI-03 | Keep the landed provider boundary; run focused conformance for any preparation change. |
| Delivery files and release images | BRW-02, K8S-15/16/17/18 | Serialize CI/release wiring; runner administration owns no repository files. |
| User documentation and examples | BRW-03, OCI-04, ENV-09, K8S-19 | Document only accepted behavior and tested commands. |

## Handoff checklist

Before a phase starts, record its exact write set, primary lock, baseline
commit, external prerequisites, and focused validation. At handoff, record
changed files, public/schema changes, validation outcomes, and the next
eligible phase. Remove completed phases and retire plans with no work left.
