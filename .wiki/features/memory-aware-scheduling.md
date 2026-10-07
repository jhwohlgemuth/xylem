---
type: Plan
title: "Add memory-aware adaptive scheduling"
description: "Measure per-input peak memory, validate an estimator, and admit workflow inputs within an explicit memory budget and batch ceiling."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, scheduler, memory, resources, telemetry, concurrency]
status: in-progress
priority: next
---

# Goal

Vary workflow input concurrency according to estimated peak resident memory while keeping total reservations within an explicit budget. Reduce out-of-memory failures for mixed-size imagery without treating output size as an unverified proxy for RAM use.

# Current evidence

- The current execution model assigns equal scheduling weight to every input regardless of input size or module behavior.
- `--threads` bounds Rayon workers and `options.batch` bounds active inputs, but neither represents memory demand.
- Workflow `resources` are string values verified as host requirements; they do not provide a quantitative memory budget or estimator.
- Xylem's durable runtime does not yet record peak child-process memory; the
  accepted spike evidence remains separate from production telemetry.
- Inputs may be files or directories of heterogeneous contents, and different modules can have different peak-memory curves for the same input.

# Project code boundaries

- **Update:** execution scheduling, resource/configuration types and validation, run orchestration, journal telemetry integration, documentation, and tests.
- **Create:** a measurement spike report and fixtures, memory-size parsing, estimator types, weighted admission scheduling, oversize-input policy, and deterministic synthetic scheduler tests.
- **Delete:** None until the spike validates a replacement for fixed-only scheduling. The fixed `batch` contract remains as a hard ceiling and fallback.

# Dependencies and ordering

