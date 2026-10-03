---
type: Plan
title: "Run workflows in policy-constrained sandboxes"
description: "Route workflow-controlled preparation and execution through a fail-closed sandbox policy and lifecycle boundary."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [security, sandbox, isolation, policy, process, lifecycle]
status: planned
---

# Goal

Run workflow-controlled code in a constrained environment that limits what it
can read, modify, contact, and consume. Preserve Xylem's parse, validate, plan,
execute, journal, and report boundaries while making the effective isolation
backend and policy explicit and auditable.

The first supported backend should make local, semi-trusted workflows safer
without pretending that a shared-kernel container is a hostile multi-tenant
boundary. The design must also admit a separate-kernel or qualified managed
backend without changing workflow execution semantics. A required sandbox
must fail closed when the selected backend cannot enforce the policy; Xylem
must never silently fall back to direct host execution.

# Threat and trust model

Treat module repositories, templates, binaries, dependency manifests, input
discovery scripts, and their output as untrusted. Treat the Xylem executable,
the CLI invocation, and an operator-owned policy file as trusted control-plane
inputs. A workflow configuration may request less access or more resources,
but it cannot broaden the operator policy or lower the required isolation
class.

The initial boundary protects one user and host from one workflow run. It does
not claim to isolate mutually hostile modules or inputs within the same run.
Use one execution session per run and one run per tenant or trust domain. Keep
per-input working directories inside that session to avoid accidental state
collisions while preserving sequential module semantics. Use separate,
short-lived sessions for discovery and preparation when those phases need
different mounts or network access.

Sandboxing limits impact; it does not solve prompt injection, authorized
destruction within writable paths, dependency provenance, denial of service
outside enforced quotas, side channels, or unsafe artifacts. Remote content
still requires the separate secure-acquisition contract.

# Current evidence

Rechecked against `fa07c39`.

- `xylem-cli/src/commands/run/mod.rs` owns the run ID, temporary root, worker
  pool, journal, module preparation, input dispatch, and final reporting. The
  temporary root has no RAII cleanup owner.
- `Config::run_workflow_recorded` constructs module scripts and calls
  `Script::run_checked` directly. There is no execution-provider boundary.
- `Script::run_checked` executes in a host working directory. Child processes
  inherit the ambient process environment in addition to `_XYLEM_*` overrides.
- `Config::set_environment_variables` mutates the parent process environment
  before concurrent inputs run.
- Preparation also executes code: Git clone, Conda/Mamba/Pixi environment
  creation, executable permission changes, and remote probes occur before the
  final module command. Sandboxing only `run_workflow_recorded` would leave
  these paths on the host.
- `verify_requirements` inspects host executables even though requirements for
  a sandboxed workflow must be evaluated in the sandbox image or session.
- `options.allowed` governs Xylem's own remote module fetches and preserves
  allow-all behavior when absent. It does not constrain child DNS, redirects,
  IPv6, UDP, private addresses, metadata endpoints, or other protocols.
- `ExecutionLimits` bounds Xylem's Rayon workers only. It does not bound child
  CPU, memory, PIDs, disk, output, network bandwidth, or wall time.
- `InputAttribute::ShellScript` is deserialized but rejected by validation and
  planning. The CEL workflow-input plan replaces this reserved variant with
  side-effect-free expression evaluation and does not require an isolation
  provider.
- `ContainerEnvironment` wraps a module command in `docker exec`, `podman
  exec`, or `apptainer exec`. It names an application environment and is not a
  Xylem security boundary. Passing the host Docker socket into a workflow
  sandbox would erase the intended host boundary.
- Completion inspection and marker persistence occur on the host. Their path
  containment work can remain trusted control-plane behavior if artifact
  publication is staged and validated first.

# Core decisions

## Separate policy from mechanism

Define a technology-neutral `SandboxPolicy` and a small set of in-tree backend
adapters. Do not expose Docker, OpenShell, or a remote service's complete
configuration as Xylem's public contract. The policy covers:

- filesystem capabilities and host-to-guest path mappings;
- network default and allow rules;
- child environment and credential handling;
- process identity, privilege, syscall, and subprocess limits;
- CPU, memory, PIDs, disk, captured output, and wall time;
- image or snapshot identity;
- persistence, retention, cancellation, and cleanup.

