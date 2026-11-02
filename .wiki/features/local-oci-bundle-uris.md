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

Rechecked on 2026-11-01 against baseline `830aa31`, accepted OCI-01 source
validation, OCI-02 archive qualification, OCI-03 provider preparation
qualification, and the history since the directory-form implementation at
`97d5203`.

- Local OCI layout directories are native script and configuration module
  sources. Paths resolve relative to the declaring configuration, ORAS reads
  the layout, and the `oci-local` example now uses the native URI.
- Execution-plan v15 permits `oci-layout:` references and run-summary v14
  records the existing digest evidence. Neither schema needs a new version if
  archive handling uses the same source and evidence shape.
- The deterministic `bundle.tar` fixture is 10,240 bytes with SHA-256
  `cfe4bf409883c7fb0eb4b2010b1406ac380e1204bb06548a35db19617f0f74a9`.
  ORAS 1.3.5 verified and extracted it by the pinned manifest digest without
  network access. Malformed-index, missing-blob, staging-limit, cancellation,
  cache-miss, and offline cache-hit cases pass without publishing incomplete
  entries.
- The checked-in acquisition tests cover a local layout directory and offline
  cache reuse. OCI-01 resolves direct and nested archive paths against their
  declaring configuration and rejects symlinked, traversing,
  non-regular, unsupported-extension, and malformed-digest sources before
  acquisition.
- Sandbox-oriented materialization now replaces the selected module URI with
  the verified input-local `file:` path before `SandboxInput` validation while
  retaining the original `oci-layout:` source and digest in immutable
  acquisition evidence. Deny-network validation accepts the resulting local
  module without registry access.
- Host execution consumes the extracted input-local tree, and OCI, Docker
  Sandbox, and OpenShell policies accept that same verified source. Docker
  Sandbox preparation is local-only with networking denied; OpenShell emits no
  preparation network policy. The native `oci-local` example passes with
  acquisition explicitly offline.
- Secure acquisition implementation and Linux/macOS qualification are landed.
  Its remaining SEC-07 work owns the Windows runner and manifests, not the
  local archive documentation, so it does not block OCI-04.

# Project code boundaries

- **Update:** example docs.
- **Create:** None.
- **Delete:** None.

# Dependencies and ordering

The acquisition, configuration-to-sandbox handoff, and sandbox preparation
locks are released. OCI-04 may take the documentation lock independently of
completed environment work and unrelated platform qualification. Keep the
v15/v14 durable vocabulary unless an observed missing field requires a
separately coordinated schema phase.

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

Verify documentation links and run the native local example with network
access disabled after the accepted behavior is documented.

# Acceptance criteria

- A digest-pinned OCI layout archive supplies script and configuration modules
  without registry access, using the existing local URI and provenance shape.
- Archive and directory input obey the same integrity, limits, cache, offline,
  dry-run, workspace, and sandbox contracts.
- Registry OCI behavior remains covered by its existing tests.

# Execution phases

## OCI-04 — Document the accepted local archive contract

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: accepted host and sandbox provider qualification
- Exact write set: `docs/src/configuration.md`, `docs/src/examples.md`, `examples/oci-local/README.md`, `examples/README.md`
- Primary integration lock: documentation
- Outcome: URI syntax, archive limits, offline behavior, and the native example match tested behavior.
- Focused validation: documentation links and a final native example smoke run.
- Handoff: Preserve the tested contract in user docs, then retire this plan and its index, network, and lane entries.
