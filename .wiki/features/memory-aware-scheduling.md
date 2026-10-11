---
type: Plan
title: "Validate memory-aware scheduling for the A/P/O rollout"
description: "Run the remaining credentialed A/P/O holdout gate before enabling concurrent production admission for that workflow."
difficulty: low
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, scheduler, memory, resources, telemetry, concurrency, validation]
status: waiting-on-validation
---

# Goal

Qualify concurrent memory-aware admission for the pinned Module A/P/O imagery
workflow before that workflow becomes an active production rollout target.
Keep its production input concurrency at one until fresh individual and
concurrent holdout measurements pass.

# Current evidence

Rechecked on 2026-10-11.

- The pure fixed-point estimator and FIFO admission queue are implemented.
- `options.memory` validation, deterministic input sizing, fixed-batch fallback,
  and budget-plus-`options.batch` admission are active.
- Execution-plan v12 and journal/run-summary v11 retain estimator identity,
  reservations, fallback reasons, and optional observed peaks.
- Linux OCI-sandboxed runs collect cgroup v2 `memory.peak` before cleanup when
  memory evidence is configured. Unsupported and unsandboxed runs leave the
  observation null.
- The checked-in `memory-aware` example covers in-range admission and
  out-of-range fallback without presenting its coefficients as universal.
- The pinned A/P v2 estimator passed the expanded fifth-case individual and
  concurrent holdout gates. Current-source Docker and Podman operational runs
  recorded positive peaks below every reservation.
- Prompt-policy rejection coverage proves one attempt, fail-fast admission,
  released reservations, discarded artifacts, bounded diagnostics, and no
  invalid observed-peak event.

The implementation and retained measurement history live in
[`tools/memory-spike/README.md`](../../tools/memory-spike/README.md). This page
tracks only the remaining rollout gate rather than duplicating those reports.

# Recommendation

Keep the shipped scheduler and accepted A/P estimator unchanged. Treat A/P/O
as a rollout-specific validation gate, not as a reason to generalize the
estimator or add runtime policy.

# Validation

The expanded A/P/O wrapper and Oklahoma operator instructions are checked in.
The run requires owner-supplied Earthdata credentials, which must never be
committed or written into durable run evidence.

When A/P/O becomes the active rollout target:

1. Run the individual and concurrent holdout measurements on the pinned
   workflow and fifth-case corpus.
2. Verify that every observation succeeds, no estimate underpredicts a held-out
   peak, no case extrapolates beyond the calibrated range, and the concurrent
   mix stays within the configured budget.
3. Retain sanitized manifests, summaries, journals, analysis, and operator
   sign-off under `tools/memory-spike/evidence/`.
4. Promote the accepted estimator identity and concurrency only after the gate
   reports GO. Record a no-go result without weakening the fallback or budget
   contracts.

# Project code boundaries

- **Update:** the A/P/O evidence directory, measurement report, and operator
  documentation. Runtime code changes require a new implementation plan.
- **Create:** sanitized A/P/O evidence for the new run.
- **Delete:** None.

# Dependencies and ordering

This validation is tracked by [work item #13](https://code.ornl.gov/research-enablement/xylem/-/work_items/13).
It waits for an active A/P/O rollout decision, access to the pinned input
corpus, and owner-provided Earthdata credentials. It does not block the shipped
module-agnostic scheduler or the accepted A/P estimator.

# Acceptance criteria

- Sanitized evidence records the exact workflow identity, corpus identity,
  platform, individual observations, concurrent mix observations, prediction
  error, safety margin, and go/no-go decision.
- No credential or credential-bearing value appears in committed evidence,
  journals, summaries, logs, or commands.
- A GO decision has no failed or OOM observation, no held-out underprediction,
  no extrapolation, and no budget violation.
- Until GO, the A/P/O production profile keeps input concurrency at one.
- A no-go result leaves fixed batching and existing memory admission behavior
  unchanged.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