Each backend reports enforceable capabilities such as network denial,
host-level or L7 egress rules, separate-kernel isolation, disk quotas, private
container runtime, and credential brokering. Policy compilation rejects a
request when a required capability is unavailable. A warning is not an
acceptable substitute for a required control.

Start with closed Rust enums and traits rather than a plugin ABI. A dynamic
provider system is unnecessary until a second out-of-tree backend proves the
need.

## Keep grants outside untrusted workflow data

Add an operator-owned policy selected by CLI or installation default. A
workflow may carry a sandbox request, but that request is not authority. Build
the effective policy with deterministic narrowing rules:

- deny overrides allow;
- path access is the intersection of operator grants and workflow needs;
- numeric quotas use the lower ceiling;
- network remains denied unless both layers permit a destination;
- raw secrets remain denied unless the operator configures a brokered
  operation;
- the required isolation class can only stay the same or become stronger.

Record the canonical effective-policy digest in the plan and journal. Reject
unknown policy fields and ambiguous host paths.

## Sandbox every workflow-controlled process

Route all of these operations through the same execution boundary:

- module commands, including binary and script modules;
- virtual-environment creation and package-manager hooks;
- configured before/after hooks if they become active;
- Git and other external acquisition helpers;
- managed application-container commands when the backend supports them.

Keep only Xylem-owned parsing, validation, policy compilation, journaling,
reporting, and safe library-based acquisition in the host control plane.
Centralize process creation so a repository search can prove that workflow
code has no alternate spawn path.

## Use explicit path capabilities

The first policy schema should use Xylem-owned roles, not arbitrary workflow-
supplied host paths:

- configuration and staged module sources: read-only;
- declared inputs: read-only;
- run scratch and temporary files: read-write, private to the sandbox;
- staged artifacts: read-write, private until publication;
- journal, credentials, host sockets, devices, and the rest of the host:
  unavailable.

Canonicalize every source before provisioning, reject symlink escapes and
special files, and assign stable guest paths such as `/xylem/modules`,
`/xylem/inputs`, `/xylem/output`, and `/xylem/tmp`. Rewrite reserved Xylem
variables to guest paths. Do not mount the invoking working directory merely
because it contains the configuration.

Publish artifacts through a separate boundary: enumerate without following
symlinks, enforce count and byte limits, reject devices/sockets and traversal,
copy into a host staging directory, then commit to the declared artifact root.
Treat published files as untrusted. A compatibility release may offer a direct
writable artifact mount only as an explicitly weaker policy with a warning in
the plan and journal.

## Clear the child environment

Start sandboxed commands from an allowlisted baseline such as `PATH`, locale,
and backend-required variables. Add resolved `_XYLEM_*` values per invocation.
Do not copy the parent environment or dotenv secrets wholesale. The initial
release should deny raw credentials; later backends may expose credential
broker capabilities that bind a credential to a destination and operation.

## Keep network phases distinct

Use deny-by-default networking for module execution.
Acquisition and environment preparation may request a separate, time-bounded
network policy, so they run in a preparation session that has no inputs,
artifacts, credentials, or host-writable mounts. Package-manager hooks are
workflow-controlled code and receive only module sources, scratch, and the
enforced acquisition destinations. Export prepared dependencies as a bounded,
validated snapshot or bundle, terminate the preparation session, and mount the
result read-only for execution. Never widen a live execution session's policy.

Do not translate `options.allowed` into a claim of child egress enforcement:
it remains a control-plane source allowlist until a backend can enforce DNS,
address, redirect, private-network, and protocol behavior for the child.

Plain Docker or Podman can reliably provide `none` versus a broader network,
but not the full hostname/L7 contract by themselves. Reject granular allow
rules on that backend instead of approximating them. A policy-aware backend
such as OpenShell may satisfy those rules through its own enforcement layer.

# Recommended backend sequence

## 1. Fake backend and conformance suite

Implement an in-memory/fake backend first. It records create, stage, exec,
cancel, export, and destroy requests and allows deterministic failures. Use it
to move orchestration behind the boundary without requiring a local runtime in
unit tests.

## 2. Hardened OCI backend for semi-trusted local work

Implement a Linux Docker/Podman backend as the first usable provider. Its
advertised profile requires a rootless runtime, a digest-pinned image, a
non-root user, a read-only root filesystem, dropped capabilities,
`no-new-privileges`, the runtime's default or stricter seccomp profile, no host
namespace sharing, no devices, no host runtime socket, explicit mounts, and
enforceable CPU, memory, PID, and wall-time limits. A rootful runtime needs a
separate operator-approved profile and cannot claim the same capability set.
Use private scratch and artifact staging.

