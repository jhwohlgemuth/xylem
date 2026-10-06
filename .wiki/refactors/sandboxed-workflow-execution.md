---
type: Plan
title: "Run opt-in workflows in policy-constrained sandboxes"
description: "Preserve host execution by default and route explicitly opted-in workflows through a fail-closed sandbox policy and lifecycle boundary."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, sandbox, isolation, policy, process, lifecycle]
status: implementing
---

# Delivery status

The first guarded slice is implemented: explicit opt-in, strict policy parsing
and hashing, schema-v8 plan/journal evidence, native rootless OCI qualification,
and hardened execution for local generic script modules. Dry-run remains
write-free and does not probe the runtime. Omitted or false `sandbox` values
continue through the unchanged host path.

The first slice requires its digest-pinned image to be present locally and
never pulls implicitly. Authorized acquisition and runner-image publication
remain part of the preparation phase below.

This slice deliberately fails before preparation for features that do not yet
cross the boundary safely: requirements, remote or binary modules, Python and
host environment wrappers, nested configurations, application containers,
GPU monitoring, completion markers, artifact publication, and retained failed
containers. The remaining sections describe the work required to lift those
restrictions without weakening the boundary.

# Goal

Sandboxing is strictly opt-in through one simple workflow field:

```yaml
sandbox: true
```

When `sandbox` is absent or `false`, Xylem preserves current host execution
behavior: acquisition, preparation, environment handling, application
containers, artifacts, and CLI defaults continue to work without a sandbox
policy or OCI runtime.

A sandboxed workflow additionally requires `--sandbox-policy <PATH>` and fails
closed if the policy or backend is unavailable. Policy selection alone does
not activate sandboxing. Never downgrade a requested sandbox to host execution.

Deliver sandboxing through lifecycle-hardened rootless Docker and Podman. The
first slice uses pure invocation fixtures around the OCI provider; a complete
fake lifecycle provider remains part of the artifact and preparation phases.
Separate-kernel providers and sandbox-by-default rollout remain deferred.

# Trust model

Treat workflow configurations, nested module repositories, templates,
binaries, dependency manifests, input discovery results, and their output as
untrusted. Treat the Xylem executable, CLI invocation, and operator policy as
trusted control-plane inputs.

The initial boundary protects one user and host from one workflow run. It does
not claim hostile multi-tenant isolation. The OCI provider is shared-kernel
containment for trusted or semi-trusted local work and must not be documented
as a separate-kernel boundary.

Sandboxing limits impact; it does not solve prompt injection, destruction
within granted writable paths, dependency provenance, side channels, or unsafe
published artifacts. Secure acquisition remains a separate contract.

# Public contracts

## Workflow configuration and CLI

- Add `Config::sandbox: bool` with Serde default `false`. Do not expose backend,
  image, filesystem, network, lifecycle, or resource details in workflow data.
- No `sandbox` field or `sandbox: false` executes through the compatibility host
  provider with current behavior.
- `sandbox: true` without `--sandbox-policy` is rejected before acquisition or
  workflow execution.
- `--sandbox-policy` with sandboxing disabled is an unused-policy error rather
  than an implicit activation or silent ignore.
- Add global `--sandbox-policy <PATH>` for `run` and TUI-launched runs. The path
  is operator input, not reusable workflow configuration.

## Operator policy version 1

Accept strict, extension-selected JSON, JSONC, YAML, and YML. Reject unknown
fields. The policy owns:

- backend selection: Docker or Podman;
- default and allowed digest-pinned images;
- permitted module, input, and artifact roots;
- preparation networking: deny or unrestricted;
- child-environment allowlist;
- CPU, memory, PID, scratch, artifact, captured-output, and wall-time limits;
- cleanup, failure retention, and stale-resource TTL.

Build one immutable `EffectiveSandboxPolicy` with canonical JSON and a SHA-256
digest. Reject ambiguous paths, tags, unapproved images, invalid ceilings, and
unsupported capabilities.

## Runtime and durable evidence

Expose policy, capability, and typed sandbox error values. Keep provider and
session traits sealed to in-tree implementations.

Advance execution plans, journals, and summaries to schema version 8. Record
either `mode: off` for compatibility host runs or the backend/version,
shared-kernel isolation, image digest, policy digest, sessions, limits, process
termination, artifact publication, retention, and cleanup for sandboxed runs.
Keep application containers separate from the outer sandbox.

Sandboxing owns schema version 8. Revise the unimplemented Kubernetes and MCP
plans to reserve versions 9 and 10 respectively.

# Implementation

## 1. Explicit execution boundary with host compatibility

- Introduce lossless structured process requests/results and explicit host and
  sandbox executors.
- Route Git helpers, requirement checks, environment managers, scripts,
  binaries, and container-dispatched module commands through the boundary.
- Keep OCI management and host telemetry as documented trusted control-plane
  processes.
