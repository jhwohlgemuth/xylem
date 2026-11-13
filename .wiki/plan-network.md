---
okf_version: "0.1"
execution: coordination
---

# Plan dependency network

Rechecked on 2026-11-01 against baseline `fcf4df3`. Arrows denote a
required contract or gate, not a whole-plan serialization. Current
baselines are execution-plan v15, run-summary v14, verified acquisition,
owned workspaces, sandbox providers, memory admission, and run storage.

CLI variable argument and configuration expansion edges were added on
2026-11-13 at `1c4a75e` plus the uncommitted basename/root-expansion work;
external gates for existing plans were not requalified by these additions.

## Delivery paths

```text
current configuration/variable resolution -> ARG-01 -> ARG-02 -> ARG-03 -> ARG-04
current bounded root expansion -> EXP-01
EXP-01 + ARG-02 -> EXP-02 (inputs), EXP-03 (output), EXP-04 (local sources), EXP-06 (command)
EXP-04 -> EXP-05 (remote/nested sources)
EXP-06 -> EXP-07 (template paths) -> EXP-08 (virtual environments)
EXP-07 -> EXP-09 (containers) -> EXP-10 (mounts)
EXP-02 + EXP-03 + EXP-05 + EXP-08 + EXP-10 + ARG-04 -> EXP-11 (combined qualification)
execution-plan v15 + run-summary v14 + immutable contexts -> K8S-03 onward
operator demand + cluster/PVC/image path -> K8S-01 -> K8S-02 onward
assigned Mac runner -> successful macOS job BRW-01 -> BRW-02 -> BRW-03
A/P/O rollout + corpus + credentials -> APO-01 -> APO-02
runner administration + narrow seccomp profile -> RBK-01 -> RBK-02 -> RBK-03
```

SEC-01 through SEC-06 are accepted; SEC-07 qualifies Windows and closes the
dependency gate after a published ACORN release and Windows host support
native Docker runner creation. The published ACORN CLI 0.5.1 path still assumes
the POSIX Docker socket. K8S worker and schema phases require the
immutable environment, cancellation, and durable-state contracts from ENV.

## Direct dependency edges

- [CLI variable arguments](features/cli-variable-arguments.md) has no external
  blocker: validated library overrides precede named CLI invocation, then
  ordered configuration declarations and positional invocation. Shared K8S
  files require serialization, not completion of the Kubernetes plan.
- [Configuration expansion](features/configuration-variable-expansion.md)
  starts with EXP-01 independently of ARG-01. Field phases depend on ARG-02
  for invocation precedence; EXP-04 threads those immutable bindings through
  nested/source resolution. ARG has no reverse dependency on EXP. Separate
  branches in the graph still serialize overlapping code, tests, and docs.
  EXP-11 also consumes positional binding from ARG-04. Accepted acquisition
  protections are prerequisites; SEC-07 Windows qualification is not a blocker.
- [Secure module acquisition](refactors/secure-module-acquisition.md) owns
  remaining platform qualification. Registry authorization and bounded OCI
  acquisition are accepted at `de0bd89`; Git transport at `1f5ed65`.
- [Kubernetes workflow execution](features/kubernetes-workflow-backend.md)
  begins with operator demand and storage qualification; executable runtime
  phases follow the environment context and schema phases.
- [A/P/O memory validation](features/memory-aware-scheduling.md) has no runtime
  implementation dependency and waits for a rollout decision and credentials.
- [Homebrew release activation](features/homebrew-export.md) has an assigned
  online Mac runner; BRW-01 must diagnose canceled jobs before required jobs
  and merge.
- [Rootless BuildKit runner](refactors/rootless-buildkit-runner.md) owns the
  dedicated runner security boundary and phased re-enablement of four
  temporarily skipped sandbox image jobs.

## Shared integration boundaries

| Boundary | Phase owners | Coordination rule |
|---|---|---|
| Manifests and `Cargo.lock` | SEC-07, K8S-02 | One dependency owner at a time; inspect feature tree and audit the accepted lockfile. |
| Bounded interpolation and selection roots | EXP-01 | Preserve current root behavior while extracting the shared core; ARG-01 code can proceed independently. |
| `crates/xylem-lib/src/config/` | ARG-01/03, EXP-02 through EXP-11, K8S-04 | One configuration owner at a time; preserve batching, memory, resume, source nesting, and sandbox opt-in. |
| Script, container, and template preparation | EXP-06 through EXP-10 | Resolve static fields before validation and preparation; reserve overlapping configuration, plan, and sandbox files too. |
| `crates/xylem-lib/src/execution.rs` and `commands/run/dispatch.rs` | K8S-03 | Preserve ordered fixed and memory-aware admission. |
| Plan, journal, run store, projection, telemetry, reporter, and TUI | K8S-08 | Allocate schema revisions serially and update durable readers and views together. |
| CLI roots and `commands/run/` | ARG-02/04, EXP-03/04/11, K8S-03/09/10/11/14 | One CLI integration owner at a time; preserve exit mapping and dry-run purity. |
| Delivery files and release images | RBK-02/03, BRW-02, K8S-15/16/17/18 | Serialize CI/release wiring; RBK-01 owns only the dedicated external runner. |
| User documentation and examples | ARG-02/03/04, EXP-02 through EXP-11, BRW-03, K8S-19 | Serialize shared files; document only accepted behavior and tested commands. |

EXP tests touch existing plan/completion projections without allocating a new
schema version. A discovered schema change needs a separate bounded phase and
Run state ownership. All phases serialize shared wiki reconciliation.

## Handoff checklist

Before a phase starts, record its exact write set, primary lock, baseline
commit, external prerequisites, and focused validation. At handoff, record
changed files, public/schema changes, validation outcomes, and the next
eligible phase. Remove completed phases and retire plans with no work left.