Classify this backend as shared-kernel isolation on native Linux. It is for
trusted or semi-trusted workloads, not hostile multi-tenant code. Backend
capability reporting must reflect platform differences instead of assuming
that Docker Desktop and native Linux have the same boundary. Docker documents
its general security and resource controls in the
[Engine security](https://docs.docker.com/engine/security/) and
[resource constraints](https://docs.docker.com/engine/containers/resource_constraints/)
references.

## 3. Separate-kernel or managed backend

Add one qualified stronger provider after the contract is proven. Candidates
include an OpenShell-managed microVM driver, Docker Sandboxes, or a remote
sandbox service. Selection requires a spike that verifies the exact Xylem
policy matrix, lifecycle API, supported platforms, artifact transfer,
concurrent exec behavior, cancellation, logs, version stability, and CI
operability.

Docker's published sandbox model uses a microVM and a private Docker daemon,
while sharing only declared workspace resources; see its
[security model](https://docs.docker.com/ai/sandboxes/security/). OpenShell
publishes explicit filesystem, process, network, and credential policy
surfaces in its [policy documentation](https://docs.nvidia.com/openshell/dev/sandboxes/policies).
Treat these as provider capabilities to verify, not guarantees Xylem inherits
merely by invoking a CLI.

Do not implement Firecracker directly in this phase. It would make Xylem own
image building, guest agents, networking, snapshotting, scheduling, quotas,
and cleanup before the higher-level contract has stabilized.

# Public contract sketch

Keep the exact field names provisional until the phase-one model is tested.
The intended shape is:

```yaml
sandbox:
  mode: required
  isolation: shared-kernel
  backend: oci
  image: registry.example/xylem-runner@sha256:...
  filesystem:
    inputs: read-only
    modules: read-only
    artifacts: staged
  network:
    default: deny
  credentials:
    raw_environment: deny
  resources:
    cpu: 2
    memory: 2GiB
    pids: 128
    disk: 8GiB
    captured_output: 16MiB
    wall_time: 15m
  lifecycle:
    retention: delete
```

`mode` has only `required` and `off` initially. `off` preserves explicit legacy
host execution and is recorded as unsandboxed; it is never an automatic
fallback. Add `preferred` only if a concrete user need justifies a downgrade
mode, and require an explicit CLI acknowledgement when it is used.

The workflow declaration requests the image, capabilities, and resource
envelope. The operator policy decides which images, backends, paths, network
destinations, and maxima are permitted. Require immutable image identities for
normal execution. Record the resolved image digest even when the user entered
a tag.

# Runtime shape

Introduce three conceptual values:

- `SandboxRequest`: deserialized workflow needs with no authority;
- `EffectiveSandboxPolicy`: an immutable, validated result of operator and
  workflow policy resolution;
- `SandboxSession`: an owned phase-scoped lifecycle with stage, execute,
  snapshot/export, cancel, and close operations.

The session executes the structured command request from the lossless process
contract and returns the same structured result. It does not receive a shell
string. Backend adapters must place `--` before untrusted command arguments
where their CLIs support it and validate every backend-controlled name so
workflow values cannot become runtime flags.

The execution sequence becomes:

```text
parse configuration
  -> validate workflow request and operator policy
  -> resolve configured inputs and build the plan and effective policy
  -> create run workspace and journal
  -> acquire and prepare modules in a network-limited preparation session
     with no workflow inputs or artifact mount
  -> close preparation and provision a network-denied execution session
  -> stage prepared modules read-only and declared inputs read-only
  -> execute ordered modules for admitted inputs into staged artifacts
  -> validate and publish artifacts
  -> finalize journal
  -> destroy or explicitly retain labeled resources
```

Use an RAII owner plus explicit close so ordinary return, error, cancellation,
and panic unwind all attempt cleanup. Label external resources with a Xylem
run ID and policy digest. On startup, reconcile expired Xylem-owned resources
according to an operator TTL; never delete an unlabeled or foreign resource.

The journal and report schema should include backend name/version, isolation
class, session ID, image digest, requested and effective policy digests,
resource-limit termination, artifact publication, and cleanup/retention state.
Never journal raw credentials or credential-bearing URLs.

# CEL workflow inputs

The CEL workflow-input feature evaluates a bounded, side-effect-free expression
inside the trusted planning process. It exposes no filesystem, process,
network, environment, clock, or random functions, so it does not consume this
sandbox boundary. Strings produced by CEL remain ordinary workflow inputs and
receive the same execution-time path capabilities as configured list inputs.

# Interaction with module containers

Keep `ContainerEnvironment` semantically separate from the outer workflow
sandbox. For the first OCI backend, reject module templates that require
Docker/Podman/Apptainer unless the backend advertises a private nested runtime.
Never make them work by mounting `/var/run/docker.sock` or a Podman control
socket from the host.

The managed-container lifecycle plan should later target the session's private
runtime when available. Externally managed host containers remain compatible
only with explicit unsandboxed mode. Dry-run must show this compatibility
decision before execution.

# Implementation phases

## Phase 0: Threat model and backend spike

- Write an ADR that fixes the trust model, isolation unit, supported platform
  matrix, and meaning of each required control.
- Prototype OCI and one separate-kernel/managed candidate against a small
  conformance harness.
- Measure create/exec/destroy latency, concurrent exec behavior, artifact
  transfer, cancellation, and cleanup after controller death.
- Choose the initial runner image contents and reproducible digest publication
  process.

Exit gate: the selected initial backend can prove its advertised controls, and
unsupported controls fail capability negotiation.

## Phase 1: Process and immutable run prerequisites

- Complete lossless process execution, bounded capture, timeout, process-group
  cancellation, and child reaping.
- Complete immutable per-run/per-input environments and remove parent
  environment mutation.
- Add an RAII run workspace and explicit cancellation token.

Exit gate: all direct host execution is represented by one structured runner
interface, and parent state remains unchanged after a run.

## Phase 2: Policy, plan, and fake backend

- Add request, operator policy, effective policy, capability, path mapping,
  resource, lifecycle, and error types.
- Implement pure policy merging and canonical policy hashing.
- Extend execution-plan and journal schemas with sandbox intent and evidence.
- Add fake discovery, preparation, and execution sessions and migrate
  orchestration to them before adding a real runtime.

Exit gate: unit and CLI tests can run the complete orchestration path without
spawning a real workflow process, and every downgrade is rejected or explicit.

## Phase 3: OCI backend

- Build create, stage, exec, cancel, export, inspect, and destroy operations
  for Docker and Podman from structured arguments.
- Apply the hardening defaults and verify the realized runtime configuration
  through inspect output before executing workflow code.
- Move requirement checks, environment creation, module commands, and external
  acquisition helpers into their phase-specific sessions. Prove that
  preparation cannot access inputs or the artifact destination.
- Reject unsupported module-container and granular-egress requests.

Exit gate: the adversarial conformance suite passes on a real rootless runtime,
and a run cannot access the host outside its declared capabilities.

## Phase 4: Artifact, lifecycle, and recovery hardening

- Stage and validate artifacts before host publication.
- Enforce cleanup on success, failure, timeout, cancellation, and panic unwind.
- Reconcile expired labeled resources after abrupt controller termination.
- Add retained-session diagnostics behind an explicit operator policy.

Exit gate: fault-injection tests leave no unaccounted sandbox, child, scratch
directory, or partial artifact publication.

## Phase 5: Stronger provider and secure-by-default rollout

- Implement and qualify one separate-kernel or managed backend.
- Add installation diagnostics without provisioning resources during dry-run.
- Roll out in stages: explicit opt-in, availability telemetry without sensitive
  data, default-on for supported installations, then an explicit unsafe flag
  for legacy host mode.

Exit gate: hostile-workload documentation names a qualified boundary, and
normal supported installations do not execute workflow code on the host.

# Project code boundaries

## Update

- `xylem-lib/src/{execution,error,plan,outcome,progress}.rs` for structured
  runner requests/results, cancellation, policy evidence, and outcomes.
- `xylem-lib/src/config/{mod,validation}.rs` for sandbox requests, operator
  narrowing, guest path resolution, and validation.
- `xylem-lib/src/script/mod.rs` to make command construction pure and remove
  workflow-owned process spawning.
- `xylem-lib/src/{remote,completion}.rs` for acquisition routing and artifact
  publication/marker ordering.
- `xylem-lib/src/journal/` and versioned schemas for sandbox lifecycle
  records.
- `xylem-cli/src/{cli,main}.rs` and `xylem-cli/src/commands/run/mod.rs` for
  trusted policy selection, session ownership, cancellation, cleanup, and exit
  mapping.
- `MODULE_SPEC.md`, architecture/configuration/CLI documentation, examples,
  and the managed-container plans.

## Create

- `xylem-lib/src/sandbox/` with policy resolution, capability negotiation,
  session contracts, fake backend, OCI adapter, and conformance tests.
- Versioned execution-plan and journal/report schema revisions.
- A minimal digest-pinned runner image and reproducible build metadata.
- Adversarial fixtures for filesystem, environment, network, process, resource,
  artifact, cancellation, and cleanup behavior.
- An ADR describing the threat model and backend qualification criteria.

## Delete

- Direct workflow calls to `Script::run_checked` after all callers migrate.
- Production parent-environment mutation through
  `Config::set_environment_variables`.
- Host-side environment creation and executable permission changes for
  sandboxed runs.
- Any implicit sandbox-to-host fallback path.

# Dependencies and ordering

- Follows the upstream [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md) and
  [workflow environment and concurrency](workflow-environment-and-concurrency.md).
- Coordinates with [secure module acquisition](secure-module-acquisition.md):
  acquisition owns provenance and atomic materialization; this plan owns where
  external helpers execute and how verified bytes enter a session.
- Does not gate [CEL workflow input](../features/cel-workflow-input.md), whose
  evaluator has no process or I/O capability.
- Preserves the validation, deterministic plan, resume, journal,
  reporter, and batching contracts.

# Validation

## Pure and fake-backend tests

- Table-test policy intersection, quota minima, deny precedence, unknown
  fields, canonical hashes, unsupported capabilities, and no-downgrade rules.
- Property-test host/guest path mapping against `..`, symlinks, prefix
  confusion, non-UTF-8 paths, duplicate mounts, and special files.
- Verify command and backend CLI argument boundaries with spaces, leading
  dashes, Unicode, empty arguments, and hostile names.
- Fault every lifecycle operation and assert deterministic journal state and
  exactly-once cleanup attempts.

## Real-backend conformance

- Filesystem: read declared input, write staged output, and fail to read host
  home, configuration secrets, journal, sockets, devices, and undeclared paths.
- Environment: observe only the documented baseline and per-command
  `_XYLEM_*` values; prove dotenv and ambient credentials are absent.
- Network: prove deny mode covers DNS, IPv4, IPv6, UDP, loopback, private
  ranges, cloud metadata addresses, and redirects as applicable to the
  backend. Reject an allowlist the backend cannot enforce.
- Phase separation: run a hostile package hook during network-enabled
  preparation and prove that workflow inputs, artifact destinations, and
  credentials are absent; prove no preparation process survives into the
  execution session.
- Process: prove non-root identity, dropped capabilities, no privilege gain,
  no host PID/IPC namespace, bounded PIDs, and no host runtime socket.
- Resources: trigger CPU, memory, PID, disk, output, and wall-time limits and
  assert typed termination plus child reaping.
- Lifecycle: interrupt during create, stage, preparation, execution, export,
  and destroy; then verify immediate cleanup or TTL reconciliation.
- Artifacts: reject traversal, symlinks, hard-link surprises, devices, sockets,
  excessive count/size, and conflicting concurrent publication.

Use fake backends in the default test suite. Run rootless Docker and Podman
conformance as opt-in platform jobs, then make at least one supported backend a
required Linux security job. A stronger provider receives the same suite plus
separate-kernel and private-runtime assertions before documentation recommends
it for hostile workloads.

# Acceptance criteria

- Every workflow-controlled process crosses one sandbox session interface;
  repository checks find no bypass.
- A required policy either has each control enforced or the run is rejected
  before workflow code executes.
- Normal sandboxed runs cannot read ambient secrets, write outside staged
  artifacts/scratch, reach the network by default, access a host container
  socket, or exceed enforced resource and time limits.
- The execution plan, journal, and report identify the backend, isolation
  class, image/snapshot identity, effective policy digest, limit failures,
  artifact publication, and cleanup result without recording secrets.
- Cancellation and all terminal paths reap children and clean or explicitly
  retain labeled resources.
- Unsandboxed compatibility is explicit, auditable, and never selected as a
  fallback.
- Documentation distinguishes shared-kernel containment from a qualified
  separate-kernel or managed boundary and states the remaining risks.
