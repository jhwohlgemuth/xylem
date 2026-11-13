---
type: Plan
title: "Qualify acquisition on Windows"
description: "Complete native Windows acquisition qualification and record the accepted dependency audit."
difficulty: low
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, acquisition, windows, validation]
status: waiting-on-validation
execution: phased
phase_budget_hours: 5
---

# Goal

Qualify the landed acquisition contracts on native Windows, preserving the
existing verified cache, transport policy, cancellation, and containment rules.

# Current evidence

Reviewed on 2026-11-13 at clean baseline `e3f0e09` using acquisition code/tests,
`Cargo.toml`, `Cargo.lock`, CI, and the
[durable platform evidence](../../security/acquisition-platform-evidence.md).
The full library suite passed again: 336 passed, two ignored.

- HTTP, Git, OCI, offline cache, bounded transport, concurrency, and containment
  implementation is landed. Linux amd64/arm64 and macOS acceptance is preserved
  in the durable platform evidence; their completed phase histories are pruned.
- Xylem still pins ACORN libraries 0.5.1 at
  `3e915697d214eda33e0d4c52f03fb09f27d9d3e4`; this is distinct from the runner
  administration CLI release.
- Published [ACORN v0.6.3](https://code.ornl.gov/research-enablement/acorn/-/releases/v0.6.3)
  exists. Its [runner creation code](https://code.ornl.gov/research-enablement/acorn/-/blob/v0.6.3/crates/acorn-cli/src/commands/create/runner/mod.rs)
  still mounts the POSIX Docker socket into a Linux-style runner; its
  [executor socket implementation](https://code.ornl.gov/research-enablement/acorn/-/blob/v0.6.3/crates/acorn-lib/src/io/mod.rs)
  explicitly assumes a Linux runner image. Windows-host remote Linux execution
  is not evidence of native Windows-container runner support.
- No native Windows acceptance appears in the repository. A suitable Windows
  host and supported runner-creation release remain blockers. No Windows host
  or new advisory audit was exercised during this review.

# Project code boundaries

- **Update:** `.gitlab-ci.yml` only for Windows qualification wiring and
  `security/acquisition-platform-evidence.md` for results.
- **Create:** native Windows runner registration and host configuration.
- **Delete:** None.

# Dependencies and ordering

A supported native Windows runner path and available Windows-container host
precede SEC-07. Existing Linux/macOS acquisition behavior is an accepted
baseline, not a dependency on a retired plan. Coordinate CI wiring with delivery
owners. No Xylem dependency upgrade is implied by selecting a newer runner CLI;
a required dependency or runtime repair gets its own bounded phase.

# Acceptance and validation

- Windows tests cover HTTP/Git/OCI, offline/cache integrity, concurrent immutable
  publication, cancellation, containment, and permissions where supported.
- Record host, runtime, CLI, toolchain, source revision, commands, skips, and
  results in the durable platform evidence. A skipped required platform case
  is an unresolved acceptance gap.
- Record `cargo tree -e features` and `cargo audit` against the accepted lockfile.

# Execution phases

## SEC-07 — Qualify Windows and close the dependency gate

- Status: blocked
- Budget: 5 active hours maximum
- Prerequisites: a released native Windows Docker runner-creation path and an available Windows host configured for Windows containers; ACORN v0.6.3 does not yet establish that support.
- Exact write set: native Windows runner host and GitLab Xylem project runner registration, `.gitlab-ci.yml`, `security/acquisition-platform-evidence.md`; this plan, `.wiki/refactors/index.md`, `.wiki/index.md`, `.wiki/plan-network.md`, `.wiki/parallel-execution.md` for reconciliation.
- Primary integration lock: acquisition platform matrix
- Outcome: Windows acquisition qualification and the accepted dependency audit have durable evidence.
- Focused validation: native Windows acquisition tests/job, `cargo tree -e features`, and `cargo audit`; verify the selected released CLI's runner help before registering it.
- Handoff: Preserve platform and audit results outside `.wiki`, then retire this plan and its index/network/lane entries. Any defect requiring implementation or dependency changes becomes a separate bounded repair phase.
