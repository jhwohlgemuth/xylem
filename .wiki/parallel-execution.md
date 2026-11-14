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
| Configuration | EXP-05, K8S-04 | Preserve typed validation, batching, memory fallback, resume, source nesting, and immutable context resolution. |
| Command preparation | EXP-06/07/08/09/10 | Resolve static fields before their owning checks; reserve shared configuration, script, container, sandbox, and test paths. |
| Run state | K8S-08 | Allocate versions serially; update plans, writers, recovery, reporters, schemas, and TUI together. |
| CLI | EXP-11, K8S-03/09/10/11/14 | One owner changes command registration, run orchestration, cancellation, or exit mapping at a time. |
| Manifest | K8S-02 | Inspect the feature tree and audit the resulting lockfile. |
| Delivery | SEC-07, RBK-02/03, BRW-02, K8S-15/16/17/18 | Serialize CI, image, and release wiring. |
| Documentation | EXP-05 through EXP-11, BRW-01/02/03, K8S-19 | Reserve overlapping files even when a phase has another primary lock; land verified behavior before user-facing updates. |

## Work lanes

### Lane A: secure module acquisition

- [SEC-07](refactors/secure-module-acquisition.md): qualify Windows and close
  the dependency gate after a published ACORN release and Windows host support
  native Docker runner creation.

### Lane B: execution runtime

- [K8S-01 through K8S-19](features/kubernetes-workflow-backend.md): operator
  gates first; reusable runtime, protocol, controller, worker, image, and
  cluster phases preserve the accepted local context/cancellation and v15/v14 contracts.

### Lane C: operational validation

- [BRW-01 through BRW-03](features/homebrew-export.md): complete macOS
  validation of the already-published v1.0.7 formula, require both jobs through
  a new CI change, then verify clean consumer installs/upgrades.
- [APO-01 through APO-02](features/memory-aware-scheduling.md): pinned
  individual and concurrent holdout after rollout and credential gates.

### Lane D: delivery infrastructure

- [RBK-01 through RBK-03](refactors/rootless-buildkit-runner.md): provision a
  dedicated narrowly confined rootless BuildKit runner, validate build-only
  jobs, then restore protected dual-registry publication.

External runner and measurement work can run beside repository phases while
its write set remains disjoint from CI and documentation integration.

### Lane E: configuration expansion

- [EXP-05 through EXP-11](features/configuration-variable-expansion.md):
  deliver remote/nested sources, template commands/filenames, and environment/container fields
  after accepted core, input, output, and local-source expansion.
  Later independent dependency branches share configuration, script, tests,
  and docs and therefore normally execute serially. EXP-11 waits for field
  delivery, then qualifies the combined interface.

Rechecked on 2026-11-13 at clean `9f85428` with named and positional bindings
committed. Preserve
the original immutable invocation bindings across early root application,
nested resolution, and final leaf application.

## Ready work and conditional starts

| Phase | Primary lock | Start condition |
|---|---|---|
| EXP-05 | Configuration | Ready; reserve remote/nested source resolution and acquisition tests. |
| EXP-06 | Command preparation | Ready; reserve script, plan, and command tests. |
| RBK-01 | Runner security policy | Blocked until administration access and an amd64 host maintenance window are confirmed. |
| BRW-01 | Tap validation | Waiting on validation; runner 54626 was online at review. Recheck and pin the current formula/job before starting. |
| APO-01 | A/P/O measurement evidence | Blocked until rollout, corpus, and credentials are supplied. |
| K8S-01 | Kubernetes deployment assumptions | Blocked until operator demand, cluster, storage, and image owner are supplied. |

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
