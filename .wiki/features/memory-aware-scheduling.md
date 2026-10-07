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
- The pure estimator and FIFO admission model landed in commit `4df056c`; no
  configuration, plan, scheduler, CLI, or durable-state contract selects it yet.

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

These decisions authorize configuration and telemetry work behind explicit
boundaries for the measured A/P and A/P/O estimators. They do not authorize
different module revisions, reordered modules, out-of-range inputs, arbitrary
workflows, or concurrent production admission. Those cases retain fixed
batching. The fifth case makes a concurrent holdout gate possible, but does not
satisfy it by existing. Synthetic scheduler and configuration work can proceed
while the expanded measurements run.

# Next implementation slice

Add validated configuration and a versioned dry-run plan representation for
memory-aware intent. This slice selects a known estimator and explicit budget
without changing runtime admission, CLI flags, journal, report, or run-store
behavior.

## Handoff

- **Prerequisite landing and baseline:**
  `4df056cabbd3ca4c2c9661f8ac806ae2cbf81866`, which contains the pure memory
  model and supersedes the earlier measurement-evidence baseline.
- **Create:** `docs/schema/execution-plan-v9.schema.json` for the versioned plan
  contract. Do not mutate the version 8 schema.
- **Update:** `xylem-lib/src/memory/mod.rs` for the accepted estimator registry;
  `xylem-lib/src/config/mod.rs` and `xylem-lib/src/config/validation.rs` for the
  configuration type and semantic validation; `xylem-lib/src/config/tests/`
  for JSON/YAML and rejection cases; `xylem-lib/src/plan.rs` and
  `xylem-lib/src/tests/plan.rs` for deterministic version 9 plan output.
- **Delete:** None.
- **Lock scope:** claim the configuration lock and execution-plan schema only.
  Do not claim the CLI, scheduler, run-state, run-store, report, or general
  documentation locks.
- **Public configuration contract:** add nested `options.memory` intent with an
  explicit positive `budget` using `acorn_schema::hardware::Memory` and an
  `estimator` identity. Omitting it preserves fixed batching. Accept only known
  measured identities and reject a module order or declared revision that does
  not match the registry.
- **Public plan contract:** version the execution plan to 9 and include the
  validated memory budget and estimator selection. Do not emit per-input
  reservations or claim that memory-aware admission would run in this slice.
- **Dependencies:** add none. Add no host-memory detection and no CLI override.
- **Focused validation:**
  `cargo test --locked --package xylem-lib config::tests`,
  `cargo test --locked --package xylem-lib tests::plan`, and schema fixture
  validation.

Validation must cover JSON/YAML round trips, zero or invalid budgets, missing
budget or estimator fields, unknown identities, reordered or extra modules,
wrong or absent declared revisions, unchanged fixed-batching behavior when the
option is absent, deterministic plan serialization, and the version 9 schema.
The current leaf-script acquisition path retains a structured `Location`
revision but does not check out or attest that revision. This slice may validate
and report the declaration, but runtime estimator activation must remain
blocked until acquisition enforces the pin or an equivalent content identity.
Do not widen acquisition behavior while holding only the configuration and plan
locks.

## Landing order after configuration and planning

1. In parallel, inventory the fifth test case, add candidate holdout entries
   and compatible two-case mixes to both measurement manifests, and rerun the
   A/P and A/P/O gates without overwriting accepted evidence.
2. Enforce or attest pinned leaf-module revisions at the acquisition boundary.
3. Integrate admission into the existing fail-fast rolling input loop while
   preserving `options.batch`, `--threads`, cancellation, resume, and ordered
   outcomes. Keep real input concurrency at one unless the concurrent holdout
   gate is GO.
4. Version journal/report evidence for estimator identity, estimated bytes,
   budget, safety reserve, fallback reason, and optional observed peak.
5. Add operational documentation and rerun accepted workflows to compare
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
