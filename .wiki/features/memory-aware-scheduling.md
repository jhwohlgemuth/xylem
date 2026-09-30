---
type: Plan
title: "Add memory-aware adaptive scheduling"
description: "Measure per-input peak memory, validate an estimator, and admit workflow inputs within an explicit memory budget and batch ceiling."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, scheduler, memory, resources, telemetry, concurrency]
status: planned
---

# Goal

Vary workflow input concurrency according to estimated peak resident memory while keeping total reservations within an explicit budget. Reduce out-of-memory failures for mixed-size imagery without treating output size as an unverified proxy for RAM use.

# Current evidence

- The current execution model assigns equal scheduling weight to every input regardless of input size or module behavior.
- `--threads` bounds Rayon workers and `options.batch` bounds active inputs, but neither represents memory demand.
- Workflow `resources` are string values verified as host requirements; they do not provide a quantitative memory budget or estimator.
- Xylem does not record peak child-process memory, so there is no repository evidence that the example input-to-output size ratios predict peak RAM.
- Inputs may be files or directories of heterogeneous contents, and different modules can have different peak-memory curves for the same input.

# Project code boundaries

- **Update:** execution scheduling, resource/configuration types and validation, run orchestration, journal telemetry integration, documentation, and tests.
- **Create:** a measurement spike report and fixtures, memory-size parsing, estimator types, weighted admission scheduling, oversize-input policy, and deterministic synthetic scheduler tests.
- **Delete:** None until the spike validates a replacement for fixed-only scheduling. The fixed `batch` contract remains as a hard ceiling and fallback.

# Dependencies and ordering

This implementation is tracked by [work item #13](https://code.ornl.gov/research-enablement/xylem/-/work_items/13). Measurement may begin independently, but production scheduling depends on the existing `options.batch` input ceiling and deterministic orchestration. Observed-versus-estimated telemetry uses the durable execution journal. Do not add runtime dependencies or public configuration until the spike produces an accepted estimator and error bound.

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
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
