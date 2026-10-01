---
type: Plan
title: "CEL workflow input"
description: "Derive workflow inputs with a bounded Common Expression Language expression while preserving deterministic planning and CLI precedence."
difficulty: medium
impact: medium
package: [xylem-lib, xylem-cli]
tags: [feature, input, cel, planning, determinism]
status: planned
priority: backlog
---

# Goal

Support `InputAttribute::Cel` as an explicit, side-effect-free input derivation
mechanism. A CEL expression evaluates to the same ordered, deduplicated input
model used by list and directory inputs while preserving the parse, validate,
plan, execute boundary.

CEL replaces the reserved `InputAttribute::ShellScript` variant. The feature is
not active today, so there is no shell-input compatibility contract to retain.
Unlike a shell command, a CEL program can be parsed and evaluated during normal
write-free planning because it receives only immutable values supplied by
Xylem and cannot perform I/O.

# Non-goals

- Discover files, inspect directories, fetch remote data, or execute processes.
- Expose the parent process environment, credentials, clocks, randomness, or
  host-specific CEL extensions.
- Turn CEL into a general workflow language or use it for module execution,
  completion checks, policy decisions, or template expansion in this feature.
- Preserve the unpublished `ShellScript(ScriptTemplate)` configuration shape.
- Add user-defined functions or a plugin ABI.

# User-facing contract

Replace the reserved shell payload with a structured CEL request:

```yaml
variables:
  DATASETS: [alpha, bravo, alpha]
  INPUT_ROOT: /data/scenes
input:
  cel: >-
    variables.DATASETS.map(name, variables.INPUT_ROOT + '/' + name)
```

The JSON form is `{"input":{"cel":"..."}}`. Define a deny-unknown-fields
`CelInput` type with one required, non-empty `cel` expression and add
`InputAttribute::Cel(CelInput)`. The object wrapper keeps CEL unambiguous within
the existing untagged string/list/object input syntax.

The expression environment contains one binding:

- `variables`: the workflow's configured `variables` object, converted
  recursively from JSON/YAML values without reading `_XYLEM_*` or the parent
  process environment.

Evaluate after nested configuration resolution has produced the effective
root workflow and before module-specific environment strings are constructed.
Document the accepted JSON-to-CEL mapping for null, boolean, integer, unsigned
integer, floating-point, string, list, and map values. Reject values that the
selected interpreter cannot represent without loss; do not silently stringify
them.

The result must be `list<string>`. Reject null, scalar, map, bytes, and mixed
lists; reject empty strings and values containing NUL. Preserve expression
order and remove later duplicates with the same first-occurrence policy used
for configured lists and CLI inputs. An empty result retains the existing
single no-input execution represented internally as `vec![None]`.

CEL output strings have exactly the same path interpretation as strings in an
ordinary configured input list. Evaluation does not assert that a path exists,
canonicalize it, or grant access to it. Later planning and execution boundaries
remain responsible for any path validation or sandbox capability.

# Evaluation boundary

Create a small capability-owned boundary with separate compile and evaluate
operations. Configuration validation checks the wrapper and non-empty source.
After CLI precedence selects configured CEL input, planning compiles the
expression, reports syntax or declaration errors at `input.cel`, evaluates the
program against the immutable `variables` value, and normalizes its result.

Use only CEL standard operators, macros, and pure functions implemented with
the semantics required by the CEL specification. Do not register filesystem,
process, network, environment, time, random, or locale-sensitive functions.
Pin the interpreter version and verify the required language subset with
upstream conformance cases before accepting the dependency. If the Rust
interpreter cannot enforce the limits below or differs on required list, map,
numeric, string, or macro behavior, narrow and document the supported subset
rather than claiming full CEL compatibility.

Enforce explicit limits for expression bytes, parsed AST depth and node count,
evaluation cost, result records, and aggregate result bytes. The selected
interpreter must expose deterministic cost accounting or an equivalent bounded
evaluation mechanism; wall-clock timeout alone is not an acceptable cost
limit. Apply limits before converting the result into workflow inputs.

