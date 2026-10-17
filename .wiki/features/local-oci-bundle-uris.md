---
type: Plan
title: "Support local OCI bundle module URIs"
description: "Acquire digest-pinned OCI image layout directories and archives as local script or configuration modules without contacting a registry."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [oci, bundles, acquisition, local, offline]
status: planned
---

# Goal

Allow a workflow to name a local OCI image layout directory or archive as a
module source. Xylem must resolve the bundle path relative to the configuration
that declares it, select a manifest by its immutable digest, and materialize
the artifact's files through the existing bounded acquisition and workspace
contracts. Bundle use must not require a registry or contact one implicitly.

Keep registry references such as `oci://registry/repository@sha256:...`
unchanged. This plan covers local OCI image layouts only; HTTP-hosted layout
archives are a separate remote source and remain out of scope.

# Current evidence

Rechecked on 2026-10-17.

- Registry-backed OCI script and configuration modules use ORAS, verify the
  manifest digest, enforce acquisition limits, cache the extracted tree, and
  record provenance.
- `examples/oci-local-registry/` demonstrates a checked-in layout copied into
  a local registry before Xylem acquires it.
- `examples/oci-local/` now demonstrates `oras pull --oci-layout` against a
  checked-in layout directory from a Xylem script wrapper. This works without a
  registry, but Xylem does not yet recognize a local OCI layout as a module URI.
- ORAS supports pulling from both OCI layout directories and layout archives
  with its `--oci-layout` mode. Keep ORAS as the layout reader unless
  implementation findings show a contract gap.
- Execution-plan schema v14 serializes OCI registry acquisition sources and
  actions. Run-summary schema v13 records OCI digest evidence; this evidence
  shape may remain unchanged if local and registry sources share the same
  manifest identity.

# Project code boundaries

- **Update:** `crates/xylem-lib/src/script/mod.rs`,
  `crates/xylem-lib/src/config/mod.rs`,
  `crates/xylem-lib/src/config/validation.rs`,
  `crates/xylem-lib/src/acquisition/mod.rs`,
  `crates/xylem-lib/src/acquisition/cache.rs`,
  `crates/xylem-lib/src/acquisition/executor.rs`,
  `crates/xylem-lib/src/acquisition/oci.rs`,
  `crates/xylem-lib/src/acquisition/policy.rs`,
  `crates/xylem-lib/src/plan.rs`,
  `crates/xylem-lib/src/sandbox/mod.rs`,
  `crates/xylem-lib/src/sandbox/lifecycle.rs`,
  `crates/xylem-lib/src/sandbox/docker_sandbox/provider.rs`,
  `crates/xylem-lib/src/sandbox/openshell/provider.rs`,
  the execution-plan schema and schema version, OCI acquisition tests,
  `examples/oci-local/`, and user-facing example documentation.
- **Create:** a focused local-layout acquisition helper or module if it keeps
  layout parsing separate from registry transport; focused directory/archive
  fixtures; `docs/schema/execution-plan-v15.schema.json` if the serialized
  acquisition intent changes.
- **Delete:** None.

# Dependencies and ordering

Build on the typed acquisition plans, immutable cache, bounded ORAS process
runner, workspace materialization, and sandbox preparation boundary already
used by registry OCI sources. This plan claims the configuration and
acquisition integration locks; schedule implementation after the current
secure-acquisition owner releases those files. It does not wait for unrelated
Git transport work to finish once the shared lock is available.

Do not broaden `oci://` or infer bundle behavior from a path suffix. Give local
layouts an explicit URI scheme and pin the selected OCI manifest digest. A
candidate spelling is `oci-layout:./bundle@sha256:<manifest-digest>`; confirm
the exact grammar, escaping, and relative-path rules before implementation.

# Recommendation

## URI and source model

- Add a typed local-layout URI variant. Resolve relative paths against the
  configuration file that owns the module, including nested configuration
  modules.
- Accept an OCI image layout directory and an OCI layout archive. Use ORAS's
  local-layout mode for both forms; do not unpack arbitrary archives in Xylem
  before validating them.
- Require the URI digest to match a SHA-256 OCI manifest digest. Resolve the
  exact manifest from the layout and reject a missing digest, missing blob,
  malformed index, invalid layout version, or descriptor/digest mismatch.
- Support the same tree-producing module kinds as registry OCI acquisition:
  script modules and configuration modules. Keep binary modules on their
  existing file or verified HTTP binary contract.
- Represent the source and action explicitly in `AcquisitionSource` and
  `AcquisitionAction`; do not disguise a local bundle as a registry URL or an
  ordinary directory source.

## Integrity, limits, and cache

- Apply existing OCI manifest, layer-count, staging-byte, tree-byte, entry,
  process-output, timeout, and cancellation limits to local layout reads.
- Permit ORAS access only to the declared local layout path. The source must
  not contain symlinks or escape the granted local module roots.
- Extract into a unique staging directory, inspect the resulting tree, then
  atomically publish it to the digest-keyed acquisition cache. Cache hits must
  revalidate the tree and manifest identity using the current cache rules.
- Preserve offline behavior: a verified cache hit is usable; an uncached local
  bundle remains locally available without a network request. Dry-run may
  validate the URI and planned digest but must not invoke ORAS, create a cache
  entry, or materialize files.
- Keep acquisition evidence explicit about the local-layout source while
  retaining the manifest digest and extracted-tree digest.

## Sandbox and configuration behavior

- Update module validation, configuration loading, execution planning, and
  sandbox preparation so local layouts are accepted only where the source can
  be materialized safely.
- Ensure Docker Sandbox and OpenShell preparation stage the validated local
  layout or its verified extracted tree under their existing module-root and
  network policies. A local bundle must not accidentally trigger registry
  credentials or network access.
- Bump execution-plan schema to v15 if its serialized source/action vocabulary
  changes. Update schema fixtures and consumers together. Keep run-summary v13
  unless the durable acquisition-evidence fields change.
- Replace the ORAS wrapper in `examples/oci-local/` with the native local URI
  after the behavior is complete; retain `oci-local-registry` to demonstrate
  registry transport separately.

# Validation

- Cover URI parsing, digest validation, config-relative resolution, nested
  modules, accepted module kinds, and rejection of unsupported schemes.
- Exercise layout-directory and layout-archive acquisition, verified cache
  miss/hit, cache corruption, digest mismatch, missing blobs, malformed index,
  empty artifacts, limits, timeout, cancellation, and cleanup after failure.
- Verify offline and dry-run paths do not contact a registry or create
  unintended cache/workspace state.
- Test symlink, traversal, special-file, duplicate destination, and
  oversized-tree handling. Preserve ORAS's default safe extraction behavior;
  never enable path traversal.
- Verify local-bundle planning and preparation in host mode and each supported
  sandbox provider. Run the example with networking unavailable to demonstrate
  that no registry is required.
- Validate execution-plan v15 and confirm whether run-summary v13 remains
  sufficient for the resulting provenance.

# Acceptance criteria

- A digest-pinned local OCI layout directory or archive can supply a script or
  configuration module without a registry, registry credentials, or network
  access.
- Relative layout paths resolve against the declaring configuration, including
  nested configuration modules.
- Manifest identity, layer content, and extracted tree limits are checked
  before execution; extraction and cache publication are bounded and atomic.
- Cache, offline, dry-run, sandbox, and provenance behavior follow the existing
  acquisition contracts.
- Registry-based `oci://` modules retain their current behavior and policy.
- `oci-local` uses the native URI; `oci-local-registry` continues to cover
  local-registry acquisition.
---
