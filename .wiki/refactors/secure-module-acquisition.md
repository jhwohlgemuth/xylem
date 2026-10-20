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

The acquisition and CI changes were committed at `de0bd89` on 2026-10-19.
SEC-01 and SEC-02 were rechecked against that revision on 2026-10-19. SEC-03
HTTPS qualification was committed at `1f5ed65` on 2026-10-19. SEC-04 and
SEC-05 Linux qualification is recorded in
[`security/acquisition-platform-evidence.md`](../../security/acquisition-platform-evidence.md).

- Runtime integration landed on 2026-10-12: remote documents, binaries, and Git modules use immutable acquisition plans, bounded executors, verified cache entries, and durable provenance.
- Existing tests cover policy and identity validation, bounded HTTP responses, offline cache behavior, cache corruption, SSH environment isolation, and escaping Git symlinks.
- Concurrent acquisition of the same immutable identity now has a focused HTTP/cache publication test, and Git origin normalization has direct coverage.
- Local OCI and Git acquisition qualification passes on macOS arm64. Linux
  amd64 and native arm64 acquisition CI each passed 33 tests at `b1e9297`.
- Xylem now pins all ACORN crates to accepted revision `0c256bae0ff42da1c96de50fd525146a95965c66`; the lockfile resolves `quick-xml` 0.41.0 and the advisory scan passes.
- OCI module acquisition now accepts digest-pinned ORAS artifacts as file trees, applies the exact-host policy and existing staging/tree ceilings, and records the OCI manifest digest and extracted tree evidence. ORAS is a host runtime prerequisite.

# Project code boundaries

- **Update:** acquisition implementation and qualification tests only where a
  demonstrated gap requires repair; CI platform jobs and audit evidence.
- **Create:** None for the remaining phases; Linux qualification evidence is
  preserved in `security/acquisition-platform-evidence.md`.
- **Delete:** None; the earlier advisory exceptions are already removed.

# Dependencies and ordering

Builds on the existing typed-error, validated-planning, managed-run-root, and
workflow-environment contracts. Coordinate with sandbox policy: acquisition
owns source validation and materialization, while sandboxing owns the
capability boundary for network, Git, credentials, and subprocess execution.

# Remaining work

The committed implementation already plans and materializes remote documents,
binaries, Git trees, and OCI trees with typed policy, verified cache identities,
and durable evidence. The accepted ACORN revision and `quick-xml` 0.41.0 have
cleared the prior advisory blocker. The local Git HTTPS fixture and both Linux
architectures are validated; macOS and Windows qualification remain.

# Handoff

- Accepted SEC-01/02 revision: `de0bd89`; registry policy, OCI extraction, cache
  publication, manifests, and focused tests are committed there.
- Validation run: `cargo test -p xylem-lib acquisition::tests --locked --offline`
  passed on macOS arm64 on 2026-10-19 (32 passed, 0 failed). Registry challenge,
  token realm, redirect, size, offline reuse, timeout, and concurrent OCI cache
  cases passed.
- Accepted SEC-03 revision: `1f5ed65`; `git.rs` now forces certificate
  verification, and the local HTTPS fixture uses a test CA and dev-only
  `rustls`. `cargo test -p xylem-lib acquisition::tests --locked --offline`
  passed on macOS arm64 (33 passed, 0 failed).
- SEC-04/05 were accepted on 2026-10-20 at `b1e9297`: the amd64 and native
  arm64 acquisition jobs each passed 33 tests with Rust 1.98.0 and ORAS 1.3.4.
  Runner names, image digests, version provenance, and job links are preserved
  in the [durable platform evidence](../../security/acquisition-platform-evidence.md).
- Remaining work: qualify macOS under SEC-06 and Windows plus the dependency
  audit under SEC-07.
- Next action: run SEC-06 on macOS arm64.

# Validation

Qualify the platform matrix. Existing Git, OCI, HTTP, offline, cache-integrity,
and dry-run tests remain regression gates. A failure that cannot fit its phase
becomes a separate bounded repair phase.

## Worktree evidence, 2026-10-19

- OCI registry requests verify the pinned manifest and each blob before local
  ORAS extraction. Bearer realms and redirects are checked against the exact
  host allowlist; cross-origin redirects do not receive registry authorization.
  Tests cover permitted and denied destinations, malformed file layers, size
  limits, offline reuse, ORAS timeout, and concurrent cache publication.
- Git smart HTTP and SSH fixtures cover pinned commits, origin checks,
  isolation from inherited Git and SSH settings, tree limits, escaping
  symlinks, executable permissions, redirect denial, timeouts, cleanup, and
  concurrent cache publication. A local HTTPS fixture now exercises a pinned
  commit through verified TLS with an isolated test CA; the focused acquisition
  suite passes with 33 tests on macOS arm64. SEC-03 is accepted at `1f5ed65`.
- macOS arm64: `cargo test -p xylem-lib --lib --offline --quiet` passed with
  290 passed and 2 ignored; the focused acquisition suite passed with 32 tests.
- Linux x86_64: the focused acquisition suite passed with 32 tests in a Rust
  1.99 container using ORAS 1.3.4 and OpenSSH. The first Rust 1.96 attempt
  exposed that pinned ACORN crates require Rust 1.98, so the workspace minimum
  was corrected to 1.98.
- GitLab acquisition jobs are configured for Linux x86_64 and native arm64;
  both ran in main pipeline
  [1034428](https://code.ornl.gov/research-enablement/xylem/-/pipelines/1034428)
  at `87bea9e` but stopped before tests: the shared CI image has Rust 1.96,
  while Xylem and pinned ACORN crates require 1.98. The amd64 job
  [4448171](https://code.ornl.gov/research-enablement/xylem/-/jobs/4448171)
  ran on RE Runner (oklahoma) and verified ORAS 1.3.4; the arm64 job
  [4448172](https://code.ornl.gov/research-enablement/xylem/-/jobs/4448172)
  hit the same compiler gate. The arm64 qualification was still pending at
  that revision; Windows remains SEC-07.
- The subsequent pipeline
  [1034462](https://code.ornl.gov/research-enablement/xylem/-/pipelines/1034462)
  passed both acquisition jobs at `b1e9297` with Rust 1.98.0, ORAS 1.3.4,
  and 33 passing tests per architecture. SEC-04 and SEC-05 are accepted; the
  image digest and Rust version evidence is recorded outside this plan.
- The available macOS host is Darwin arm64 with Rust 1.99.0 and ORAS
  1.3.4+Homebrew, ready for the SEC-06 focused validation.

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

## SEC-06 — Qualify macOS acquisition

- Status: ready; macOS arm64 host has Rust 1.99.0 and ORAS 1.3.4+Homebrew
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
