---
okf_version: "0.1"
execution: coordination
---

# Plan dependency network

Rechecked on 2026-11-01 against baseline `83bc01c`, completed environment work, and OCI-01
implementation. Arrows denote a
required contract or gate, not a whole-plan serialization. Current
baselines are execution-plan v15, run-summary v14, verified acquisition,
owned workspaces, sandbox providers, memory admission, and run storage.

## Delivery paths

```text
execution-plan v15 + run-summary v14 + immutable contexts -> K8S-03 onward
accepted provider qualification -> OCI-04
operator demand + cluster/PVC/image path -> K8S-01 -> K8S-02 onward
assigned Mac runner -> successful macOS job BRW-01 -> BRW-02 -> BRW-03
A/P/O rollout + corpus + credentials -> APO-01 -> APO-02
```

SEC-01 through SEC-06 are accepted; SEC-07 qualifies Windows and closes the
dependency gate after a published ACORN release and Windows host support
native Docker runner creation. The published ACORN CLI 0.5.1 path still assumes
the POSIX Docker socket. Local archive provider qualification is accepted;
OCI-04 documents that contract without waiting for unrelated platform
qualification. K8S worker and schema phases require the
immutable environment, cancellation, and durable-state contracts from ENV.

## Direct dependency edges

- [Secure module acquisition](refactors/secure-module-acquisition.md) owns
  remaining platform qualification. Registry authorization and bounded OCI
  acquisition are accepted at `de0bd89`; Git transport at `1f5ed65`.
- [Local OCI archive qualification](features/local-oci-bundle-uris.md) has
  accepted source validation, extraction, local sandbox handoff, and provider
  preparation; OCI-04 documents the tested contract.
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
| `crates/xylem-lib/src/config/` | K8S-04 | Preserve batching, memory, resume, source nesting, and sandbox opt-in. |
| `crates/xylem-lib/src/execution.rs` and `commands/run/dispatch.rs` | K8S-03 | Preserve ordered fixed and memory-aware admission. |
| Plan, journal, run store, projection, telemetry, reporter, and TUI | K8S-08 | Allocate schema revisions serially and update durable readers and views together. |
| CLI roots and `commands/run/` | K8S-03/09/10/11/14 | One CLI integration owner at a time; preserve exit mapping and dry-run purity. |
| Delivery files and release images | BRW-02, K8S-15/16/17/18 | Serialize CI/release wiring; runner administration owns no repository files. |
| User documentation and examples | BRW-03, OCI-04, K8S-19 | Document only accepted behavior and tested commands. |

## Handoff checklist

Before a phase starts, record its exact write set, primary lock, baseline
commit, external prerequisites, and focused validation. At handoff, record
changed files, public/schema changes, validation outcomes, and the next
eligible phase. Remove completed phases and retire plans with no work left.
