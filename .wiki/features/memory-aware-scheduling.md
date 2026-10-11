---
type: Plan
title: "Add memory-aware adaptive scheduling"
description: "Measure per-input peak memory, validate an estimator, and admit workflow inputs within an explicit memory budget and batch ceiling."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, scheduler, memory, resources, telemetry, concurrency]
status: waiting-on-validation
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
- Journal/report schema v10 accepts optional per-input observed peaks and
  typed whole-run artifact finalization. On Linux,
  OCI-sandboxed runs with configured memory evidence collect the exact cgroup
  v2 `memory.peak` before input cleanup; unsupported and unsandboxed runs leave
  the observation null.
- Inputs may be files or directories of heterogeneous contents, and different modules can have different peak-memory curves for the same input.
- The pure estimator and FIFO admission model landed in commit `4df056c`.
  Validated configuration, read-only input sizing, execution-plan v11, and the
  fail-fast runtime admission integration and durable journal/report evidence
  are now implemented. Narrow Linux sandbox observation is implemented; the
  expanded fifth-case measurement and the rootless A/P comparison are
  complete, while broader observed-versus-estimated follow-up remains pending.
- The checked-in `memory-aware` example demonstrates a complete configuration
  with deterministic 1 KiB and 2 KiB inputs, budget-constrained reservations,
  and a 3 KiB out-of-range fixture that selects fixed batching. Its estimator
  is explicitly illustrative and workflow-specific.
- Failed workflow attempts are terminal and excluded from peak-memory
  observations. Prompt-policy rejection coverage retains the bounded upstream
  diagnostic without parsing its message or retrying the request, releases the
  reservation, and discards staged sandbox artifacts.

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
- Each measurement uses a generated per-run configuration. Concurrent subjects
  resolve `OUTPUT_DIRECTORY` beneath a distinct input-name subdirectory so the
  single multi-input Xylem invocation retains real concurrency without output
  or completion-marker collisions.
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
2.58 GB of LFS objects. A clean, Git-free copy on Oklahoma contains 139 regular
files and 2,588,922,236 logical bytes, with aggregate SHA-256
`cf899764c2243b2539d7878ebef5f368d8b1c34fc298286433e0cb92ce0c8f1f`.
Its eight checked UInt16 JPEG2000 NITF rasters total 11,666,456,576 decoded
bytes. The initial expanded candidates placed it in the holdout partition for
both pinned workflows and paired it with `wv02-medium-mid-high` in two-case
concurrent holdout mixes. Those runs did not alter either accepted v1 identity.

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

On 2026-10-08 and 2026-10-09, the expanded candidate gates completed without
overwriting either accepted evidence set. The sanitized NO-GO artifacts and
operator notes are retained under
`tools/memory-spike/evidence/oklahoma-2026-10-08-fifth-case/` and
`tools/memory-spike/evidence/oklahoma-2026-10-08-fifth-case-module-o/`.

- All five recorded repetitions of the new large holdout and all three
  concurrent holdout repetitions succeeded under both workflows, with zero OOM
  events. The large individual and concurrent peaks were approximately 64 GiB.
- Every candidate covered the individual held-out cases, but the new
  2,588,922,236-byte case was one extrapolation beyond the
  1,143,813,163-byte training maximum and each concurrent mix was
  underpredicted.
- For A/P, the input-byte candidate used a 3,753,780,717.734-byte intercept,
  29.681 bytes per input byte, and a 1.10 safety multiplier. Its median
  reservation reduction was 61.8% versus the 68,717,719,552-byte fixed worst
  case, but the decision was **NO-GO**.
- For A/P/O, the input-byte candidate used a 3,697,283,379.507-byte intercept,
  29.775 bytes per input byte, and a 1.10 safety multiplier. Its median
  reservation reduction was 61.8% versus the 68,715,487,232-byte fixed worst
  case, but the independent decision was also **NO-GO**.
- Neither accepted v1 estimator identity changes, and production input
  concurrency remains one.

On 2026-10-09, a second A/P gate promoted the fifth case to training, moved
`wv03-large-mid-low` to the holdout partition, and paired that holdout with
`wv02-medium-mid-high`. The sanitized artifacts and operator scope are retained
under `tools/memory-spike/evidence/oklahoma-2026-10-09-v2-concurrency/`.

- The `module-a-p-v2` decision is **GO** for the pinned, ordered Module A plus
  Module P workflow and the expanded 529,854,516–2,588,922,236-byte range.
- Both decoded-byte and input-byte candidates had zero individual or mix
  underpredictions and zero extrapolations. The analyzer selected decoded bytes
  by mean absolute error; the portable runtime configuration uses the accepted
  input-byte candidate because Xylem can size that feature generically.
