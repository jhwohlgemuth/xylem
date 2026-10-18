---
type: Plan
title: "Activate the Homebrew release export"
description: "Finish macOS validation, require both tap jobs, and verify installation from the v1.0.3 release."
difficulty: low
impact: high
package: [workspace, xylem-cli]
tags: [homebrew, release, distribution, macos, linux, packaging]
status: waiting-on-validation
execution: phased
phase_budget_hours: 5
---

# Goal

Activate the implemented Homebrew publication path and verify that macOS and
Linux users can install and upgrade Xylem through the official tap.

# Current evidence

Local release configuration and tap project rechecked on 2026-10-18.

- Formula generation, tag-only tap merge-request creation, tap CI, offline formula testing, and upgrade coverage are implemented.
- The immutable v1.0.0 archive cannot compile with Homebrew Rust 1.98 because it locks `ethnum` 1.5.2. The published v1.0.3 archive locks the compatible `ethnum` 1.5.3 release.
- The v1.0.3 release pipeline published both crates and opened tap merge request `!2`. Its Linux validation job passes.
- The v1.0.3 formula passes `brew style`, `brew audit --strict`, a macOS source install with Homebrew Rust 1.98.1, and `brew test`.
- Tap [merge request !2](https://code.ornl.gov/research-enablement/homebrew-xylem/-/merge_requests/2)
  is open and mergeable at commit
  `94ac912432d61472276792f27e38967389fc2451`.
  [Pipeline 1017726](https://code.ornl.gov/research-enablement/homebrew-xylem/-/pipelines/1017726)
  reports success because Linux passed and macOS is allowed to fail. The
  `RE Runner (macbook)` group runner is online, tagged `macos`, and assigned to
  the tap; its most recent [macOS job 4396906](https://code.ornl.gov/research-enablement/homebrew-xylem/-/jobs/4396906)
  was canceled. A successful
  macOS validation job and required two-platform checks remain.

# Project code boundaries

- **Update:** `README.md`, `CONTRIBUTING.md`, and release documentation after the first successful publication.
- **Create:** required merge checks in the tap project settings; no new runner is needed.
- **Delete:** None.

# Dependencies and ordering

The immutable v1.0.3 tag and artifacts were produced from the release workflow. The tap merge request must pass both platform jobs before merge. Update the public installation documentation with the verified command after activation.

# Recommendation

1. Diagnose the canceled macOS job and rerun validation on the assigned Mac runner.
2. Require both platform jobs and review and merge the generated formula after they pass.
3. Verify fresh installation and upgrade on clean Linux and macOS Homebrew environments.
4. Publish the tested tap command and operational recovery steps.

## Retry and require macOS validation

Diagnose the canceled macOS job and retry validation on the already-online
`RE Runner (macbook)`. Do not use a prior job ID as the retry target because
each retry creates a new job.

After the runner has completed a job successfully, remove `when: manual` and
`allow_failure: true` from `homebrew-macos` in
`scripts/homebrew/tap.gitlab-ci.yml` and from the tap merge request's
`.gitlab-ci.yml`. Require the resulting pipeline before merge.

# Validation

- Confirm the protected credential is unavailable to branch and merge-request pipelines.
- Confirm an incomplete or failed release cannot open or update the tap merge request.
- Install from the rendered formula in clean macOS and Linux Homebrew environments and run the formula test offline.
- Upgrade from the previous formula version and verify that `brew upgrade xylem` selects the new immutable release.

# Acceptance criteria

- A documented `brew install <tap>/xylem` command installs the tagged Xylem release on every declared Homebrew platform.
- Audit, style, install, and formula tests are required before the tap change can merge.
- The v1.0.3 tap update passes both platform jobs and merges through review.
- The publication credential is protected and scoped only to the tap update operation.

# Execution phases

## BRW-01 — Complete macOS tap validation

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: the assigned `RE Runner (macbook)` remains online and merge request !2 still targets the intended immutable release
- Exact write set: tap merge request !2 validation job, `RE Runner (macbook)` host
- Primary integration lock: tap validation
- Outcome: Diagnose the canceled jobs and obtain a successful macOS formula validation on the assigned runner.
- Focused validation: tap `homebrew-macos` job passes style, audit, source install, upgrade, and `brew test` on the pinned formula.
- Handoff: Record the new job URL and runner result; remove BRW-01 when macOS validation passes.

## BRW-02 — Require both tap jobs and merge the verified formula

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: BRW-01 and a fresh check that tap merge request !2 still targets the intended immutable release
- Exact write set: `scripts/homebrew/tap.gitlab-ci.yml`, tap-project `.gitlab-ci.yml`, tap merge request !2
- Primary integration lock: Homebrew tap delivery
- Outcome: Linux and macOS validation are required, pass on the same formula, and the reviewed tap change merges.
- Focused validation: GitLab CI lint and the tap merge-request pipeline with both platform jobs passing.
- Handoff: Record the merged commit and pipeline; remove BRW-02 after merge.

## BRW-03 — Verify installation and publish the command

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: BRW-02 and clean Linux and macOS Homebrew environments
- Exact write set: `README.md`, `CONTRIBUTING.md`, `scripts/homebrew/README.md`
- Primary integration lock: release documentation
- Outcome: Fresh install and upgrade use the merged tap formula, and the verified command and recovery steps are documented.
- Focused validation: `brew install`, `brew test`, and `brew upgrade` on each declared platform.
- Handoff: Record tested platforms and commands in release documentation, then retire this plan and its index, network, and lane entries.
