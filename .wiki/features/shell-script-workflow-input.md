---
type: Plan
title: "Shell-script workflow input"
description: "Resolve workflow inputs from a configured shell script without weakening deterministic planning or execution safety."
difficulty: high
impact: medium
package: [xylem-lib, xylem-cli]
tags: [feature, input, shell, planning, security]
status: planned
priority: backlog
---

# Goal

Support `InputAttribute::ShellScript` as an explicit input-discovery mechanism.
The script must produce the same ordered, deduplicated input model used by list
and directory inputs while preserving the parse, validate, plan, execute
boundary.

# Contract decisions

- Define stdout as the sole input channel, with one UTF-8 path per line.
- Reject non-zero status, invalid UTF-8, empty records, and output above a
  documented byte and record limit.
- Run discovery before plan construction and represent discovery failures as
  typed input errors with captured status and bounded stderr.
- Preserve first occurrence order and apply the same duplicate policy as CLI
  and configured list inputs.
- Make the side effect explicit: normal planning may execute input discovery,
  while a future strict no-process planning mode must reject it.
- Apply the workflow's remote, environment, and working-directory policy
  deliberately; do not inherit arbitrary process-global variables.

# Implementation phases

## Phase 1: Typed discovery boundary

Add a capability-owned input discovery type and error enum. Move list and
directory resolution behind that boundary so shell discovery shares the same
normalization, duplicate handling, limits, and diagnostics.

## Phase 2: Bounded shell execution

Execute the configured `ScriptTemplate` through the checked process layer.
Capture stdout and stderr without deadlock, enforce time and output limits, and
parse stdout into input records only after successful completion.

## Phase 3: Planning and CLI integration

Replace the current semantic-validation rejection with discovery during input
resolution. Record the discovery source in the execution plan and keep CLI
inputs at highest precedence so an explicit CLI list does not spawn the
configured discovery script.

## Phase 4: Documentation and fixtures

Document quoting, working directory, environment, line format, limits, and
failure behavior. Add JSON and YAML fixtures plus compiled-CLI integration
coverage.

# Validation

- Unit tests for empty output, duplicates, ordering, invalid UTF-8, non-zero
  status, oversized output, timeout, and bounded stderr.
- Integration proof that CLI inputs bypass the shell script.
- Determinism proof that identical stdout yields byte-identical plans.
- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`

# Acceptance criteria

- Valid shell output resolves to deterministic `Vec<Option<String>>` inputs.
- Every process and parsing failure is typed and path-qualified.
- Input discovery cannot silently inherit unrestricted environment state or
  produce unbounded memory use.
- CLI input precedence remains unchanged.