Return a typed CEL input error that distinguishes parse, check, binding,
evaluation, result-type, and limit failures. Preserve interpreter diagnostics
with source spans where available, but translate them at the Xylem boundary so
the public error contract is not an interpreter crate's error type.

# Planning and CLI precedence

CLI `--input` remains highest precedence. A non-empty CLI input list bypasses
CEL compilation and evaluation just as it bypasses configured list and
directory inputs. This ensures an invalid or expensive unused expression
cannot prevent an explicit CLI override from running.

Without a CLI override, ordinary execution and `--dry-run` use the same CEL
compile, evaluate, and normalization path. CEL evaluation performs no writes,
network requests, process spawns, directory reads, or runtime provisioning, so
dry-run renders the complete final execution plan rather than an unresolved
static plan.

Extend the versioned execution-plan input section with:

- source kind `cel`;
- a digest of the original expression;
- the CEL language/version and interpreter identity;
- the effective limit profile; and
- the normalized resolved inputs.

Do not place the full expression or configured variable values in plan or
journal metadata merely to provide provenance; either may contain sensitive
configuration. A plan-schema change must use a new schema version and file
rather than modifying a released schema in place. CEL needs no discovery
journal transition because all inputs are known before journal creation.

# Implementation phases

## Phase 1: Interpreter qualification and typed boundary

Choose and pin a Rust CEL interpreter through the dependency-review process.
Define the supported CEL subset, JSON/YAML conversion table, resource limits,
compile/evaluate interface, and typed errors. Add focused conformance tests for
the operators, macros, value types, and deterministic cost behavior this
feature exposes.

## Phase 2: Configuration and normalization

Replace `ShellScript(ScriptTemplate)` with the structured `Cel(CelInput)`
variant. After CLI precedence is resolved, compile and evaluate selected CEL
input during planning and route its `list<string>` result through the existing
ordered deduplication path. Keep CLI precedence outside the evaluator so an
override does not touch CEL.

## Phase 3: Plan, journal, and CLI integration

Record bounded CEL provenance and resolved inputs in a new execution-plan
schema. Preserve the current pre-journal input boundary, scheduler admission,
resume behavior, and reporter semantics. Ensure the normal run and dry-run
commands call the same pure resolution path.

## Phase 4: Documentation and fixtures

Document the configuration shape, available binding, supported CEL subset,
type conversion, limits, output rules, CLI precedence, and diagnostics. Add
equivalent JSON and YAML fixtures plus compiled-CLI integration coverage.

# Validation

- Unit tests for parse and declaration errors, missing variables, result type,
  empty strings, NUL, mixed lists, duplicates, ordering, empty output, numeric
  conversion, and every configured limit.
- Conformance tests for each supported CEL operator, macro, and value type.
- Property tests that normalization preserves first occurrence order and is
  idempotent.
- Integration proof that CLI inputs bypass CEL compilation and evaluation.
- Dry-run proof that CEL resolves to the same inputs and byte-identical plan as
  normal pre-execution planning without process, filesystem, network, or write
  access.
- Determinism proof that the same expression and variables produce
  byte-identical normalized inputs and provenance on every supported platform.
- Regression proof that string, list, directory, batching, and no-input
  behavior remain unchanged.
- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`

# Acceptance criteria

- A valid CEL expression deterministically resolves to
  `Vec<Option<String>>` through the shared normalization path.
- Invalid expressions and results produce typed, path-qualified diagnostics.
- CEL evaluation can access only the documented immutable binding and pure
  language subset.
- Evaluation and output are bounded by deterministic, tested limits.
- Dry-run produces the final resolved plan without external effects or a
  second planning stage.
- CLI input precedence and all existing input forms remain unchanged.

# Dependencies

- Coordinates with [dependency and security gates](../refactors/dependency-surface-and-security-gates.md)
  for interpreter selection, pinning, licensing, and advisory policy.
- Preserves the input scheduling contract established by
  [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md).
- Does not depend on [sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md)
  because CEL evaluation has no process or I/O capability.