- The input-byte candidate uses a 9,083,341,409.049-byte intercept, a
  23.320130499 slope, and a 1.15 safety multiplier. Its median reservation
  reduction is 49.2% versus the 68,718,493,696-byte fixed worst case.
- The two holdouts reserve 69,755,234,828 bytes together. All three recorded
  two-input runs succeeded with no OOM events and peaked at
  52,209,172,480–52,842,696,704 bytes.
- The conservative fixed-point coefficients and a 72 GiB, `batch: 2` example
  are documented in the configuration guide. A/P/O remains independently
  gated and must not reuse the A/P identity.

An A/P/O v2 run started on 2026-10-09 but was intentionally stopped during its
first recorded GE01 repetition, after warm-up, to prioritize A/P activation.
Its partial observations are diagnostic only: they produce no gate decision
and do not change the A/P/O v1 operating scope. Start a fresh A/P/O run when
that workflow becomes the active rollout target.

The controlled A/P activation canary then passed on Oklahoma. Run
`X110ljMr1R` selected `memory_aware` mode and retained exact reservations of
28,634,438,485 and 41,120,796,343 bytes. Both inputs entered `running` before the
first module terminal event, both Module A executions overlapped, both Module P
executions overlapped, and every module succeeded. The parent container's live
cgroup v2 `memory.peak` counter reached 52,214,681,600 bytes, within the earlier
concurrent holdout range and below the 69,755,234,828-byte combined
reservation. The durable evidence is retained under
`tools/memory-spike/evidence/oklahoma-2026-10-09-v2-canary/`.

Because the canary was an unsandboxed Xylem run inside the measurement
container, its per-input `observed_peak` values are correctly null. The
rootless native Linux OCI path has since passed synthetic collector conformance
on both supported runtimes.

That work is split into two independent gates:

1. **Collector conformance:** native rootless Podman run `FsifEzCO84` and Docker
   run `ZksTg4eTQ8` passed on Oklahoma. Their retained summaries and journals
   record per-input `observed_peak` values of 3,056 KB and 5,792 KB,
   respectively, against 131,072 KB reservations. Evidence is under
   `tools/memory-spike/evidence/oklahoma-2026-10-09-rootless-podman/` and
   `tools/memory-spike/evidence/oklahoma-2026-10-09-rootless-docker/`. This gate
   is closed: it proves cgroup discovery and durable telemetry without making
   a claim about the imagery estimator.
2. **Geospatial sandbox comparison:** the Linux/amd64 workload was published as
   `savannah.ornl.gov/research-enablement/xylem/geospatial@sha256:d0a3e444817303f418e5829ba805b945476932e456f966b8fc9d80eeee21ac03`.
   Its disk-backed publication path passed native rootless Docker and Podman
   conformance, including non-empty declared-output validation, atomic
   publication, durable artifact evidence, and positive cgroup peaks. Evidence
   is retained under
   `tools/memory-spike/evidence/oklahoma-2026-10-09-geospatial-sandbox-artifacts/`.
   Docker run `QxdoFw1crP` and Podman run `rbpUE3-vny` used the verified offline
   ASTER DEM package without weakening execution isolation. Both admitted the
   A/P holdouts concurrently, completed every module, recorded positive peaks
   below their reservations, and published all 42 declared artifacts
   atomically. This gate is closed.

On 2026-10-10, the accepted A/P workflow was rerun with the same current-source
binary under rootless Docker and Podman. Runs `UAGcVSj6wQ` and `dZyvyDx2Ot`
admitted both holdouts concurrently, completed every module, published all 42
artifacts atomically, and recorded positive per-input peaks. Docker observed
23,651,516 KB and 35,663,024 KB; Podman observed 22,498,308 KB and 35,400,180
KB. All four peaks remained below their respective 27,963,319 KB and
40,157,028 KB enforced reservations. Evidence is retained under
`tools/memory-spike/evidence/oklahoma-2026-10-10-operational-rerun/`. The
observed-versus-estimated operational gate is closed.

The checked-in rootless preflight now verifies native x86_64 Linux, a local
rootless Docker or Podman runtime, cgroup v2 memory and CPU enforcement, the
hardened IPC and process flags, a canonical Linux/amd64 workload, and
caller-selected available capacity. CI retains the synthetic summary and
journal as job artifacts. The reusable workload definition is staged under
`sandbox/runner/geospatial/`; CI publishes canonical Savannah and GHCR digest
references.

These decisions establish evidence only for the measured A/P and A/P/O
revisions, order, and input range. They do not make those combinations a
production registry or prevent another workflow from declaring its own model.
Each estimator definition carries its complete coefficients and calibrated
range; its author is responsible for making the identity and evidence match the
configured modules. The expanded v2 A/P gate now supports measured two-input
admission; the A/P/O v1 result remains single-input evidence only.

