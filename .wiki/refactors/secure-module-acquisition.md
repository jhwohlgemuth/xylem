---
type: Plan
title: "Harden remote module acquisition"
description: "Validate, pin, verify, bound, and safely materialize remote configuration, binaries, Git repositories, and OCI artifact trees under offline and dry-run policy."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, downloads, git, oci, checksum, offline, supply-chain]
status: in-progress
---

# Goal

Make every remote workflow input reproducible and fail closed. Before network
or filesystem mutation, Xylem must identify the source kind, effective policy,
immutable identity, resource limits, expected content, and destination. Remote
configuration documents, executable binaries, and Git-backed script projects
have different materialization contracts and must not share an implicit
"remote URI" fallback.

# Current evidence

Rechecked on 2026-10-17.

- Runtime integration landed on 2026-10-12: remote documents, binaries, and Git modules use immutable acquisition plans, bounded executors, verified cache entries, and durable provenance.
- Existing tests cover policy and identity validation, bounded HTTP responses, offline cache behavior, cache corruption, SSH environment isolation, and escaping Git symlinks.
- Concurrent acquisition of the same immutable identity now has a focused HTTP/cache publication test, and Git origin normalization has direct coverage.
- The broader Git transport, resource-bound, cancellation, and cross-platform qualification matrix is not yet recorded.
- Xylem now pins all ACORN crates to accepted revision `0c256bae0ff42da1c96de50fd525146a95965c66`; the lockfile resolves `quick-xml` 0.41.0 and the advisory scan passes.
- OCI module acquisition now accepts digest-pinned ORAS artifacts as file trees, applies the exact-host policy and existing staging/tree ceilings, and records the OCI manifest digest and extracted tree evidence. ORAS is a host runtime prerequisite.

# Project code boundaries

- **Update:** acquisition qualification tests, workspace ACORN pins, `Cargo.lock`, and `deny.toml`.
- **Create:** only Git fixtures needed to cover a demonstrated gap in the remaining qualification matrix.
- **Delete:** resolved advisory exceptions when accepted dependency revisions clear them.

# Dependencies and ordering

Builds on the existing typed-error, validated-planning, managed-run-root, and
workflow-environment contracts. Coordinate with sandbox policy: acquisition
owns source validation and materialization, while sandboxing owns the
capability boundary for network, Git, credentials, and subprocess execution.

# Implementation status

The first stage was implemented on 2026-10-11. `xylem-lib` now has a dormant
acquisition planner, fail-closed nested policy, bounded HTTP and Git executors,
verified content-addressed cache, atomic publication, offline cache reuse, and
provenance records. Tests cover policy and identity validation, bounded HTTP
downloads, cache corruption, offline behavior, plan tampering, and escaping Git
symlinks.

The runtime-integration stage was implemented on 2026-10-12. CLI policy is
resolved before remote configuration loading; direct documents, binaries, and
Git modules now use typed immutable acquisition plans and the verified cache.
Executable content is copied from cache into each owned input workspace before
host or sandbox preparation, and sandbox providers reject any unmaterialized
remote URI. The checksum bypass is now an explicit CLI-only emergency policy:
it warns, remains bounded, records the observed digest, and cannot perform an
offline lookup without a pretrusted identity. Request-only probes, the floating
Git path, and legacy unbounded download/read helpers were removed.

Execution-plan schema v14 carries complete acquisition intent, including
digest-pinned OCI artifact trees. Journal and run-summary schema v13 carry verified configuration and per-input module
acquisition evidence. Integration tests cover request-free dry-run, offline
miss and cache reuse, bounded document/binary acquisition, workspace copying,
nested policy intersection, and exactly-once provenance recovery. Workspace
tests, Clippy with warnings denied, and the complete test suite pass.

The concurrent cache-publication and Git-origin normalization tests now pass on
the current macOS host. ACORN revision `0c256bae0ff42da1c96de50fd525146a95965c66`
upgrades `quick-xml` to 0.41.0; Xylem has moved its ACORN pins to that revision
and removed the two resolved advisory exceptions. The plan remains in progress
pending the broader Git transport/cancellation and cross-platform qualification
matrix.

## Remaining phases

1. Qualify OCI artifact format handling, registry authorization behavior, ORAS cancellation and cleanup, and offline cache reuse; ensure registry auth redirects and token-service hosts respect the host policy.
2. Complete Git transport, bounded-resource, cancellation, and interrupted-cleanup qualification; run the containment, permissions, atomicity, and same-identity concurrency matrix on every supported platform.
3. Qualify the accepted ACORN revision's narrow feature path, rerun the advisory audit, and complete workspace, native, and Windows checks.