- The host executor delegates lifecycle to `acorn-cmd` while preserving the
  inherited environment, working directories, preparation order, application
  containers, and artifact behavior.
- Use ACORN shell-word parsing for legacy shell-format templates and preserve
  exact exec-format arguments.
- Instantiate sandbox-only immutable contexts, per-input workspaces, clean
  environments, and staged publication only for `sandbox: true`.

## 2. Policy and fake backend

- Create `xylem-lib/src/sandbox/` with strict parsing, validation, hashing, path
  roles, capability negotiation, session contracts, typed errors, and a fake
  provider with deterministic failure injection.
- Resolve policy only for opted-in workflows. Unsandboxed workflows must not
  read policy files, probe runtimes, pull images, or apply sandbox path rules.
- Map roles to `/xylem/modules`, `/xylem/input`, `/xylem/prepared`,
  `/xylem/output`, and `/xylem/tmp`.
- Canonicalize sources and reject symlink escapes, special files, overlapping
  mounts, non-UTF-8 template paths, and paths outside policy roots.
- Preserve write-free dry runs. Render effective policy and required
  capabilities without invoking a runtime, pulling images, creating a
  workspace, or accessing the network.

## 3. Rootless OCI backend and runner image

- Qualify native x86_64 Linux rootless Docker and Podman. Reject rootful
  engines, remote contexts, Docker Desktop, Podman machines, non-Linux hosts,
  and unsupported security or storage configurations.
- One logical run session owns separate preparation and execution containers
  per admitted input. Modules remain sequential within an input; concurrent
  inputs remain independently cancellable.
- Require digest-pinned images and verify the realized digest and runtime
  configuration. Pull only during an authorized non-offline run.
- Enforce a read-only root filesystem, numeric non-root identity, dropped
  capabilities, `no-new-privileges`, seccomp, private namespaces, no devices or
  host sockets, explicit mounts, resource ceilings, bounded tmpfs storage, and
  timeout/output limits.
- Execution always uses network `none`. Preparation may use policy-approved
  deny or unrestricted egress and receives no workflow inputs, final artifact
  destination, ambient credentials, or host runtime socket.
- Start containers from a sanitized environment and reject image environment
  entries outside the operator allowlist.
- Reject application `ContainerEnvironment` declarations only in sandbox mode;
  they remain compatible in host mode.
- Publish a reproducible digest-pinned x86_64 runner image from a pinned Debian
  snapshot containing CA certificates, POSIX shell, coreutils, Git, and a
  non-root Xylem user. Permit policy-approved compatible custom images after
  bootstrap probes.

## 4. Artifacts, lifecycle, and recovery

- Keep prepared dependencies and outputs in bounded container-private storage
  and export them into an RAII-owned staging workspace.
- Validate archives without following links. Reject traversal, special files,
  duplicate paths, excessive files or bytes, and cross-input conflicts.
- Publish only validated artifacts. Never expose the final host artifact
  directory as a writable sandbox mount.
- Publish completion markers with the corresponding successful artifacts.
- Explicitly close sessions on every terminal path and retain best-effort RAII
  cleanup during unwind.
- Reconcile only expired, Xylem-labeled resources whose durable run lease is
  absent or terminal. Never remove foreign or active resources.
- Default to deletion. Allow `retain_on_failure` only through trusted policy and
  record retained resource identifiers.
- Update architecture, configuration, CLI, threat-model, image, operations,
  and shared-plan documentation; regenerate help and replace v7 schemas with
  v8.

# Validation

- Preserve behavior and tests for configurations where sandbox is absent or
  false. These runs require no policy or runtime.
- Test explicit opt-in, missing and unused policy errors, TUI propagation,
  offline behavior, write-free dry-run, and the absence of sandbox-to-host
  fallback.
- Unit- and property-test strict policy parsing, canonical hashes, resource
  validation, path containment, hostile arguments, and deterministic v8
  artifacts.
- Fault every fake-provider lifecycle operation and verify durable terminal
  evidence plus exactly-once explicit cleanup attempts.
- Run required native-Linux conformance jobs against rootless Docker and
  Podman: filesystem, environment, network, privilege, resource, timeout,
  cancellation, descendant-reaping, artifact, and controller-death cases.
- Add a repository spawn audit permitting only the explicit host adapter and
  trusted provider or telemetry control-plane sites.

# Acceptance criteria

- Unsandboxed configurations retain current runtime behavior.
- Sandboxing occurs only with `sandbox: true` and a trusted policy.
- Every sandboxed workflow-controlled process crosses the session boundary.
- Required controls fail closed before workflow execution.
- Sandboxed commands cannot observe ambient secrets or undeclared host paths.
- Every external sandbox resource is removed or durably recorded as retained.
- Plans, journals, and reports contain sufficient policy and lifecycle evidence
  without storing secrets.

# Deferred scope

Rootful OCI, GPUs and devices, granular egress allowlists, nested container
runtimes, separate-kernel providers, installation-default policies, and
default-on rollout remain deferred.
