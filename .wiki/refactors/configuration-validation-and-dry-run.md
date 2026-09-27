---
type: Plan
title: "Configuration validation and side-effect-free planning"
description: "Implemented parse, validate, plan, and execute staging with deterministic write-free dry-run."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, configuration, validation, dry-run, planning, paths]
status: complete
---

# Outcome

Implemented as Phase 3 of [resumable execution](../features/resumable-execution.md).
This record is not a schedulable work item.

- Parsing preserves configuration path, format, and parser source context.
- Semantic validation rejects unsupported capabilities and returns ordered,
  path-qualified multi-error diagnostics.
- Planning resolves deterministic input order and duplicate handling,
  CLI-over-configuration precedence, artifact destinations, variables,
  acquisition actions, command information, and completion declarations.
- `output` supplies the default `OUTPUT_DIRECTORY`; `--artifact-output`
  overrides configured artifact destinations, independently of report output.
- Dry-run stops after planning and performs no workspace, journal, marker,
  artifact, permission, network, environment, or child-process mutation.
- JSON and YAML produce equivalent plans, and checked-in example
  configurations pass semantic validation.

# Evidence

- `xylem-lib/src/config/validation.rs`
- `xylem-lib/src/plan.rs`
- `docs/schema/execution-plan-v1.schema.json`
- `xylem-cli/tests/workflow.rs`