# Recommendation

## Shared policy and planning

- Resolve CLI policy before configuration loading, preparation, or planning. A nested configuration may tighten inherited policy but may not broaden hosts, schemes, limits, or unsafe capabilities.
- Represent source kind explicitly: local file, bounded HTTP(S) document, verified HTTP(S) binary, or Git repository. Reject ambiguous extensions, unsupported schemes, credentials in URLs, and remote module types without a defined materializer.
- Make the plan describe acquisition without performing it. Dry-run must not connect, resolve DNS, invoke Git, create destinations, change permissions, or populate a cache.
- Give every remote operation explicit connect, total, response-byte, and process/output limits. Return typed errors that retain module path and redacted source context.

## Remote binaries

- Allow HTTPS by default; allow HTTP only through an explicit local/development policy. Do not treat SSH as a binary-download scheme.
- Require an algorithm-qualified digest, initially SHA-256 or stronger, for executable content. If an unsafe bypass remains, make it CLI-only, conspicuous, unavailable from untrusted configuration, and recorded in the outcome.
- Enforce redirect, host/scheme, response-status, timeout, content-length, and maximum-byte policy; do not rely on a reachability probe as authorization.
- Download to a unique file within a capability-owned staging directory, stream the digest while writing, fsync as appropriate, verify, then atomically rename into a validated destination.
- Reject symlinks, path traversal, destination collisions, unexpected content type where applicable, and content outside the run workspace. Never derive an executable destination solely from an untrusted URL basename.

## Git-backed scripts and configuration

- Model a Git repository separately from a generic URI. Permit supported HTTPS/SSH transports only after host and sandbox policy validation.
- Require a full immutable commit ID, or resolve a user-supplied tag/ref once and record the resulting commit before execution. Verify checked-out `HEAD` and normalized remote origin after clone/fetch.
- Use a capability-owned temporary checkout and atomic publish. Bound clone/fetch time and output, disable hooks, submodules, LFS, credential helpers, and inherited credentials by default, and make shallow/object limits explicit.
- Treat SSH as a Git transport only when the credential policy explicitly permits it; it is never a binary-download scheme.
- Record source, resolved revision, content/tree identity where available, acquisition time, limits, and policy decisions in the workflow outcome.

## Offline and dry-run

- Offline execution may use only a previously verified cache entry addressed by digest/revision and matching the current policy. A cache miss is a typed offline error and never falls through to another scheme branch.
- Cache writes occur only after verification and atomic publication; cache entries are immutable, containment-checked, and garbage-collectable without affecting active runs.
- Dry-run validates source policy and cache identity and renders intended actions without network access, Git invocation, destination creation, permission changes, or cache mutation.

# Validation

- Unit-test URL redaction, exact-host and scheme policy, nested-policy intersection, redirect targets, response limits, typed errors, digest parsing, and deterministic destination identities.
- Fake-server tests cover HTTPS/HTTP policy, redirects, non-success status, timeout, oversized and truncated bodies, malformed documents, checksum mismatch, destination collisions, symlink/traversal attempts, atomic cleanup, and request-free dry-run.
- Git fixtures cover pinned commits, moved tags, origin mismatch, shallow/object/time bounds, hook/submodule/LFS/credential denial, SSH/HTTPS policy, cancellation, interrupted cleanup, and cache reuse.
- Test offline verified-cache hit, cache miss, identity mismatch, corruption, and policy mismatch. Assert no network or Git process occurs in offline and dry-run cases.
- Run archive/path traversal, symlink containment, permission, and atomicity tests on every supported platform; test concurrent acquisition of the same identity.
- Run dependency audit and feature-tree inspection after choosing the final HTTP/Git implementation.

# Acceptance criteria

- Remote configuration and executable content cannot be consumed without an allowed source, explicit scheme/type policy, and bounded materializer.
- Remote binaries cannot run without a verified algorithm-qualified digest under the normal policy; any unsafe bypass is CLI-only, conspicuous, and recorded.
- Git modules execute only from a verified immutable revision with a verified origin and disabled implicit code-fetch features.
- All acquisition is bounded, staged, containment-checked, atomically committed, and safe under concurrent requests.
- Offline mode performs no network access and uses only verified cache identities; cache misses and mismatches are typed failures.
- Dry-run performs no network, Git, filesystem-materialization, permission, or cache side effect.
- Outcomes retain sufficient provenance to reproduce or audit the selected document, repository revision/tree, or binary bytes.
- Unsupported, ambiguous, credential-bearing, or policy-violating remote sources fail validation instead of falling through or panicking.
