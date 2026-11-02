---
okf_version: "0.1"
execution: coordination
---

# Parallel execution guide

This guide overlays phase write ownership on the [plan dependency
network](plan-network.md). Start one named phase only after its prerequisites,
exact write set, primary lock, baseline, and focused validation are confirmed.
Disjoint locks are necessary for parallel work; shared files still serialize.

## Integration locks

| Lock | Primary owners | Rule |
|---|---|---|
| Configuration | K8S-04 | Preserve typed validation, batching, memory fallback, resume, source nesting, and immutable context resolution. |
| Run state | K8S-08 | Allocate versions serially; update plans, writers, recovery, reporters, schemas, and TUI together. |
| CLI | K8S-03/09/10/11/14 | One owner changes command registration, run orchestration, cancellation, or exit mapping at a time. |
| Manifest | SEC-07, K8S-02 | Inspect the feature tree and audit the resulting lockfile. |
| Delivery | BRW-02, K8S-15/16/17/18 | Serialize CI, image, and release wiring. |
| Documentation | BRW-03, K8S-19 | Land final names and verified behavior before user-facing updates. |

## Work lanes

### Lane A: secure module acquisition

- [SEC-07](refactors/secure-module-acquisition.md): qualify Windows and close
  the dependency gate after a published ACORN release and Windows host support
  native Docker runner creation.

### Lane B: execution runtime

- [K8S-01 through K8S-19](features/kubernetes-workflow-backend.md): operator
  gates first; reusable runtime, protocol, controller, worker, image, and
  cluster phases follow accepted ENV prerequisites.

### Lane C: operational validation

- [BRW-01 through BRW-03](features/homebrew-export.md): complete macOS
  validation on the assigned runner, require both jobs and merge, then verify
  clean install and upgrade documentation.
- [APO-01 through APO-02](features/memory-aware-scheduling.md): pinned
  individual and concurrent holdout after rollout and credential gates.

External runner and measurement work can run beside repository phases while
its write set remains disjoint from CI and documentation integration.

## Safe initial batch

| Phase | Primary lock | Start condition |
|---|---|---|
| BRW-01 | Tap validation | Assigned `RE Runner (macbook)` remains online. |
| APO-01 | A/P/O measurement evidence | Rollout, corpus, and credentials are supplied. |
| K8S-01 | Kubernetes deployment assumptions | Operator demand, cluster, storage, and image owner are supplied. |

The process adapter uses the published and pinned ACORN 0.5.1 contract at
`3e915697d214eda33e0d4c52f03fb09f27d9d3e4`. The completed environment,
scheduler, cancellation, plan, source-validation, and local OCI qualification
phases released their locks.

## Agent start checklist

- Name one plan and phase, exact files or external systems, and its primary
  integration lock.
- Confirm baseline commit, prerequisite evidence, public contracts, and focused
  validation before writing.
- Stop and schedule a new bounded repair phase if a defect crosses the current
  outcome or lock.
- At handoff, record the accepted evidence, release the lock, and remove a
  completed phase from the active plan.
