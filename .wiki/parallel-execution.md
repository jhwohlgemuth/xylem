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
| Interpolation | EXP-01 | Extract the bounded core without changing existing root behavior; reserve selection files and architecture docs. |
| Configuration | ARG-01/03, EXP-02/04/05, K8S-04 | Preserve typed validation, batching, memory fallback, resume, source nesting, and immutable context resolution. |
| Command preparation | EXP-06/07/08/09/10 | Resolve static fields before their owning checks; reserve shared configuration, script, container, sandbox, and test paths. |
| Run state | K8S-08 | Allocate versions serially; update plans, writers, recovery, reporters, schemas, and TUI together. |
| CLI | ARG-02/04, EXP-03/11, K8S-03/09/10/11/14 | One owner changes command registration, run orchestration, cancellation, or exit mapping at a time. |
| Manifest | K8S-02 | Inspect the feature tree and audit the resulting lockfile. |
| Delivery | SEC-07, RBK-02/03, BRW-02, K8S-15/16/17/18 | Serialize CI, image, and release wiring. |
| Documentation | ARG-02/03/04, EXP-02 through EXP-11, BRW-01/02/03, K8S-19 | Reserve overlapping files even when a phase has another primary lock; land verified behavior before user-facing updates. |

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

### Lane E: workflow invocation

- [ARG-01 through ARG-04](features/cli-variable-arguments.md): validated
  library overrides, named CLI invocation, ordered config declarations, then
  positional invocation. ARG-01 is ready; phases are sequential. Configuration
  and CLI edits serialize with their K8S owners, and documentation files with
  BRW/K8S documentation owners. External runner and measurement work can proceed
  in parallel when files remain disjoint.

External runner and measurement work can run beside repository phases while
its write set remains disjoint from CI and documentation integration.

### Lane F: configuration expansion

- [EXP-01 through EXP-11](features/configuration-variable-expansion.md):
  extract bounded interpolation first, then after ARG-02 deliver input/output
  paths, sources, template commands/filenames, and environment/container fields.
  EXP-01 and ARG-01 have disjoint code write sets; serialize their wiki edits.
  Later independent dependency branches share configuration, script, tests,
  and docs and therefore normally execute serially. ARG-03/04 may overlap only
  with a phase whose exact reserved write set is disjoint. EXP-11 waits for
  both field delivery and ARG-04, then qualifies the combined interface.

Reviewed on 2026-11-13 at `e3f0e09`, which commits the basename/root work. Preserve
the original immutable invocation bindings across early root application,
nested resolution, and final leaf application. No reverse ARG dependency on
EXP is introduced.

## Ready work and conditional starts

| Phase | Primary lock | Start condition |
|---|---|---|
| ARG-01 | Configuration | Ready at `e3f0e09`; reserve exact library/test files and planning metadata. |
| EXP-01 | Interpolation | Ready at `e3f0e09`; reserve selection/core files and serialize wiki edits with ARG-01. |
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
