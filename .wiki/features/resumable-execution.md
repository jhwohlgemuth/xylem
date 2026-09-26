---
type: Plan
title: "Add resumable execution and output completion checks"
description: "Declare expected module outputs, classify completed work before scheduling, and safely skip work during explicit resume runs."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, resume, output, completion, fingerprint, idempotency]
status: planned
---

# Goal

Let users rerun a full input directory while Xylem identifies work whose expected outputs are already complete. Keep module implementations free of repeated `output_filename.exists()` loops and record every skip as an explicit execution outcome.

# Current evidence

- `Config.output` documents an artifact path but is not used by execution.
- The top-level `Check` configuration is not integrated into workflow execution and does not define per-input, per-module completion identity.
- Modules conventionally receive `OUTPUT_DIRECTORY` as a user variable, but Xylem does not know the filenames or file set that prove a module completed.
- The current workaround must be implemented inside each image-processing module and may treat partial files as complete.
- Xylem's scheduling unit is a top-level input path. It cannot promise pair-level resume when a child module internally loops over multiple image pairs.

# Project code boundaries

- **Update:** configuration and module declarations, semantic validation, input planning, orchestration skip handling, progress states, module/configuration specifications, examples, and tests.
- **Create:** completion-predicate types, template-expanded output matching, workflow/module fingerprinting, resume planning, and output fixtures for complete, partial, stale, and missing cases.
- **Delete:** None. Module-local existence workarounds may be removed from downstream modules only after their completion declarations are proven equivalent.

# Dependencies and ordering

This implementation is tracked by [work item #12](https://code.ornl.gov/research-enablement/xylem/-/work_items/12). It consumes the existing durable execution journal, [configuration validation](../refactors/configuration-validation-and-dry-run.md), and deterministic scheduling from [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md). The [fixed input concurrency](fixed-input-concurrency.md) contract remains in force for work that is not skipped.

# Recommendation

## Completion declaration

Add an optional module-level completion declaration whose paths or globs use the existing Xylem variable interpolation contract:

```yaml
completion:
  outputs:
    - "{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/*_MODO.tif"
  require: all
```

Validate empty output sets, unsupported glob syntax, unresolved reserved variables, and output paths outside allowed roots before execution. Initially define completion as all declared outputs existing and being non-empty. Do not infer completion from arbitrary files in an output directory.

## Resume policy

- Require an explicit `options.resume` value or `--resume` flag; normal runs do not silently skip work.
- Resolve and evaluate completion predicates during side-effect-free planning before admitting inputs to the scheduler.
- Record completed work as `SkippedExisting` in the journal and progress stream.
- Combine the completion check with a fingerprint of relevant workflow, module, template, variables, and completion configuration. A stale fingerprint forces re-execution unless the user selects a separately specified trust-existing policy.
- If no completion declaration exists, run the input and explain why it cannot be skipped safely.
- Make dry-run show the same run/skip decisions without creating outputs or journal transitions that claim real completion.

## Granularity

Document that the resume unit equals the Xylem input unit. To resume individual pairs, input discovery must expose each pair or a future module protocol must enumerate pair-level work and expected outputs.

# Validation

- Test complete, missing, zero-length, partial, stale-fingerprint, changed-variable, unresolved-template, and invalid-glob cases.
- Test multiple modules where an early module is complete but a later module is not.
- Test dry-run, explicit resume disabled, duplicated inputs, non-Unicode paths, and deterministic skip ordering.
- Prove skipped modules are never spawned and their journal/progress states are distinct from successful execution.
- Exercise a representative imagery workflow at its actual input granularity.

# Acceptance criteria

- Resume behavior is explicit and absent from ordinary runs unless enabled.
- Completion is declared outside module implementation and validated before execution.
- Complete work is skipped without spawning its module; partial or stale work reruns.
- Missing and zero-length outputs do not count as complete.
- Every skip is visible in progress, journal state, and final reporters.
- Documentation states the supported resume granularity and fingerprint policy.
- Module/configuration specifications and end-to-end examples cover the new contract.

# Related plans

- [Configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md)
- [Fixed input concurrency](fixed-input-concurrency.md)
- [Structured run reporters](run-reporters.md)

