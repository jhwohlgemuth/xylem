---
type: Plan
title: "Manage workflow container lifecycle"
description: "Optionally create, verify, and clean up Docker or Podman containers declared by workflow modules."
difficulty: medium
impact: high
package: [xylem-lib, xylem-cli]
tags: [containers, docker, podman, workflow, cleanup]
status: planned
---

# Goal

Allow a workflow to declare the container it needs and have Xylem create it
before module execution, wait until it is usable, and clean it up according to
an explicit ownership policy. Existing configurations that name an externally
managed container must remain supported.

# Current evidence

- `ContainerEnvironment` currently carries only a container name, runtime,
  context, and image fields.
- Container-backed scripts use `<runtime> exec <name> ...`; the kitchensink
  example therefore requires users to run `podman run` manually before Xylem.
- The run workspace and journal already establish a run identifier, durable
  lifecycle records, and a CLI-owned cleanup boundary.
- Docker and Podman share the required `run`, `inspect`, `exec`, and `rm`
  vocabulary for a narrow initial implementation.

# Recommendation

Add an optional `container` lifecycle declaration alongside the existing
execution container fields:

```json
{
  "name": "xylem-kitchensink",
  "runtime": "podman",
  "image": "alpine:3.22",
  "create": true,
  "arguments": ["sleep", "infinity"],
  "cleanup": "on_success"
}
```

- Preserve `create: false` as the default, retaining current `exec` behavior.
- Require `image` when `create` is true; reject lifecycle-only fields when the
  container is externally managed.
- Before preparation, group compatible declarations by `(runtime, context,
  name)`. Reject conflicting declarations rather than choosing one silently.
- For owned containers, perform `run --detach --name ...`, then poll bounded
  `inspect` state until running. Treat an existing container as an error unless
  it belongs to the same retained run workspace.
- Execute module commands only after readiness succeeds. A failure to create,
  inspect, or start a container is an availability failure with the runtime,
  container name, and safe command context.
- Record create, ready, and cleanup transitions in the execution journal and
  include owned container details in JSON and console reports.
- Clean up owned containers with `rm --force` after a successful run by
  default. Support `always`, `never`, and `on_success`; never remove an
  externally managed container.
- Dry runs validate declarations and render lifecycle intent without invoking
  the runtime or allocating a container workspace.

# Project code boundaries

## Update

- `xylem-lib/src/script/mod.rs` and configuration validation for lifecycle
  declarations and Docker/Podman command construction.
- `xylem-lib/src/{config,execution,journal,progress,outcome}.rs` for lifecycle
  orchestration, state, cancellation, and reporting.
- `xylem-cli/src/commands/run/mod.rs` for CLI-owned workspace lifetime,
  cleanup, exit mapping, and dry-run rendering.
- `docs/src/configuration.md`, `docs/src/architecture.md`, `MODULE_SPEC.md`,
  and `examples/kitchensink/` for the public contract and runnable example.

## Create

- Hermetic fake-runtime fixtures that record `run`, `inspect`, `exec`, and
  `rm` invocations without requiring Docker or Podman.
- Schema coverage for lifecycle declarations and journal/report records.

## Delete

- The kitchensink README requirement to start its owned Podman container
  manually once the example opts into `create: true`.

# Validation

- Test externally managed compatibility, owned creation, readiness timeout,
  conflicting declarations, runtime/image validation, and all cleanup modes.
- Test Docker and Podman command rendering with fake runtimes.
- Verify fail-fast and cancellation remove owned containers exactly once and
  never remove external containers.
- Verify dry-run performs no runtime invocation and renders planned lifecycle
  actions.
- Run the kitchensink example against the fake runtime in CI; retain one
  opt-in integration job for each available real runtime.

# Acceptance criteria

- A workflow can opt into an owned Docker or Podman container without an
  external setup command.
- Every owned container is either cleaned up according to its declared policy
  or intentionally retained with its name reported to the user.
- External containers remain untouched.
- Lifecycle failures are contextual, journaled, and do not leave orphaned
  containers behind.

# Dependencies

- Build on [lossless process execution](process-execution-contract.md) for
  bounded command execution and cancellation.
- Coordinate with [workflow environment and concurrency](workflow-environment-and-concurrency.md)
  for workspace ownership and cleanup ordering.
- Consume [structured run reporters](../features/run-reporters.md) and the
  durable journal rather than introducing a separate event stream.
