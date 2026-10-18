---
type: Plan
title: "Harden remote module acquisition"
description: "Qualify the landed acquisition contracts across OCI, Git, cancellation, and supported platforms."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, downloads, git, oci, checksum, offline, supply-chain]
status: in-progress
execution: phased
phase_budget_hours: 5
---

# Goal

Finish qualification of the landed remote acquisition path. Prove the OCI and
Git transports, cancellation, cache publication, and platform behavior against
the existing fail-closed policy before retiring the plan.

# Current evidence

Committed baseline rechecked on 2026-10-18 at `97d5203`. The acquisition and
CI worktree changes are uncommitted and require their own validation and review.

- Runtime integration landed on 2026-10-12: remote documents, binaries, and Git modules use immutable acquisition plans, bounded executors, verified cache entries, and durable provenance.
- Existing tests cover policy and identity validation, bounded HTTP responses, offline cache behavior, cache corruption, SSH environment isolation, and escaping Git symlinks.
- Concurrent acquisition of the same immutable identity now has a focused HTTP/cache publication test, and Git origin normalization has direct coverage.
- The broader OCI registry, Git transport, resource-bound, cancellation, and
  cross-platform qualification matrix is not yet recorded.
- Xylem now pins all ACORN crates to accepted revision `0c256bae0ff42da1c96de50fd525146a95965c66`; the lockfile resolves `quick-xml` 0.41.0 and the advisory scan passes.
- OCI module acquisition now accepts digest-pinned ORAS artifacts as file trees, applies the exact-host policy and existing staging/tree ceilings, and records the OCI manifest digest and extracted tree evidence. ORAS is a host runtime prerequisite.

# Project code boundaries

- **Update:** acquisition implementation and qualification tests only where a
  demonstrated gap requires repair; CI platform jobs and audit evidence.
- **Create:** the uncommitted `oci_registry.rs` helper if qualification accepts
  it; keep focused registry and Git fixtures inline in the named test file.
- **Delete:** None; the earlier advisory exceptions are already removed.

# Dependencies and ordering

Builds on the existing typed-error, validated-planning, managed-run-root, and
workflow-environment contracts. Coordinate with sandbox policy: acquisition
owns source validation and materialization, while sandboxing owns the
capability boundary for network, Git, credentials, and subprocess execution.

# Remaining work

The committed implementation already plans and materializes remote documents,
binaries, Git trees, and OCI trees with typed policy, verified cache identities,
and durable evidence. Do not reimplement these contracts. The current worktree
contains uncommitted OCI registry and qualification changes; review them against
`97d5203` and retain only verified outcomes. The accepted ACORN revision and
`quick-xml` 0.41.0 have cleared the prior advisory blocker.

# Handoff

- Verified baseline: committed `97d5203` with acquisition plans and v14/v13 evidence.
- Changed files: the current uncommitted acquisition, manifest, CI, Makefile,
  `oci_registry.rs`, and `scripts/install-oras-ci.sh` changes span SEC-01,
  SEC-02, SEC-04, and SEC-05; partition and review them before accepting a
  phase result.
- Validation run: this wiki review validated planning structure and local
  baseline claims; the uncommitted product changes have no accepted test result
  recorded here.
- Remaining defect: OCI registry authorization, ORAS bounds, and the platform
  matrix have not yet been accepted.
- Next action: compare the current acquisition diff with `97d5203`, run the
  focused SEC-01 registry cases, and record the accepted result.

# Validation

Qualify the remaining registry authorization/redirect policy, ORAS bounds and
cancellation, Git transport and cleanup, and platform matrix. Existing HTTP,
offline, cache-integrity, and dry-run tests remain regression gates. A failure
that cannot fit its phase becomes a separate bounded repair phase.

# Acceptance criteria

- OCI registry authorization and any token-service or redirect host respect the
  effective host policy; ORAS output, staging, timeout, and cancellation remain
  bounded, and interrupted work leaves no published cache entry.
- Git HTTPS and permitted SSH cover pinned identity, origin, credential and
  implicit-fetch denial, resource limits, cancellation, and cleanup.
- Same-identity concurrent acquisition and containment/permissions behavior
  pass on the supported platform matrix, with the accepted ACORN feature tree
  and advisory audit recorded.

# Execution phases

## SEC-01 — Qualify OCI registry identity and authorization