# Landed configuration, planning, runtime, and durable-evidence slices

The configuration and execution-plan slice is implemented on the sandbox
lifecycle baseline `b7caa55`. It adds `options.memory` with an explicit positive
`Memory` budget and a complete module-agnostic fixed-point estimator. Any
module list can use the option; validation covers the estimator identity,
coefficients, safety multiplier, and calibrated input range without inspecting
module names, order, count, or revisions.

Execution-plan schema v11 adds nullable artifact and memory intent and ordered per-input
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

Journal and run-summary schema v10 introduced artifact finalization and memory
evidence; the current v11 schema retains the estimator identity, configured
budget, safety multiplier, and ordered per-input base estimate, safety reserve,
enforced reservation, or structured fallback. A separate one-shot journal
event can record a positive observed peak after an input starts; underprediction
is retained as evidence and does not change the run result. Linux OCI sandbox
inputs with configured memory evidence read the input container's cgroup v2
`memory.peak` after workflow execution and before cleanup. Collection remains
optional: an unavailable platform peak emits a warning and leaves the value
null, while a collected peak must be durably journaled. Unconfigured and
unsandboxed summaries record no observed peak. Host-memory detection and a CLI
override are not active.

On 2026-10-09, the ignored live Linux collector test passed on Oklahoma's
cgroup v2 hierarchy and returned a positive exact `memory.peak`. The native
sandbox conformance test, which requires a durable positive observed peak on
Linux, then passed against local rootless Podman and Docker daemons. The
retained evidence closes the synthetic collector gate for both runtimes.

## Remaining landing order

The measured A/P estimator, pinned operational canary, Docker/Podman rootless
comparison, and reusable memory-aware example are landed. The retained runs
prove two concurrent admissions, positive per-input peaks, and atomic
publication through the disk-backed path. The example adds deterministic
in-range reservations and out-of-range fallback coverage.

On 2026-10-10, the prompt-policy failure phase landed without adding
provider-specific message matching or automatic retries. A local memory-aware
regression proves one attempt, bounded durable diagnostics, fail-fast
admission, and a null observed peak. The ignored native conformance test passed
against rootless Docker and Podman on Oklahoma: successful controls published
artifacts and recorded positive peaks, while policy-rejected runs failed the
input, discarded artifacts, and emitted no `input_memory_observed` event.
Evidence is retained under
`tools/memory-spike/evidence/oklahoma-2026-10-10-prompt-policy-failure/`.
Provider-specific adapters should use the provider's structured error code,
not diagnostic message text. Invalid or policy-blocked input must be reviewed
and corrected before resubmission. For OpenAI workloads, follow the official
[error recovery guidance](https://developers.openai.com/api/docs/guides/agents-api/errors)
and give reasoning models a clear goal, constraints, output contract,
completion criteria, and verification steps as described in the
[reasoning prompt guidance](https://developers.openai.com/api/docs/guides/reasoning#advice-on-prompting).

The accepted A/P workflow operational rerun also landed on 2026-10-10. Its
current-source Docker and Podman runs retained positive observed peaks below
every enforced reservation. The expanded A/P/O operator wrapper and manual
Oklahoma instructions are checked in; that gate still requires owner-supplied
Earthdata credentials.

1. Run the expanded A/P/O measurement when that workflow becomes the active
   rollout target. Keep its production input concurrency at one until a fresh
   run's individual and concurrent holdout gates are GO.

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
- Inject a prompt-policy rejection and verify that the input terminates, its
  reservation is released exactly once, no automatic retry occurs, the bounded
  diagnostic is retained, and the attempt is excluded from estimator evidence.
- Validate and dry-run the checked-in memory-aware example in CI, including its
  in-range reservations and out-of-range fixed-batch fallback.

# Acceptance criteria

- The spike documents measurements, prediction error, safety margin, and a go/no-go decision.
- Production scheduling is added only after an estimator is accepted for representative workloads.
- Active reservations never exceed the explicit memory budget or fixed batch ceiling.
- Oversized inputs receive a clear policy-driven result rather than an implicit OOM attempt.
- Scheduling is deterministic, rolling, and starvation-resistant.
- Unconfigured workflows and inputs outside the estimator range safely fall back to fixed batching.
- The checked-in example demonstrates the complete memory-aware configuration
  without presenting its illustrative coefficients as a universal model.
- Prompt-policy rejections fail closed, release reservations, do not publish
  artifacts, and are never retried through automatic prompt rewriting.
- Operational documentation explains budgeting, headroom, telemetry, and estimator limitations.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
