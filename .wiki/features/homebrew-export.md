---
type: Plan
title: "Require Homebrew platform validation"
description: "Qualify macOS and require both tap jobs after the v1.0.7 publication, then verify consumer installation and upgrade."
difficulty: low
impact: high
package: [workspace, xylem-cli]
tags: [homebrew, release, distribution, macos, linux, packaging]
status: waiting-on-validation
execution: phased
phase_budget_hours: 5
---

# Goal

Require successful Linux and macOS Homebrew validation for future tap changes
and finish consumer install/upgrade qualification of the published formula.

# Current evidence

Reviewed on 2026-11-13 at clean Xylem baseline `e3f0e09`, including release CI,
formula generator/tests, tap template, installation/release docs, and live
read-only GitLab API results.

- [Tap merge request !5](https://code.ornl.gov/research-enablement/homebrew-xylem/-/merge_requests/5)
  published v1.0.7 and merged at `3de3d8e16489132b2e1e55a745f3d66227d232fe`.
  The [main formula](https://code.ornl.gov/research-enablement/homebrew-xylem/-/blob/main/Formula/xylem.rb)
  selects the immutable `xylem-cli-1.0.7.crate` archive with its SHA-256.
  Obsolete v1.0.3/!2 activation and merge tasks are removed.
- [Pipeline 1054610](https://code.ornl.gov/research-enablement/homebrew-xylem/-/pipelines/1054610)
  passed its [Linux job 4539890](https://code.ornl.gov/research-enablement/homebrew-xylem/-/jobs/4539890).
  Its [macOS job 4539891](https://code.ornl.gov/research-enablement/homebrew-xylem/-/jobs/4539891)
  remains manual with `allow_failure: true`; pipeline success does not qualify macOS.
- Both `scripts/homebrew/tap.gitlab-ci.yml` and live tap `main` retain those
  optional macOS settings. Runner 54626 (`RE Runner (macbook)`) was online and
  idle at review time; recheck availability before execution.
- Formula generation, release prerequisites, and tag-only publication are
  implemented; all seven `xtask` tests passed in this review. `README.md`
  already publishes the tap/install commands;
  `CONTRIBUTING.md` and `scripts/homebrew/README.md` describe release operation.
  Do not recreate this completed documentation work.
- Clean consumer installs/upgrades on both platforms and credential-setting
  qualification were not repeated. Protected/scoped publisher settings must
  be verified without reading or retaining secret values.

# Project code boundaries

- **Update:** `scripts/homebrew/tap.gitlab-ci.yml`, tap `.gitlab-ci.yml`, tap
  merge-check settings, and `scripts/homebrew/README.md` with validation evidence.
- **Create:** a tap CI-hardening merge request against current `main`.
- **Delete:** optional macOS validation settings after BRW-01 passes.

# Dependencies and ordering

BRW-01 validates the current published formula on the online Mac runner.
BRW-02 makes both jobs required through a new CI change; the release formula
has already merged. BRW-03 records clean consumer behavior and only updates
existing installation guidance if verification reveals a correction.

# Acceptance criteria

- Style, audit, source installation/upgrade, and offline formula tests pass on
  both declared Homebrew platforms for the selected immutable formula.
- Both platform jobs are required for subsequent tap merges, including the
  generated template so the next release cannot restore optional macOS checks.
- Protected/scoped publisher credential and tag-only access are verified using
  setting metadata, never secret values.
- Clean consumer install/upgrade results and recovery steps are documented.

# Execution phases

## BRW-01 — Complete macOS tap validation

- Status: waiting-on-validation
- Budget: 5 active hours maximum
- Prerequisites: confirm runner 54626 availability and pin the current formula commit; create an equivalent validation pipeline if the merged MR job cannot run.
- Exact write set: tap project 22654 macOS validation job/pipeline and `RE Runner (macbook)` host; `scripts/homebrew/README.md` for sanitized evidence.
- Primary integration lock: tap validation
- Outcome: Current formula passes macOS style, audit, source install, upgrade, and offline test.
- Focused validation: run the macOS formula-validation script against the pinned v1.0.7 formula or a newer explicitly recorded immutable release; record job URL, commit, and tool versions.
- Handoff: Preserve successful evidence, prune BRW-01, and make BRW-02 ready. Failures outside validation get a bounded repair phase.

## BRW-02 — Require both platform jobs for future tap merges

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: BRW-01 and tap-project maintenance access.
- Exact write set: `scripts/homebrew/tap.gitlab-ci.yml`, tap-project `.gitlab-ci.yml`, tap merge-check settings and a new CI-hardening merge request, publisher credential metadata, `scripts/homebrew/README.md`.
- Primary integration lock: Homebrew tap delivery
- Outcome: Linux/macOS checks pass and become required on the same formula; the generated template agrees with tap main. Publisher credential protection/scope is verified without exposing values.
- Focused validation: GitLab CI lint, both required jobs, merge-check metadata, tag-only publisher rules and protected credential metadata.
- Handoff: Record the CI-hardening commit/pipeline and settings evidence, then prune BRW-02. Do not reopen or recreate the merged release update.

## BRW-03 — Verify clean consumer installation and upgrade

- Status: waiting-on-validation
- Budget: 5 active hours maximum
- Prerequisites: BRW-02 and clean Linux/macOS Homebrew environments.
- Exact write set: clean Linux/macOS Homebrew validation environments, `scripts/homebrew/README.md`; `README.md` and `CONTRIBUTING.md` only for corrections demonstrated by validation.
- Primary integration lock: release documentation
- Outcome: Published tap install and upgrade work on both platforms, with durable commands, versions, and recovery evidence.
- Focused validation: `brew install`, `brew test`, and `brew upgrade` against the merged formula on each declared platform.
- Handoff: Preserve results in release documentation and retire the plan when all acceptance criteria pass.

Every phase also owns this plan, `.wiki/features/index.md`, `.wiki/index.md`,
`.wiki/plan-network.md`, and `.wiki/parallel-execution.md` for serialized lifecycle reconciliation.
