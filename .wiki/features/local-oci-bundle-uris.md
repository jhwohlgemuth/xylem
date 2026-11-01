---
type: Plan
title: "Qualify local OCI layout archives"
description: "Complete archive and sandbox qualification for the landed digest-pinned local OCI layout module path."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [oci, bundles, acquisition, local, offline]
status: planned
execution: phased
phase_budget_hours: 5
---

# Goal

Complete local OCI archive support and qualification for the existing
`oci-layout:<path>@sha256:<manifest-digest>` module URI. A local bundle must be
usable without registry access while registry `oci://` behavior stays intact.

# Current evidence

Rechecked on 2026-10-31 against baseline `ef95f81` and current OCI-01 changes,
acquisition/configuration tests, and the history since the directory-form
implementation at `97d5203`.

- Local OCI layout directories are native script and configuration module
  sources. Paths resolve relative to the declaring configuration, ORAS reads
  the layout, and the `oci-local` example now uses the native URI.
- Execution-plan v15 permits `oci-layout:` references and run-summary v13
  records the existing digest evidence. Neither schema needs a new version if
  archive handling uses the same source and evidence shape.
- The checked-in acquisition tests cover a local layout directory and offline
  cache reuse. OCI-01 resolves direct and nested archive paths against their
  declaring configuration and rejects symlinked, traversing,
  non-regular, unsupported-extension, and malformed-digest sources before
  acquisition. Archive extraction, malformed archive/index/blob cases, and
  the complete sandbox preparation matrix lack accepted qualification.
- Secure acquisition implementation and Linux/macOS qualification are landed.
  Its remaining SEC-07 work owns the Windows runner and manifests, not the
  configuration/acquisition source files, so it does not block OCI-02.

# Project code boundaries

- **Update:** acquisition URI/path validation, ORAS materialization, focused
  archive tests, configuration and sandbox preparation tests, and example docs.
- **Create:** a small OCI layout archive fixture only when needed for the
  focused tests.
- **Delete:** None.

# Dependencies and ordering

The configuration/acquisition lock is released after OCI-01. OCI-02 owns the
acquisition executor and can proceed independently of ENV-08. The archive
extension does not wait for unrelated platform qualification. Keep the v15/v13
durable vocabulary unless an observed missing field requires a separately
coordinated schema phase.

# Recommendation

- Accept directory and archive forms through the existing explicit
  `oci-layout:` scheme and SHA-256 manifest pin. Reject symlinks, special files,
  escaping paths, malformed indexes, missing blobs, digest mismatches, and
  unsupported module kinds before execution.
- Apply the existing ORAS timeout, output, staging, tree, and entry ceilings.
  Stage and publish atomically; a verified offline cache hit remains usable,
  and an uncached local archive may be read without network access.
- Dry-run validates the URI and source intent without invoking ORAS or writing
  a workspace. Host and supported sandbox preparation receive the same
  verified extracted tree and never request registry credentials.

# Validation

Cover archive parsing and acquisition, digest and descriptor errors, limits,
corruption, cancellation, offline and dry-run behavior, nested configuration,
and host/Docker Sandbox/OpenShell preparation. Run the native local example with
network access disabled.

# Acceptance criteria

- A digest-pinned OCI layout archive supplies script and configuration modules
  without registry access, using the existing local URI and provenance shape.
- Archive and directory input obey the same integrity, limits, cache, offline,
  dry-run, workspace, and sandbox contracts.
- Registry OCI behavior remains covered by its existing tests.

# Execution phases

## OCI-02 — Materialize bounded archives through ORAS

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: local archive source validation accepted; configuration ownership released
- Exact write set: `crates/xylem-lib/src/acquisition/oci.rs`, `crates/xylem-lib/src/acquisition/executor.rs`, `crates/xylem-lib/src/acquisition/tests.rs`, `examples/oci-local/module/bundle.tar`
- Primary integration lock: acquisition executor
- Outcome: Archive reads verify the pinned manifest, bound extraction, and publish only complete cache entries.
- Focused validation: `cargo test -p xylem-lib acquisition::tests --locked` with archive, corruption, limit, cancellation, and offline cases.
- Handoff: Record fixture identity, ORAS version, and cache evidence; remove OCI-02 when accepted.

## OCI-03 — Qualify host and sandbox preparation

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: OCI-02 and release of the sandbox preparation lock
- Exact write set: `crates/xylem-lib/src/sandbox/tests/mod.rs`, `crates/xylem-lib/src/sandbox/docker_sandbox/tests.rs`, `crates/xylem-lib/src/sandbox/openshell/tests.rs`, `crates/xylem-lib/src/config/tests/mod.rs`
- Primary integration lock: sandbox preparation
- Outcome: Host, Docker Sandbox, and OpenShell runs receive only the verified local tree, with no registry credentials or network use.
- Focused validation: focused provider preparation tests and `oci-local` example with networking unavailable.
- Handoff: Record each supported provider result; remove OCI-03 when accepted.

## OCI-04 — Document the accepted local archive contract

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: OCI-03
- Exact write set: `docs/src/configuration.md`, `docs/src/examples.md`, `examples/oci-local/README.md`, `examples/README.md`
- Primary integration lock: documentation
- Outcome: URI syntax, archive limits, offline behavior, and the native example match tested behavior.
- Focused validation: documentation links and a final native example smoke run.
- Handoff: Preserve the tested contract in user docs, then retire this plan and its index, network, and lane entries.
