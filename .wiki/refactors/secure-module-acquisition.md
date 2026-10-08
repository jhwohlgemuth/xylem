---
type: Plan
title: "Harden remote module acquisition"
description: "Validate, pin, verify, bound, and safely materialize remote configuration, binaries, and repositories under offline and dry-run policy."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, downloads, git, checksum, offline, supply-chain]
status: planned
---

# Goal

Make every remote workflow input reproducible and fail closed. Before network
or filesystem mutation, Xylem must identify the source kind, effective policy,
immutable identity, resource limits, expected content, and destination. Remote
configuration documents, executable binaries, and Git-backed script projects
have different materialization contracts and must not share an implicit
"remote URI" fallback.

# Current evidence

The existing exact-host policy, redirect checks, URL redaction, typed remote
errors, checksum requirement for normal binary downloads, and dry-run handling
for remote configuration wrappers are the baseline. The remaining gaps are:

- The default allowlist is still allow-all, and HTTP is not distinguished from HTTPS by policy.
- Binary downloads use `Response::copy_to` without a byte limit or streamed digest, derive a filename from the URL, and write under the run root without an Xylem-owned staging/collision policy.
- Remote configuration documents are read fully into a `String` without a response-size limit or cache identity.
- `--skip-verify-checksum` is still an ordinary configuration field and CLI flag rather than an explicitly unsafe operator policy.
- Dry-run binary preparation still performs a reachability request before reporting the planned download; executable remote acquisition must be request-free.
- Git-backed scripts and configuration modules can clone floating `HEAD`, use an unbounded clone, and do not verify the checked-out commit or remote origin after materialization.
- Offline mode rejects remote sources rather than using a previously verified cache, and acquisition emits no provenance record for run reporters.

# Project code boundaries

- **Update:** `crates/xylem-lib/src/remote.rs`, acquisition and preparation logic in `crates/xylem-lib/src/config/mod.rs` and `script/mod.rs`, location/checksum and options types, CLI security options, typed outcomes/journal metadata, tests, schemas, and user-facing configuration/module documentation.
- **Create:** an acquisition planner/executor boundary, explicit source-policy and limit types, safe staging/cache/materialization helpers, fake HTTP/Git fixtures, and provenance records.
- **Delete:** routine configuration-controlled checksum bypasses; network probes during dry-run; unbounded response and Git operations; floating Git checkout behavior; direct writes to caller-selected roots; and unsupported remote source fallbacks.

# Dependencies and ordering

Builds on the existing typed-error, validated-planning, managed-run-root, and
workflow-environment contracts. Coordinate with sandbox policy: acquisition
owns source validation and materialization, while sandboxing owns the
capability boundary for network, Git, credentials, and subprocess execution.

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
