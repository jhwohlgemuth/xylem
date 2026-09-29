---
type: Plan
title: "Make process execution lossless and deadlock-free"
description: "Give Script execution a structured result, safe argument parsing, concurrent output capture, and explicit timeout and exit semantics."
difficulty: high
impact: critical
package: [xylem-lib]
tags: [refactor, process, command, stdout, stderr, deadlock, arguments]
status: planned
---

# Goal

Execute module commands without pipe deadlocks or argument corruption, and return enough information for workflow policy to distinguish output from failure.

# Current evidence

- `Script::run` pipes both stdout and stderr, drains stdout to EOF, then drains stderr. A child that fills stderr while stdout remains open can block both processes indefinitely.
- After manually draining both pipes, it calls `wait_with_output`; output bytes in that return value are no longer meaningful.
- Any spawn, capture, wait, or non-zero-exit failure returns `Vec::new()`, which is indistinguishable from a successful silent command.
- Stderr lines are appended to the same string vector as stdout, losing stream identity and ordering.
- `Instruction::ShellFormat` is parsed with `split_whitespace`, so quoting, escaped spaces, and empty arguments are not preserved.
- Display formatting joins arguments with spaces without shell escaping, so logged commands are ambiguous and cannot be copied safely.
- There is no timeout, cancellation, output-size bound, signal status, or child cleanup contract.

# Project code boundaries

- **Update:** `xylem-lib/src/script/mod.rs`, its tests, template fixtures, and public documentation.
- **Create:** a process execution module if extraction makes ownership clearer; structured `ExecutionResult`, `CapturedOutput`, and execution-option types; fake child-process fixtures.
- **Delete:** `Script::run -> Vec<String>`, sequential manual pipe draining, `split_whitespace` parsing, and empty-vector error fallbacks.

# Dependencies and ordering

Consumes the landed typed-error and workflow-outcome taxonomy. It precedes
[workflow environment and concurrency](workflow-environment-and-concurrency.md),
secure acquisition subprocesses, and reporters.

# Recommendation

## Command construction

- Keep the executable and argument vector structured from deserialization through `std::process::Command`.
- Prefer exec-format arrays. Parse the legacy shell-format string with a documented cross-platform parser, or deprecate it if its quoting semantics cannot be made portable.
- Treat display rendering as diagnostic-only and escape every argument so boundaries are visible.
- Reject an empty command and unsupported container or environment manager before spawning.

## Execution and capture

- Return status, signal/termination information, separately captured stdout and stderr, duration, and truncation metadata.
- Drain both pipes concurrently or use a primitive that does so. Preserve stream identity; document whether cross-stream temporal order is retained.
- Bound captured bytes and lines. Stream to configured sinks for long-running jobs while keeping a bounded diagnostic tail.
- Add timeout and cancellation behavior that terminates and reaps the entire child process group where the platform permits.
- Preserve non-UTF-8 output as bytes or use an explicitly documented lossy conversion at the presentation boundary.

## Failure policy

- A non-zero status is a typed process failure with the captured diagnostic tail.
- Spawn, working-directory, read, wait, timeout, and cancellation failures are distinct.
- The caller decides whether a non-zero module is fatal; `Script` never logs and silently converts it to success.

# Validation

- Test exact argument preservation for spaces, quotes, escapes, empty values, Unicode, and template expansions.
- Use helper programs to emit enough stderr to fill a pipe while stdout stays open; assert completion within a bounded time.
- Test interleaved output, silent success, non-zero exit, signal termination, missing executable, bad working directory, timeout, cancellation, output truncation, and non-UTF-8 bytes.
- Test direct, virtual-environment, and Docker/Podman/Apptainer argument assembly without requiring those tools to be installed.
- Run tests on Linux, macOS, and Windows with platform-specific process-group assertions gated narrowly.

# Acceptance criteria

- No child can deadlock because one captured stream is drained after the other.
- `Script` returns a structured result or typed error; empty output is never an error sentinel.
- Stdout and stderr remain distinct and bounded.
- Non-zero exit, timeout, cancellation, and signal termination are observable by workflow policy.
- Exec-format arguments round-trip exactly, and supported shell-format quoting has documented tests.
- Every spawned child is waited or terminated and reaped.

# Related plans

- [Workflow environment and concurrency](workflow-environment-and-concurrency.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
