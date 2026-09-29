---
type: Plan
title: "Add opt-in managed-container keepalive"
description: "Let managed Docker and Podman containers request a documented idle startup command without changing the default command supplied by the image."
difficulty: medium
impact: medium
package: [xylem-lib, xylem-cli]
tags: [containers, configuration, lifecycle, docker, podman]
status: implemented
---

# Goal

Add `keep_alive: true` to managed container configuration as an explicit shorthand for an idle startup command. The default remains `false`, so omitting both `keep_alive` and `command` continues to use the image's `ENTRYPOINT` and `CMD`.

# Current evidence

- `ContainerEnvironment.command` is an optional argument vector appended after the image in `docker run` or `podman run`.
- A managed container with no configured command uses the image default. This is required for service images whose normal process must remain intact.
- The kitchensink Alpine container sets `command: ["sleep", "infinity"]` because Alpine's default shell exits before Xylem can use `podman exec`.
- Validation rejects an empty command but has no keepalive vocabulary or conflict rule.
- `PlannedContainer` reports creation, cleanup, image, mounts, runtime, and readiness timeout, but not the effective startup strategy.
- There is no portable keepalive executable shared by every Linux, distroless, Windows, and custom container image.

# Project code boundaries

- **Update:** `ContainerEnvironment` and its validation in `xylem-lib/src/{script/mod,config/validation}.rs`, container argument construction in `xylem-lib/src/container.rs`, dry-run output in `xylem-lib/src/plan.rs`, configuration tests, lifecycle tests, `MODULE_SPEC.md`, and `examples/kitchensink/`.
- **Create:** focused tests for keepalive resolution, configuration conflicts, dry-run rendering, and runtime arguments.
- **Delete:** the explicit kitchensink `command: ["sleep", "infinity"]` after the example opts into `keep_alive`.

# Dependencies and ordering

Build on the managed-container lifecycle and its `args!`-based runtime boundary. Coordinate changes to `ContainerEnvironment`, `PlannedContainer`, and the execution-plan schema with sandboxed workflow execution so providers can reject unsupported startup strategies before creating a container.

# Recommendation

1. Add `keep_alive: bool` with a Serde and builder default of `false`.
2. Permit `keep_alive: true` only when `create: true` and the runtime is Docker or Podman. Reject it for external containers and Apptainer.
3. Reject configurations that set both `keep_alive: true` and `command`. These fields select different startup strategies and should not have precedence rules.
4. Resolve startup arguments in one pure `ContainerEnvironment` method:
   - `keep_alive: true` resolves to `args!["sleep", "infinity"]`;
   - an explicit `command` remains unchanged;
   - neither field produces arguments, allowing the image default to run.
5. Pass the resolved vector directly to the runtime without a shell. Keep argument boundaries visible in errors and dry-run output.
6. Add a startup-strategy field to `PlannedContainer` and increment the execution-plan schema version. Render `image_default`, `explicit_command`, or `keep_alive` without creating the container.
7. Report a container that exits during readiness with its selected startup strategy and a hint that the image may not provide a compatible `sleep` command.
8. Replace the kitchensink command with `keep_alive: true`. Document that the initial implementation supports Linux images containing a `sleep` implementation that accepts `infinity`.

# Validation

- Round-trip `keep_alive` through JSON and YAML with `false` as the omitted default.
- Reject `keep_alive` with `create: false`, Apptainer, an unknown runtime, or an explicit command.
- Assert exact `args!` output for image-default, explicit-command, and keepalive startup strategies.
- Verify dry-run reports the strategy and performs no runtime invocation.
- Exercise success, early exit, timeout, cleanup, and `--container-cleanup never` with the fake runtime.
- Run the kitchensink workflow with Podman and verify its container accepts `exec`, is removed by default, and is retained when cleanup is overridden.

# Acceptance criteria

- Existing configurations retain their current image-default or explicit-command behavior.
- `keep_alive` is opt-in and never overrides an image command silently.
- Invalid combinations fail configuration validation before runtime work begins.
- Dry-run exposes the exact startup strategy without side effects.
- The kitchensink example no longer spells out `sleep infinity`.
- Compatibility limits and cleanup behavior are documented and tested.

# Related plans

- [Sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md)
- [Managed container lifecycle](../refactors/managed-container-lifecycle.md)