This implementation is tracked by [work item #13](https://code.ornl.gov/research-enablement/xylem/-/work_items/13). Production scheduling depends on the existing `options.batch` input ceiling and deterministic orchestration. Observed-versus-estimated telemetry uses the durable execution journal. The estimator gate is now satisfied for the exact A/P and A/P/O scopes below; runtime work must retain those boundaries without adding dependencies.

# Measurement spike status

The non-publishable `xylem-memory-spike` workspace tool now implements the
measurement gate under `tools/memory-spike/`. It does not change Xylem runtime,
configuration, plan, journal, or report contracts.

- Measurements require Linux cgroup v2 with an explicitly delegated writable
  subtree and collect exact process-tree `memory.peak` plus sampled anonymous,
  file-backed, and current memory.
- `tools/memory-spike/docker-probe.sh` provides a tracked arm64/amd64 Ubuntu
  Docker preflight for the process-tree probe. Passing it validates host
  accounting only and does not affect the estimator decision.
- On 2026-10-02, Vergil (Ubuntu 24.04.5 LTS, aarch64, cgroup v2) passed the
  32 MiB parent plus 32 MiB child probe: exact `memory.peak` was 68,096,000
  bytes against a 67,108,864-byte minimum.
- On 2026-10-02, Oklahoma (Ubuntu 22.04.5 LTS, x86_64, cgroup v2) passed the
  same probe: exact `memory.peak` was 68,165,632 bytes against the same
  67,108,864-byte minimum.
- The versioned manifest records opaque case IDs, exact ordered module
  identities, explicit training/holdout assignments, input bytes, and checked
  raster dimensions, bands, bit depths, formats, and compression modes.
- Individual cases receive one warm-up and five recorded runs. Concurrent
  holdout mixes receive one warm-up and three recorded runs.
- Analysis compares input-byte, decoded-byte, and conservative size-bucket
  estimators. Training determines a 5%-step safety multiplier plus one extra
  5% guard; holdout extrapolation is rejected.
- A candidate passes only with no failed/OOM observations, no held-out case or
  mix underprediction, and at least 20% lower median reservation than the
  observed fixed worst case. Synthetic observations validate the harness but
  never affect the decision.

The four current test-case projects and the Module A/P/O repositories were
located in `GSED/common/imagery-processing` and pinned on 2026-10-02. Oklahoma
has clean, Git-free copies of the four inputs. This is enough for three
training cases and one holdout case, but not a two-case holdout mix. The first
real run was therefore scoped to Module A plus Module P. A later A/P/O run used
owner-supplied Earthdata credentials and a measurement wrapper that rejects
missing or empty Module O outputs, closing the pinned Module O revision's
worker-error reporting gap for the spike.

On 2026-10-03, the scoped Module A plus Module P run completed on Oklahoma with
five recorded repetitions per case. The sanitized artifacts and operator
sign-off are retained under
`tools/memory-spike/evidence/oklahoma-2026-10-03/`.

- The gate decision is **GO** for the pinned, ordered Module A plus Module P
  workflow and the measured 529,854,516–1,143,813,163 input-byte range.
- The selected input-byte estimator uses a 3,754,352,537.535-byte intercept,
  29.680 bytes per input byte, and a 1.10 safety multiplier.
- It produced zero training or held-out underpredictions, no extrapolations,
  and a 27.6% median reservation reduction versus the 36,276,289,536-byte
  fixed worst case.
- Non-baseline five-run peak ranges were 0.004%–0.012% of each case median.

On 2026-10-06, the pinned Module A, Module P, and Module O workflow completed
on Oklahoma with five recorded repetitions per case. The sanitized artifacts
and operator scope are retained under
`tools/memory-spike/evidence/oklahoma-2026-10-06-module-o/`.

- The independent gate decision is **GO** for Module A revision
  `2396c36644a89170bde2da2f29a65a364b21338c`, Module P revision
  `a80b166736e8174171e398653fda925d3c124fae`, and Module O revision
  `404ab3bccb64e0c7823f5eb577cc491f695bbf2b`, in that order.
- The selected input-byte estimator uses a 3,696,455,730.646-byte intercept,
  29.776 bytes per input byte, and a 1.10 safety multiplier over the same
  529,854,516–1,143,813,163 input-byte range.
- It produced zero training or held-out underpredictions, no extrapolations,
  and a 27.7% median reservation reduction versus the 36,363,116,544-byte
  fixed worst case.
- Non-baseline five-run peak ranges were 0.002%–0.242% of each case median.

These decisions authorize implementing the scheduler and telemetry behind
explicit configuration boundaries for the measured A/P and A/P/O estimators.
They do not authorize different module revisions, reordered modules,
out-of-range inputs, or arbitrary workflows. Those cases retain fixed batching.
Because the approved corpus cannot form a two-case holdout mix, keep production
input concurrency at one until a concurrent holdout mix validates additive
reservations; the synthetic scheduler and configuration work can proceed now.

# Next implementation slice

Start with a library-only estimator and admission model. This slice proves the
portable arithmetic and scheduling invariants without changing configuration,
CLI, execution-plan, journal, report, or run-store schemas.

## Handoff

- **Prerequisite landing:** the 2026-10-06 Module O measurement harness and
  evidence currently in the worktree must land before runtime work begins. Its
  landing commit is pending because these changes are intentionally uncommitted.
- **Baseline commit:** `12b84b9fc4902b8162626e44cdb76f7af3c4d239`.
- **Create:** `xylem-lib/src/memory.rs` for checked byte quantities, estimator
  calculations, reservation decisions, deterministic FIFO admission, and
  colocated synthetic tests.
- **Update:** `xylem-lib/src/lib.rs` to expose the new library module.
- **Delete:** None.
- **Lock scope:** claim only the new memory scheduler model and the library
  module list. Do not claim the configuration, CLI, scheduler-integration,
  run-state, run-store, or documentation locks in this slice.
- **Public contract:** add only the documented library estimator and admission
  API. Add no serialized configuration fields, CLI flags, plan fields, journal
  records, report fields, or automatic host-memory detection.
- **Dependencies:** add none. Use checked integer or fixed-point arithmetic and
  always round reservations up.
- **Focused validation:** `cargo test --locked --package xylem-lib memory` plus
  `cargo test --locked --package xylem-lib execution::tests`.

The model must accept an explicit positive budget, positive reservations, and
the existing `InputBatch` ceiling. It must preserve input order, release a
reservation exactly once on terminal completion, reject an input whose
reservation exceeds the entire budget, and return a deterministic admission
decision without reading host state or starting work. Unsupported estimator
identities and out-of-range inputs are typed fallback results, not guessed
reservations.

Required synthetic cases are: exact-budget fit, one-byte-over-budget,
zero/overflow rejection, release followed by immediate admission, batch lower
than memory capacity, memory capacity lower than batch, FIFO fairness, stable
ordering, unsupported estimator fallback, and oversized-input rejection.
Property tests must show that admitted reservation bytes never exceed the
budget and admitted count never exceeds `batch` for any transition sequence.

## Landing order after the pure model

1. Add validated configuration and dry-run plan types that select only known,
   measured estimator identities and require an explicit memory budget.
2. Integrate admission into the existing fail-fast rolling input loop while
   preserving `options.batch`, `--threads`, cancellation, resume, and ordered
   outcomes. Keep real input concurrency at one.
3. Version journal/report evidence for estimator identity, estimated bytes,
   budget, safety reserve, fallback reason, and optional observed peak.
4. Add operational documentation and rerun the accepted real workflows to
   compare estimates with observed peaks.

# Recommendation

## Measurement gate

- Measure peak resident memory for representative modules, image formats, dimensions, bit depths, compression modes, and directory contents.
- Record input features, module/version identity, concurrency, platform, peak memory, and success/failure.
- Compare candidate models such as a fixed base plus per-input-byte multiplier, dimension-aware formulas, and conservative buckets.
- Publish model error, safety margin, and cases where no useful prediction is possible.
- Stop with a documented no-go decision if the estimator cannot protect the budget reliably.

Output size may be recorded as a feature, but it must not become the reservation estimate without measurement evidence.

## Budget and admission

- Require an explicit run memory budget or an explicitly selected fraction of detected capacity, with reserved headroom for Xylem, the OS, and unrelated work.
- Estimate and reserve memory before starting an input. Admit the next deterministic candidate only when its reservation fits both remaining memory and the fixed `batch` ceiling.
- Release reservations when the input reaches a terminal state and immediately consider waiting work.
- Define fairness so a large input cannot starve indefinitely behind smaller work.
- Reject an estimate larger than the full budget with an actionable diagnostic unless a separate run-alone policy is explicitly enabled.
- Fall back to fixed batching when no estimator is configured or validated for a module.

## Telemetry

Record estimator identity, estimated bytes, configured budget, safety reserve, and observed peak when available. Keep platform-specific observation optional and separate from the portable scheduling contract.

# Validation

- Use a pure synthetic scheduler model to test budgets, releases, fairness, deterministic ordering, batch interaction, zero/overflow values, and oversized work.
- Property-test that the sum of active reservations never exceeds the configured budget and active count never exceeds `batch`.
- Test estimator selection and fallback for files, directories, unknown modules, and missing metadata.
- Run representative production workloads and compare OOM rate, utilization, elapsed time, and prediction error against fixed batching.
- Validate Linux first if that is the supported production environment; gate platform-specific peak-memory observation narrowly.

# Acceptance criteria

- The spike documents measurements, prediction error, safety margin, and a go/no-go decision.
- Production scheduling is added only after an estimator is accepted for representative workloads.
- Active reservations never exceed the explicit memory budget or fixed batch ceiling.
- Oversized inputs receive a clear policy-driven result rather than an implicit OOM attempt.
- Scheduling is deterministic, rolling, and starvation-resistant.
- Unsupported modules safely fall back to fixed batching.
- Operational documentation explains budgeting, headroom, telemetry, and estimator limitations.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
