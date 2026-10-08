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
- Workflow `resources` remain string values verified as host requirements.
  Quantitative admission is configured independently through validated
  `options.memory`.
- Journal/report schema v9 accepts optional per-input observed peaks, but no
  platform collector is active; accepted spike evidence therefore remains
  separate from production telemetry.
- Inputs may be files or directories of heterogeneous contents, and different modules can have different peak-memory curves for the same input.
- The pure estimator and FIFO admission model landed in commit `4df056c`.
  Validated configuration, read-only input sizing, execution-plan v10, and the
  fail-fast runtime admission integration and durable journal/report evidence
  are now implemented. Platform observation and production measurement
  follow-up remain pending.

# Project code boundaries

- **Update:** execution scheduling, resource/configuration types and validation, run orchestration, journal telemetry integration, documentation, and tests.
- **Create:** a measurement spike report and fixtures, memory-size parsing, estimator types, weighted admission scheduling, oversize-input policy, and deterministic synthetic scheduler tests.
- **Delete:** None until the spike validates a replacement for fixed-only scheduling. The fixed `batch` contract remains as a hard ceiling and fallback.

# Dependencies and ordering

This implementation is tracked by [work item #13](https://code.ornl.gov/research-enablement/xylem/-/work_items/13). Production scheduling depends on the existing `options.batch` input ceiling and deterministic orchestration. Observed-versus-estimated telemetry uses the durable execution journal. The A/P and A/P/O measurements below provide reviewed estimator examples; the configuration contract remains module-agnostic so other workflows can supply their own calibrated models.

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

The accepted evidence uses four test-case projects and the pinned Module A/P/O
repositories located in `GSED/common/imagery-processing` on 2026-10-02.
Oklahoma has clean, Git-free copies of those four inputs. That corpus provides
three training cases and one holdout case, but cannot form a two-case holdout
mix. The first real run was therefore scoped to Module A plus Module P. A later
A/P/O run used owner-supplied Earthdata credentials and a measurement wrapper
that rejects missing or empty Module O outputs, closing the pinned Module O
revision's worker-error reporting gap for the spike.

On 2026-10-07, a fifth private test case,
`GSED/common/imagery-processing/test-cases/test-case-wv02-large-low-mid`, became
available at commit `fc4cd2647bdc8ca9c561c44bb038ab71bb9c1cb7`. Its README
describes four WorldView-2 PAN/MUL tile pairs, and GitLab reports approximately
2.58 GB of LFS objects. This case is not part of either accepted manifest or
decision. Before using it, materialize a Git-free copy, record its exact file
count, input bytes, and checked raster features, and run it as a candidate
second holdout under both pinned workflows. If its individual observation is
compatible, pair it with `wv02-medium-mid-high` in a two-case holdout mix. Keep
production input concurrency at one until both the individual and concurrent
holdout gates pass. If the accepted coefficients or measured range change,
publish a new estimator identity rather than changing either `*-v1` identity
in place.

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

These decisions establish evidence only for the measured A/P and A/P/O
revisions, order, and input range. They do not make those combinations a
production registry or prevent another workflow from declaring its own model.
Each estimator definition carries its complete coefficients and calibrated
range; its author is responsible for making the identity and evidence match the
configured modules. The fifth case makes a concurrent holdout gate possible for
the measured imagery workflows, but does not satisfy it by existing. Synthetic
scheduler work can proceed while the expanded measurements run.

# Landed configuration, planning, runtime, and durable-evidence slices

The configuration and execution-plan slice is implemented on the sandbox
lifecycle baseline `b7caa55`. It adds `options.memory` with an explicit positive
`Memory` budget and a complete module-agnostic fixed-point estimator. Any
module list can use the option; validation covers the estimator identity,
coefficients, safety multiplier, and calibrated input range without inspecting
module names, order, count, or revisions.

Execution-plan schema v10 adds nullable memory intent and ordered per-input
sizing decisions. Xylem recursively totals logical regular-file bytes without
following symbolic links, calculates checked reservations for inputs inside the
calibrated range, and rejects a valid reservation larger than the run budget
before work begins. Any missing, unsafe, zero, or out-of-range input selects
`fixed_fallback` for the whole run and produces one warning during a real run.
Otherwise, the existing fail-fast rolling scheduler admits the FIFO prefix that
fits the memory budget, `options.batch`, and `--threads`; it releases each
reservation once at terminal input completion and admits no new work after a
failure or cancellation. An unconfigured plan records `memory: null` and keeps
the original fixed batching path.

Journal and run-summary schema v9 records the estimator identity, configured
budget, safety multiplier, and ordered per-input base estimate, safety reserve,
enforced reservation, or structured fallback. A separate one-shot journal
event can record a positive observed peak after an input starts; underprediction
is retained as evidence and does not change the run result. Unconfigured
summaries record `memory: null`. Host-memory detection, a CLI override, and a
platform peak collector are not active.

## Remaining landing order

1. Inventory the fifth test case, add candidate holdout entries
   and compatible two-case mixes to both measurement manifests, and rerun the
   A/P and A/P/O gates without overwriting accepted evidence.
   The generic scheduler accepts any valid caller-supplied estimator, but keep
   the reviewed imagery profile's real input concurrency at one unless its
   concurrent holdout gate is GO.
2. Add a narrowly gated platform collector for optional per-input peak memory
   without changing the portable reservation contract.
3. Add operational examples and rerun accepted workflows to compare
   estimates with observed peaks.

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
- Fall back to fixed batching when no estimator is configured or an input is outside its calibrated range.

## Telemetry

Record estimator identity, estimated bytes, configured budget, safety reserve, and observed peak when available. Keep platform-specific observation optional and separate from the portable scheduling contract.

# Validation

- Use a pure synthetic scheduler model to test budgets, releases, fairness, deterministic ordering, batch interaction, zero/overflow values, and oversized work.
- Property-test that the sum of active reservations never exceeds the configured budget and active count never exceeds `batch`.
- Test estimation and fallback for files, directories, arbitrary module combinations, and missing metadata.
- Run representative production workloads and compare OOM rate, utilization, elapsed time, and prediction error against fixed batching.
- Validate Linux first if that is the supported production environment; gate platform-specific peak-memory observation narrowly.

# Acceptance criteria

- The spike documents measurements, prediction error, safety margin, and a go/no-go decision.
- Production scheduling is added only after an estimator is accepted for representative workloads.
- Active reservations never exceed the explicit memory budget or fixed batch ceiling.
- Oversized inputs receive a clear policy-driven result rather than an implicit OOM attempt.
- Scheduling is deterministic, rolling, and starvation-resistant.
- Unconfigured workflows and inputs outside the estimator range safely fall back to fixed batching.
- Operational documentation explains budgeting, headroom, telemetry, and estimator limitations.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