- Status: in-progress
- Budget: 5 active hours maximum
- Prerequisites: committed baseline `97d5203`; reconcile the current uncommitted acquisition changes and claim the manifest lock for the dependency diff
- Exact write set: `Cargo.toml`, `Cargo.lock`, `crates/xylem-lib/Cargo.toml`, `crates/xylem-lib/src/acquisition/mod.rs`, `crates/xylem-lib/src/acquisition/oci_registry.rs`, `crates/xylem-lib/src/acquisition/policy.rs`, `crates/xylem-lib/src/acquisition/tests.rs`
- Primary integration lock: acquisition policy
- Outcome: Registry manifest identity, authorization, token-service hosts, and redirects obey the declared host policy.
- Focused validation: `cargo test -p xylem-lib acquisition::tests --locked` with local registry and denied-host cases.
- Handoff: Record accepted commit, touched files, test results, and remaining defects; remove SEC-01 only after the registry cases pass.

## SEC-02 — Bound ORAS and interrupted cache publication

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: SEC-01
- Exact write set: `crates/xylem-lib/src/acquisition/oci.rs`, `crates/xylem-lib/src/acquisition/executor.rs`, `crates/xylem-lib/src/acquisition/tests.rs`
- Primary integration lock: acquisition executor
- Outcome: OCI staging, output, time, cancellation, cleanup, and same-identity publication are bounded under failure.
- Focused validation: focused OCI acquisition tests with limits, cancellation, offline cache reuse, and concurrent requests.
- Handoff: Record the bounded cases and cache evidence; remove SEC-02 when accepted.

## SEC-03 — Qualify Git transport and cleanup

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: acquisition executor lock released after SEC-02
- Exact write set: `crates/xylem-lib/src/acquisition/git.rs`, `crates/xylem-lib/src/acquisition/tests.rs`
- Primary integration lock: acquisition executor
- Outcome: HTTPS and permitted SSH preserve pinned identity, origin, limits, implicit-fetch denial, cancellation, and atomic cleanup.
- Focused validation: focused Git acquisition tests using local HTTPS/SSH fixtures and interrupted clone cases.
- Handoff: Record transport coverage and observed gaps; remove SEC-03 when accepted.

## SEC-04 — Qualify Linux x86_64 acquisition

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: SEC-02 and SEC-03, schedulable Linux x86_64 runner
- Exact write set: `.gitlab-ci.yml`, `scripts/install-oras-ci.sh`, Linux x86_64 runner
- Primary integration lock: acquisition platform matrix
- Outcome: OCI/Git containment, permissions, cache concurrency, and interruption pass on Linux x86_64.
- Focused validation: x86_64 acquisition CI job and `cargo test -p xylem-lib acquisition::tests --locked`.
- Handoff: Record host, revision, ORAS version, and job link; remove SEC-04 when accepted.

## SEC-05 — Qualify Linux arm64 acquisition

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: SEC-04 and schedulable Linux arm64 runner
- Exact write set: `.gitlab-ci.yml`, `scripts/install-oras-ci.sh`, Linux arm64 runner
- Primary integration lock: acquisition platform matrix
- Outcome: The same acquisition matrix passes natively on Linux arm64.
- Focused validation: arm64 acquisition CI job and `cargo test -p xylem-lib acquisition::tests --locked`.
- Handoff: Record arm64 job and platform result; remove SEC-05 when accepted.

## SEC-06 — Qualify macOS acquisition

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: SEC-03 and a macOS host with ORAS
- Exact write set: macOS host, `crates/xylem-lib/src/acquisition/tests.rs`
- Primary integration lock: acquisition platform matrix
- Outcome: Git, OCI, cache concurrency, containment, and permissions pass on macOS.
- Focused validation: `cargo test -p xylem-lib acquisition::tests --locked` on macOS.
- Handoff: Record host and revision; remove SEC-06 when accepted.

## SEC-07 — Qualify Windows and close the dependency gate

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: SEC-05, SEC-06, and a schedulable Windows runner
- Exact write set: Windows runner, `Cargo.toml`, `Cargo.lock`, `deny.toml`
- Primary integration lock: acquisition platform matrix
- Outcome: Windows acquisition tests pass and the accepted ACORN feature tree and advisory audit are recorded.
- Focused validation: Windows acquisition job, `cargo tree -e features`, and `cargo audit` against the accepted lockfile.
- Handoff: Preserve platform and audit results outside `.wiki`, then retire this plan and its index, network, and lane entries.
