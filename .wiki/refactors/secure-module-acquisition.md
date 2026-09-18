---
type: Plan
title: "Harden remote module acquisition"
description: "Validate, pin, verify, bound, and atomically materialize remote binaries and repositories under offline and dry-run policy."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, downloads, git, checksum, offline, supply-chain]
status: planned
---

# Goal

Make remote workflow modules reproducible and fail closed. A plan must identify an allowed source, immutable revision or digest, size/time bounds, expected content, and destination before Xylem performs network or filesystem mutation.

# Current evidence

- Remote binary schemes include HTTP, HTTPS, and SSH even though the download helper is HTTP-oriented; comments mention an allowlist, but none is implemented.
- A missing binary checksum terminates as unavailable, yet `--skip-verify-checksum` permits arbitrary downloaded bytes and is exposed as an ordinary workflow/CLI option.
- Download errors collapse to an empty checksum, making transport, read, and mismatch failures hard to distinguish.
- Downloaded binaries are written and then made executable before a broader transactional materialization boundary is established.
- Remote Python modules run `git clone` without a required commit/tag, repository allowlist, depth/size bound, or post-clone origin/revision verification.
- Remote reachability validation is currently `if true`.
- Generic remote scripts are accepted by configuration but panic as unimplemented.
- Offline matching has no explicit branch for remote sources; the guarded URL arm falls through to generic unsupported handling.
- Destination names derive from URL/hash helpers without a Xylem-owned collision, symlink, or containment policy.

# Project code boundaries

- **Update:** acquisition and preparation logic currently in `xylem-lib/src/config/mod.rs` and `script/mod.rs`, location/checksum configuration types, CLI security options, tests, and examples.
- **Create:** an acquisition planner/executor module, source-policy types, safe destination/materialization helpers, fake HTTP/Git fixtures, and provenance records.
- **Delete:** `--skip-verify-checksum` as a routine production bypass or restrict it to an explicitly unsafe local-development policy; unconditional reachability success; floating Git clone behavior; unsupported SSH binary download paths.

# Dependencies and ordering

Follows the [ACORN 0.2 upgrade](../migrations/acorn-0-2-upgrade.md), [typed errors](typed-error-and-outcome-boundary.md), [process execution](process-execution-contract.md), and [validated planning](configuration-validation-and-dry-run.md).

# Recommendation

## Remote binaries

- Allow HTTPS by default; allow HTTP only through an explicit local/development policy. Do not treat SSH as a binary-download scheme.
- Require an algorithm-qualified digest, initially SHA-256 or stronger, for executable content. If an unsafe bypass remains, require a conspicuous flag unavailable from untrusted configuration and record it in the outcome.
- Enforce redirect, host, response-status, timeout, and maximum-byte policy before materialization.
- Download to a unique file within a capability-owned staging directory, stream the digest while writing, fsync as appropriate, verify, then atomically rename.
- Reject symlinks, path traversal, destination collisions with different content, and content outside the run workspace.

## Git-backed scripts

- Model a Git repository separately from a generic URI. Permit supported HTTPS/SSH transports only after host policy validation.
- Require an immutable commit ID or a tag resolved and recorded to a commit. Verify the checked-out `HEAD` and remote origin after clone/fetch.
- Bound clone depth, object size/time, submodules, LFS, hooks, and credential forwarding. Disable hooks and submodules unless explicitly required and reviewed.
- Record source, revision, digest/provenance, acquisition time, and policy decisions in the workflow outcome.

## Offline and dry-run

- Offline execution may use only a previously verified cache entry addressed by digest/revision. A cache miss is a typed offline error and never falls through to a different scheme branch.
- Dry-run validates the source policy and renders intended actions without network access or destination creation.

# Validation

- Fake-server tests cover redirects, HTTP denial, TLS/host policy, non-success status, timeout, oversized bodies, checksum mismatch, truncated responses, destination collisions, and atomic cleanup.
- Git fixtures cover pinned commits, moved tags, origin mismatch, submodule denial, hook suppression, SSH/HTTPS policy, cancellation, and cache reuse.
- Test offline cache hit/miss and dry-run zero-I/O behavior.
- Run archive/path traversal and symlink containment tests on every supported platform.
- Run dependency audit and feature-tree inspection after choosing the final HTTP/Git implementation.

# Acceptance criteria

- Remote executable content cannot run without an allowed source and verified digest under the normal policy.
- Git modules execute from a verified immutable revision.
- All acquisition is bounded, staged, containment-checked, and atomically committed.
- Offline mode performs no network access and uses only verified cache identities.
- Dry-run performs no acquisition side effect.
- Outcomes retain sufficient provenance to reproduce or audit the selected module bytes.
- Unsupported remote module types fail validation instead of panicking.

# Related plans

- [ACORN 0.2 upgrade](../migrations/acorn-0-2-upgrade.md)
- [Configuration validation and dry-run](configuration-validation-and-dry-run.md)
- [Dependency surface and security gates](dependency-surface-and-security-gates.md)
