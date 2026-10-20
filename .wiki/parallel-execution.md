---
okf_version: "0.1"
---

# Parallel execution guide

This guide overlays phase write ownership on the [plan dependency
network](plan-network.md). Start one named phase only after its prerequisites,
exact write set, primary lock, baseline, and focused validation are confirmed.
Disjoint locks are necessary for parallel work; shared files still serialize.

## Integration locks

| Lock | Primary owners | Rule |
|---|---|---|
| Acquisition policy and executor | OCI-01/02 | Finish or hand off the current acquisition change before another phase writes its files. |
| Configuration | ENV-02/03/04, OCI-01, K8S-04 | Preserve typed validation, batching, memory fallback, resume, and source nesting. |
| Process adapter | ENV-01/08 | Other phases consume the accepted ACORN process boundary. |
| Scheduler | ENV-05 | Preserve ordered fixed and memory-aware admission. |
| Run state | ENV-07/08, K8S-08 | Allocate versions serially; update plans, writers, recovery, reporters, schemas, and TUI together. |
| CLI | ENV-06, K8S-03/09/10/11/14 | One owner changes command registration, run orchestration, cancellation, or exit mapping at a time. |
| Sandbox preparation | OCI-03 | Preserve provider conformance and lifecycle evidence. |
| Manifest | SEC-07, ENV-06, K8S-02 | Inspect the feature tree and audit the resulting lockfile. |
| Delivery | SEC-04/05, BRW-02, K8S-15/16/17/18 | Serialize CI, image, and release wiring. |
| Documentation | BRW-03, OCI-04, ENV-09, K8S-19 | Land final names and verified behavior before user-facing updates. |

## Work lanes

### Lane A: secure module acquisition

- [SEC-04 through SEC-07](refactors/secure-module-acquisition.md): qualify the
  remaining platform matrix.
- [OCI-01 through OCI-04](features/local-oci-bundle-uris.md): archive and
  sandbox work starts when its shared lock is released; it need not await
  unrelated Git or platform results.

### Lane B: execution runtime

- [ENV-01 through ENV-09](refactors/workflow-environment-and-concurrency.md):
  process results, immutable contexts, scheduling, cancellation, durable state,
  and parent-environment cleanup.
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
| SEC-04 | Acquisition platform matrix | Run the Linux x86_64 acquisition CI job against accepted Git revision `1f5ed65`. |
| ENV-01 | Process adapter | Pinned ACORN 0.4.0 process contract is available. |
| BRW-01 | Tap validation | Assigned `RE Runner (macbook)` remains online. |
| APO-01 | A/P/O measurement evidence | Rollout, corpus, and credentials are supplied. |
| K8S-01 | Kubernetes deployment assumptions | Operator demand, cluster, storage, and image owner are supplied. |

## Agent start checklist

- Name one plan and phase, exact files or external systems, and its primary
  integration lock.
- Confirm baseline commit, prerequisite evidence, public contracts, and focused
  validation before writing.
- Stop and schedule a new bounded repair phase if a defect crosses the current
  outcome or lock.
- At handoff, record the accepted evidence, release the lock, and remove a
  completed phase from the active plan.
