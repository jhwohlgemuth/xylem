---
type: Plan
title: "Persist a durable execution journal"
description: "Record per-input and per-module state transitions so failed, interrupted, and unstarted work remains inspectable and reportable."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, journal, outcomes, interruption, reporting, reliability]
status: planned
---

# Goal

Persist enough ordered execution state to identify completed, failed, active, skipped, and unstarted inputs after normal failure or interruption. Give final reporters and resume planning a durable source of truth rather than asking either feature to infer state from tracing logs.

# Current evidence

- `Script::run` terminates the process on a failed child command, so orchestration cannot collect a final result.
- `run_workflow_with_progress` returns `()` and emits `Finished` after each module without a failed or skipped progress state.
- The parallel input loop discards every per-input result.
- Reporter and output CLI arguments are present but ignored, and no persistent run state exists.
- The existing [typed outcome](../refactors/typed-error-and-outcome-boundary.md), [process execution](../refactors/process-execution-contract.md), [workflow concurrency](../refactors/workflow-environment-and-concurrency.md), and [run reporter](run-reporters.md) plans already own error classification, child results, scheduling policy, and final presentation. This plan must consume those contracts rather than redefine them.

# Project code boundaries

- **Update:** workflow orchestration integration in `xylem-cli/src/commands/run/mod.rs`, progress event types, CLI output-path handling where journal placement is selected, tests, and operational documentation.
- **Create:** a versioned run-journal/event model in `xylem-lib`, a durable journal sink, recovery/summary readers, and interruption/failure fixtures.
- **Delete:** None. Library exits and discarded results are removed by their owning refactor plans.

# Dependencies and ordering

This implementation is tracked by [work item #11](https://code.ornl.gov/research-enablement/xylem/-/work_items/11). It consumes typed workflow outcomes, lossless child-process results, and deterministic input orchestration. It precedes [resumable execution](resumable-execution.md) and supplies durable state to [structured run reporters](run-reporters.md).

The journal may prototype its standalone serialization and recovery rules early, but CLI integration waits until the outcome and scheduling contracts land.

# Recommendation

## State model

Represent monotonic transitions for the run, input, and module levels. At minimum support `Pending`, `Running`, `Succeeded`, `Failed`, `Skipped`, and `NotStarted`, with timestamps, stable input identity, module identity, bounded error details, and sequence numbers.

Initialize the journal with the complete deterministic input list before execution starts. Under fail-fast policy, stop admitting new work after the first required failure, let already-running work settle according to the concurrency contract, and leave the remainder explicitly `NotStarted`.

## Durability

- Prefer an append-only versioned event journal, such as JSON Lines, or an equivalently crash-tolerant checkpoint design.
- Serialize writes through one owner and flush at defined transition boundaries so concurrent workers cannot interleave records.
- Include a run ID, configuration identity, schema version, start metadata, and finalization event.
- Treat a missing finalization event as an interrupted run; reconstruct the latest state from valid complete records and ignore only a demonstrably truncated final record.
- Keep raw child output bounded or referenced externally rather than duplicating unbounded streams in the journal.

## Presentation boundary

The journal records facts. [Run reporters](run-reporters.md) own console, stdout, and final JSON rendering. Reporter failure must not erase the journal, and reporter code must not rewrite historical transitions.

# Validation

- Round-trip every event and state through the versioned format.
- Run concurrent input transitions and prove records remain individually valid and correctly sequenced.
- Simulate failure before execution, module failure, fail-fast with in-flight work, cancellation, a truncated final record, and a run without finalization.
- Reconstruct the same deterministic summary from an uninterrupted journal and its recovered copy.
- Test output-path errors, flush failures, full storage, and reporter failure without losing the original execution error.

# Acceptance criteria

- Every discovered input has a durable initial state before module execution begins.
- Child and module failures reach orchestration and are recorded with input and module identity.
- Interrupted or incomplete runs remain readable and distinguish running from never-started work.
- Journal writes are versioned, ordered, bounded, and safe under concurrent execution.
- Overall command failure remains non-zero and independent of final reporter rendering.
- Reporters and resume logic can consume the journal without scraping logs.

# Related plans

- [Typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md)
- [Process execution contract](../refactors/process-execution-contract.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Structured run reporters](run-reporters.md)
- [Resumable execution](resumable-execution.md)

