---
type: Plan
title: Consistent variable expansion across workflow configuration
description: Resolve variables in input and output paths, module sources, templates, and execution environments before their owning validation and preparation steps.
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [configuration, variables, interpolation, acquisition]
status: planned
priority: next
execution: phased
phase_budget_hours: 5
---

# Consistent variable expansion across workflow configuration

## Outcome

A workflow author can use `{{ NAME }}` in the supported path and execution
fields below and obtain one effective value shared by validation, planning,
preparation, execution, and resume. Each field has an explicit variable scope
and resolution time. Complete the five expansion gaps identified by the review
without performing unrestricted substitution over the entire configuration.

This plan owns configuration-field expansion. Named `--var` bindings and
config-declared positional invocation are accepted behavior to preserve.

## Current evidence and review findings

Reviewed on 2026-11-13 at clean baseline `e3f0e09`. The `INPUT_BASENAME`
hard cutover and bounded selection-root interpolation are committed and are
baseline behavior to preserve. `SelectionOptions::resolve` now owns the shared
host/sandbox option-resolution entry point, while the bounded implementation
still lives in `selection/roots.rs`; extracting it remains EXP-01 work.
Named `--var` bindings are implemented in the current worktree; field phases
can consume their invocation precedence after EXP-01.

The full library suite passed again (336 passed, two ignored). Current code,
configuration docs, CLI/help, and tests confirm the five gaps below remain;
no EXP phase has landed. The earlier temporary dry-run observations are review
evidence, not newly rerun probes or acceptance of future expansion work.

| Review priority | Remaining gap | Repository evidence | Observable consequence |
|---|---|---|---|
| 1 | Plain `input` directory strings and list entries | `Config::resolve_inputs`, `crates/xylem-lib/src/plan.rs:271`, directly reads a path or clones list/CLI values. | A temporary dry run retained `{{ INPUT_PATH }}` in selected input identity while its command argument later became `/data/example.tif`. Discovery, reporting, and command execution can disagree. |
| 2 | Workflow `output` / `artifacts` | `prepare_config_with_output`, `crates/xylem-cli/src/commands/run/preparation.rs:164`, normalizes the authored string. | A temporary dry run produced `<config-directory>/{{ OUTPUT_ROOT }}/artifacts`, preserving the placeholder in the artifact path. |
| 3 | Module source `uri` | `Config::resolve_at` loads nested sources and validates leaf locations before constructing the effective module variable map. | A variable-selected source is interpreted as an authored URI/path without interpolation. Expansion must precede acquisition policy and immutable-source checks. |
| 4 | Template filename and `template.command` | `Script::from_module` loads the authored filename; `Script::expand_arguments_from` changes arguments only. | The dry-run probe retained `{{ COMMAND }}` as the executable even when `COMMAND: echo` was configured. A variable-selected template filename can fall through to the default template. |
| 5 | Container and virtual-environment settings | `Config::container_declarations`, `ContainerEnvironment::create_managed`, and `Module::resolve_virtual_environment` consume literal settings. | Image names, mount paths, logical container names, startup arguments, and environment names cannot reliably share configured variables. |

The review concerns public configuration behavior in the Rust library and its
CLI adapter. These are follow-up interface gaps, not newly introduced Rust
safety findings. The applicable review principle is to document and test
behavior not expressed by types: `.agents/skills/rust-best-practices/SKILL.md`,
section 10, and the repository's workflow-format consistency rules in `AGENTS.md`.

Existing behavior to retain:

- Selection roots expand recursively through scalar workflow variables before
  discovery and sandbox authorization, with bounded depth, substitutions, and
  output size. CEL receives the original typed variables and canonical roots.
- Argument interpolation, module-variable precedence, completion path expansion,
  input-local workspaces, and environment isolation already work.
- `INPUT_BASENAME` is the only built-in basename name. Do not reintroduce
  `PARENT_DIRECTORY` or reserve it again as a built-in.
- Acquisition verification, redirect/host checks, immutable cache identities,
  sandbox grants, symlink checks, and `.pixi/envs/` exclusion remain authoritative.

## Chosen contract

### Scope and timing

Definitions:

- **Workflow bindings:** declaring workflow variables plus inherited workflow
  bindings where nesting applies, with invocation overrides highest.
- **Module bindings:** existing workflow/wrapper/child/module precedence, with
  invocation overrides highest. Values needed to select a child configuration
  can use its parent's and wrapper's bindings; they cannot use values inside
  the child that has not been loaded yet.
- **Invocation overrides:** the immutable named/positional bindings. They must
  remain separately available through nested resolution,
  rather than being indistinguishable from an ordinary root default.

| Field | Bindings | Required resolution boundary |
|---|---|---|
| Configured `input` directory and list elements; selected `--input` elements | Workflow | Before discovery, duplicate removal, input admission, and input identity/provenance construction. |
| `input.select.options.roots` | Workflow | Preserve the current before-discovery and before-sandbox-authorization behavior. |
| Workflow `output` / `artifacts` | Workflow | Before path normalization, artifact policy checks, default `OUTPUT_DIRECTORY`, and workspace/context preparation. |
| Leaf module local `uri` | Module | Before URI interpretation, rebasing, validation, materialization, and acquisition-plan identity. |
| Remote leaf and nested configuration `uri` | Module at the declaring node | Before cycle identity, host/scheme checks, fetch/cache lookup, and checksum/revision verification. |
| String-valued `template` path | Module | Before lookup/loading and existing containment checks; relative to the materialized module under existing rules. |
| Inline or external template `command` | Module | After loading the template but before requirement/command validation and command construction; dry-run and execution use the same value where the template is available. |
| `virtual_environment.name` | Module | Before environment preparation, existence checks, and launch command construction. |
| `container.image`, `container.name`, each `container.command` element | Module | Before container declaration deduplication, validation, creation, and deterministic owned-name construction. |
| `container.mounts[].source` and `.target` | Module | Before source rebasing/canonicalization and target/mount validation. |

These newly supported structural fields use static workflow/module bindings.
Input-derived and run-derived built-ins are unavailable in them. Existing
per-input argument and completion-path behavior stays available. A workflow
can use an explicitly configured `OUTPUT_DIRECTORY` where it is in scope;
resolving `output` must not invent a circular default from that same field.

### Expansion and error semantics

1. Extract the bounded selection-root resolver into a crate-internal shared
   component. Preserve its existing root behavior and expose a field-qualified
   scalar-string expansion operation. Do not change the permissive legacy
   argument resolver as an incidental part of extraction.
2. Reuse `{{ NAME }}`, recursive references, string/number/boolean conversion,
   and deterministic limits: 32 reference levels, 10,000 substitutions per
   resolution batch, and 1 MiB per expanded/intermediate string. Bindings are
   read-only; no process-environment lookup, shell evaluation, or JSON inference.
3. New structural expansion rejects unknown or unavailable names, cycles,
   malformed placeholders, referenced arrays/maps/null, NUL, and exceeded
   limits before the relevant side effect. Field validators then enforce their
   existing rules for emptiness, syntax, schemes, paths, images, and names.
   Empty startup argument strings remain legal when existing argv rules allow
   them; an empty executable or source URI does not become valid.
4. Errors identify the configuration/module and exact field or element index,
   without dumping variable maps or credentials. Preserve the owning API's
   typed error chain and CLI data-error classification.
5. Expansion resolves references inside configured bindings, then renders the
   selected field. Do not repeatedly interpret a resolved filesystem value as
   another configuration template in later phases. All consumers receive the
   same effective value; retain authored text only where needed for diagnostics.
6. Literal configurations retain their behavior. Preserve each input form's
   existing path anchoring, order, and duplicate policy; this is not a migration
   of plain-input relative-path semantics. `--input` still replaces configured
   selection, including bypassing invalid or unused configured roots. Its
   selected elements gain the explicitly documented workflow interpolation.
7. `--artifact-output` keeps highest precedence and remains a literal CLI path.
   An overridden, unused configured `output` must not fail interpolation.
   Preserve module `OUTPUT_DIRECTORY` defaults/overrides and the existing
   stronger CLI artifact override.
8. An expanded command is one executable name/path, never a shell command line.
   Preserve argument boundaries, including spaces, quotes, and empty strings.
9. No invocation-scoped values leak into another run. Resume must run when a
   relevant effective value changes and skip only when the existing completion
   contract is satisfied. Do not omit bindings from fingerprints to manufacture
   backwards compatibility.

### Source and environment safeguards

- Expand URI text, then parse and validate it. Do not grant authority from an
  unexpanded host, URI, or path. Reapply existing allowlists, redirects,
  offline behavior, immutable revision/digest/checksum rules, and cache identity
  to the final source. Do not automatically URL-encode variable values.
- Preserve simple and detailed location forms. This plan expands their URI
  text, not separately typed revisions/checksums or policy fields. Recheck
  deserialization constraints before extending a source form; a placeholder
  must not force permissive parsing of unrelated invalid fields.
- Nested configuration cycle/depth checks use the effective source identities.
  Each node resolves using its own declaring configuration and binding scope.
- Loading an explicitly interpolated template that does not exist must fail
  clearly instead of silently executing a different default template. Preserve
  current fallback behavior for existing literal declarations in this phase.
- Container declaration equality and owned logical-name generation use expanded
  values. Two declarations that collide after expansion must undergo the existing
  conflict checks. Keep lifecycle, cleanup, mount, and startup limits intact.
- Expand virtual-environment names, not Xylem-managed environment prefixes.
  Preserve per-input environment isolation and the manager's existing name rules.
- Operator sandbox policies and allowed workloads remain trusted literal input.
  Workflow interpolation cannot rewrite their grants or environment allowlists.

## Explicit exclusions

Do not interpolate arbitrary serialized JSON/YAML, object keys, module identity
names, descriptions, enum selectors, booleans/numbers, `recursive`, resource
limits, timeouts, checksums/revisions, requirements, container runtime/context,
or operator-policy fields. Do not inject text into CEL scripts: use
`variables.NAME`. Lifecycle scripts remain unsupported under current validation.
Do not add a templating language, secrets store, environment expansion, or
general configurable interpolation registry.

External-template dry runs currently may report `template_source` without
loading/expanding its executable. Preserve dry-run acquisition purity; do not
fetch or materialize unavailable content solely to populate a preview command.
Where content is unavailable, expose that existing limitation rather than
claiming that the effective command has been inspected.

## Dependencies, execution order, and parallel work

EXP-01 is ready on the reviewed worktree. The committed baseline includes the
completed root-resolution refactor.

The feature phases consume accepted named binding behavior so each
new field ships with correct CLI precedence. This sequencing avoids releasing
a field that expands config defaults while ignoring the advertised override.
Named bindings apply before preview configuration and after leaf flattening.
EXP-04 owns carrying the same immutable overlay
into each source-resolution node. Do not introduce a second override type.

Recommended order after EXP-01: EXP-02, EXP-03, EXP-04,
EXP-05, EXP-06, EXP-07, then EXP-08, EXP-09, EXP-10. This is a scheduling
recommendation; the precise prerequisites are listed per phase. EXP-11
qualifies the combined behavior.

Configuration and CLI phases serialize with K8S owners of the same
files. Source phases reserve acquisition tests, but do not require SEC-07's
Windows qualification or a dependency update. Command/environment/container
phases share `script/mod.rs` and must not run concurrently. External runner
and measurement work can continue with disjoint files. User docs and wiki
metadata are shared integration surfaces even with different primary locks.

## Project code boundaries

Create:

- `crates/xylem-lib/src/interpolation.rs`: internal bounded scalar interpolation
  and its focused unit tests, extracted from selection-root implementation.
- `crates/xylem-lib/src/config/interpolation.rs`: staged workflow/module field
  resolution, including invocation overlays and field-qualified errors.
- `crates/xylem-cli/examples/configuration-variables.json`: local executable
  example that grows only as each field's support lands.

Update:

- `crates/xylem-lib/src/lib.rs`
- `crates/xylem-lib/src/config/mod.rs`
- `crates/xylem-lib/src/config/validation.rs`
- `crates/xylem-lib/src/config/tests/mod.rs`
- `crates/xylem-lib/src/selection/cel/mod.rs`
- `crates/xylem-lib/src/selection/cel/tests.rs`
- `crates/xylem-lib/src/selection/mod.rs`
- `crates/xylem-lib/src/plan.rs`
- `crates/xylem-lib/src/script/mod.rs`
- `crates/xylem-lib/src/script/tests/mod.rs`
- `crates/xylem-lib/src/container.rs`
- `crates/xylem-lib/src/acquisition/tests.rs`
- `crates/xylem-lib/src/sandbox/mod.rs`
- `crates/xylem-lib/src/sandbox/tests/mod.rs`
- `crates/xylem-lib/src/tests/plan.rs`
- `crates/xylem-lib/src/tests/completion.rs`
- `crates/xylem-cli/src/commands/run/preparation.rs`
- `crates/xylem-cli/src/commands/run/tests/mod.rs`
- `crates/xylem-cli/src/commands/export.rs`
- `crates/xylem-cli/tests/workflow.rs`
- `crates/xylem-cli/tests/export.rs`
- `ARCHITECTURE.md`
- `MODULE_SPEC.md`
- `docs/src/configuration.md`

Delete: `crates/xylem-lib/src/selection/roots.rs` only after EXP-01 moves its
behavior and all callers/tests to the shared implementation.

Shared exact write sets:

- **User documentation:** `MODULE_SPEC.md`, `docs/src/configuration.md`.
- **Planning reconciliation:** this plan, `.wiki/features/index.md`,
  `.wiki/index.md`, `.wiki/plan-network.md`,
  `.wiki/parallel-execution.md`.

The phase fields below incorporate those named sets explicitly. Generate
fixtures beneath temporary directories from the listed test files. Do not
modify the user's validation repository, existing environments, release files,
dependency manifests, or lockfile. Additional paths require a bounded phase
revision before editing; unexpected failures do not silently expand ownership.

## Acceptance and validation

| Area | Evidence required |
|---|---|
| Binding semantics | Workflow/module/nested precedence, named then positional override precedence, recursive references, scalar conversions, unknown/cyclic/oversized values, and no environment fallback. |
| Inputs and output | File/directory/list/CLI cases; spaces and Unicode; chosen input identity equals command-visible input; effective output agrees across plan, context, artifacts, and completion; explicit CLI bypass/override behavior. |
| Sources | Local leaf and nested config fixtures; verified local HTTP/Git/OCI fixtures; changed host/scheme denial; offline/cache isolation; checksums/revisions still enforced; cycle identities use expanded sources; invalid interpolation makes no request. |
| Templates | Inline/external command parity where available; interpolated filename missing error; existing literal fallback retained; one executable and unchanged argv boundaries; no shell execution introduced. |
| Environments and containers | Input-local environment prefixes retained; expanded names/images/startup args/mounts appear in fake runtime requests; conflict and invalid-name/target tests; sandbox restrictions unchanged. |
| Compatibility | JSON, JSONC, YAML, YML; unchanged literal configs; nested relative sources and mount bases; direct library use, CLI, TUI adapter, and export validation; no schema or journal shape drift. |
| Durable results | Matching effective fields in plans and runs; relevant changes invalidate completion identity; repeat runs resume; no process/config-file mutation or cross-run contamination. |

Focused filters below are test names to introduce with each phase, not claims
that tests already exist. Run changed-package checks without automatic fixes.
Use `cargo fmt --all -- --check`, focused Clippy, and the wiki validator at
handoffs. Keep a runnable local example and field-support table in the user
documentation as each feature lands. No CLI syntax changes are expected here.

Schema baselines remain execution-plan v15 and run-summary v14: this work
changes values, not serialized shapes. If authored-versus-effective provenance
requires new fields, stop and add a bounded schema phase with Run state ownership.

# Execution phases

## EXP-01 — Share the bounded structural interpolation core

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: preserve and recheck the current selection-root implementation; no external gate.
- Exact write set: `crates/xylem-lib/src/interpolation.rs`, `crates/xylem-lib/src/lib.rs`, `crates/xylem-lib/src/selection/roots.rs`, `crates/xylem-lib/src/selection/mod.rs`, `crates/xylem-lib/src/selection/cel/mod.rs`, `crates/xylem-lib/src/selection/cel/tests.rs`, `ARCHITECTURE.md`; Planning reconciliation set.
- Primary integration lock: Interpolation
- Outcome: Selection roots use a reusable internal scalar resolver with the same strict bounded semantics, errors, and discovery ordering. The root-specific wrapper validates paths; the shared core also permits field-valid empty strings. Remove the superseded root implementation without changing legacy argument interpolation.
- Focused validation: `cargo test -p xylem-lib root_variables` and new `structural_interpolation` tests for recursion, scalar handling, limits, error field paths, and immutable bindings. Compare existing root behavior before/after extraction.
- Handoff: Preserve the core contract in rustdoc/tests and architecture docs; prune EXP-01 and mark dependent phases ready.

## EXP-02 — Resolve plain input values before input identity is built

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-01 accepted; reserve Configuration and input-policy files.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/plan.rs`, `crates/xylem-lib/src/sandbox/mod.rs`, `crates/xylem-lib/src/sandbox/tests/mod.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-cli/tests/workflow.rs`, `crates/xylem-cli/examples/configuration-variables.json`; User documentation and Planning reconciliation sets.
- Primary integration lock: Configuration
- Outcome: Configured directory/list input and selected CLI input values expand before discovery/deduplication/provenance. Plan input identity and command-visible input agree. Preserve CLI replacement, batching, existing path bases, sandbox checks, and selection bypass behavior.
- Focused validation: New `input_interpolation` library/CLI tests for literal and templated inputs, named overrides, duplicates after expansion, unknown/cyclic bindings, unchanged anchoring, and actual execution matching the dry-run input. Re-run root discovery and sandbox input tests.
- Handoff: Ship input expansion independently with an executable local example; record the input-identity regression fix, prune this phase, and release shared files.

## EXP-03 — Resolve workflow artifact output before normalization

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-01 accepted; reserve CLI preparation and configuration files after conflicting work releases them.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/tests/completion.rs`, `crates/xylem-cli/src/commands/run/preparation.rs`, `crates/xylem-cli/src/commands/run/tests/mod.rs`, `crates/xylem-cli/tests/workflow.rs`, `crates/xylem-cli/examples/configuration-variables.json`; User documentation and Planning reconciliation sets.
- Primary integration lock: CLI
- Outcome: Workflow output aliases expand before normalization and artifact checks, using workflow/invocation scope. Existing module output defaults remain intact; `--artifact-output` wins and bypasses unused configured output interpolation. Direct library callers have an explicit equivalent normalization step without a breaking API replacement.
- Focused validation: New `output_interpolation` tests for relative/absolute paths, aliases, nested workflow defaults, explicit output-variable references, self-reference failure, CLI precedence, sandbox denial, and actual output/marker location agreement.
- Handoff: Preserve the output precedence contract and migration examples; prune the phase after independently usable output behavior passes.

## EXP-04 — Resolve local module sources with invocation bindings

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-01 accepted; reserve Configuration plus CLI preparation and source-policy test files.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-lib/src/sandbox/tests/mod.rs`, `crates/xylem-cli/src/commands/run/preparation.rs`, `crates/xylem-cli/src/commands/run/tests/mod.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Configuration
- Outcome: Local executable leaf URI text expands in module scope before rebasing/materialization. Carry the immutable invocation bindings through the resolution API with existing public entry points delegating to empty bindings. No child/module default can override an invocation binding. Non-local or configuration-wrapper URI interpolation remains explicitly unsupported until EXP-05; literal sources retain their behavior.
- Focused validation: New `local_source_interpolation` tests for simple/detailed file locations, module/nested defaults, named overrides, absolute/relative sources, malformed/escaping sources, workspace isolation, and matching source identity in dry-run/runtime.
- Handoff: Document the bounded local-source support and binding-aware resolver handoff. Prune EXP-04 and make EXP-05 eligible.

## EXP-05 — Resolve remote and nested sources before security checks

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-04 accepted; local fixture server/process capabilities available for deterministic acquisition tests.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/acquisition/tests.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Configuration
- Outcome: Supported remote leaf and configuration-wrapper URI text expands before loading, policy enforcement, cache identity, or cycle detection. Parent/wrapper bindings select child sources; child-local declarations cannot bootstrap their own location. Existing acquisition implementations remain responsible for transport and verification.
- Focused validation: New `source_interpolation` tests against local HTTP/Git/OCI fixtures: denied final hosts/schemes, zero requests on unresolved variables, offline cache identity, immutable verification, nested cycles/depth, and override precedence across wrappers.
- Handoff: Record supported source forms and verified security boundaries; prune after passing. Transport defects outside these files require a separate repair phase rather than expanding this phase.

## EXP-06 — Expand executable names in loaded templates

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-01 accepted; reserve command preparation and plan files.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-lib/src/plan.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Command preparation
- Outcome: Inline and available external-template executable names expand from module/invocation bindings before validation and launch. Preserve one executable value and existing argument boundaries; reject input/run-derived placeholders in this static field. Keep external dry-run availability limitations explicit.
- Focused validation: New `command_interpolation` tests for inline/external templates, module precedence, named override, spaces in executable paths, unknown names before spawn, and dry-run/runtime agreement without shell interpretation.
- Handoff: Ship command-name support with matching docs/tests, prune the phase, and expose the loaded-template resolution operation for subsequent template/environment phases.

## EXP-07 — Expand template filenames before loading

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-06 accepted; reserve command preparation and sandbox template-validation files.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-lib/src/sandbox/mod.rs`, `crates/xylem-lib/src/sandbox/tests/mod.rs`, `crates/xylem-lib/src/plan.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Command preparation
- Outcome: String-valued template filenames expand consistently for discovery, environment detection, sandbox validation, and execution. Missing explicitly interpolated filenames fail; existing literal fallback remains unchanged. Reapply existing path/containment rules to the final filename.
- Focused validation: New `template_path_interpolation` tests for alternate templates, module overrides, missing expanded files, unchanged literal fallback, symlink/containment cases, and external-template command expansion after selection.
- Handoff: Record filename behavior and error contract, prune the phase, and make environment/container consumers eligible.

## EXP-08 — Expand virtual-environment names without changing isolation

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-07 accepted; command preparation files released by their previous owner.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Command preparation
- Outcome: Explicit virtual-environment names expand before preparation and invocation for supported managers. Detection defaults and Xylem-owned environment paths retain their behavior; templates and invocation bindings use the same static module scope.
- Focused validation: New `environment_interpolation` tests with recording/fake manager commands for Pixi and Conda/Mamba name arguments, missing variables before installation, and distinct input-local prefixes. No dependency downloads or mutation of installed user environments.
- Handoff: Document verified managers and any live-manager limitations; prune after the deterministic preparation/launch contract passes.

## EXP-09 — Expand container identity and startup settings

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-07 accepted; reserve command preparation, container declaration, and plan files.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/validation.rs`, `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-lib/src/container.rs`, `crates/xylem-lib/src/plan.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Command preparation
- Outcome: Container image/logical name/startup-command elements expand before declaration comparison, validation, owned naming, and creation. Preserve runtime/context literals, external-container ownership, startup strategy, and cleanup behavior. Operator sandbox workload identities remain unchanged.
- Focused validation: New `container_interpolation` tests using fake runtimes for exact image/name/argv, collisions after expansion, conflicting declarations, keep-alive compatibility, no runtime call on invalid binding, and consistent planned/runtime values.
- Handoff: Preserve the container scope and ownership contract in tests/docs; prune and enable EXP-10.

## EXP-10 — Expand container mount paths before rebasing and validation

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-09 accepted; reserve the same command/container files serially.
- Exact write set: `crates/xylem-lib/src/config/interpolation.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/validation.rs`, `crates/xylem-lib/src/script/mod.rs`, `crates/xylem-lib/src/script/tests/mod.rs`, `crates/xylem-lib/src/container.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-cli/tests/workflow.rs`; User documentation and Planning reconciliation sets.
- Primary integration lock: Command preparation
- Outcome: Source/target mount fields expand before their existing declaring-file rebasing and validation. Inline and external-template mount bases remain correct; retain target syntax, read-only settings, ownership, and runtime argument boundaries.
- Focused validation: New `mount_interpolation` tests for relative/absolute sources, external/nested declarations, invalid/nonexistent sources, target rejection, spaces, conflict detection, and exact fake-runtime mounts. Re-run existing mount and container lifecycle tests.
- Handoff: Document mount resolution order, prune the phase, and release the command/container lock.

## EXP-11 — Qualify the complete field and invocation matrix

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: EXP-02, EXP-03, EXP-05, EXP-08, and EXP-10 accepted; serialize final checks after all implementation work stabilizes.
- Exact write set: `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-lib/src/tests/completion.rs`, `crates/xylem-cli/src/commands/export.rs`, `crates/xylem-cli/tests/workflow.rs`, `crates/xylem-cli/tests/export.rs`, `crates/xylem-cli/examples/configuration-variables.json`, `ARCHITECTURE.md`; User documentation and Planning reconciliation sets.
- Primary integration lock: CLI
- Outcome: One executable configuration demonstrates combined inputs/output/source/template behavior with named and positional invocation bindings. Export check/generation validates effective configuration through the same shared resolution contract while keeping its existing command interface. No product feature is deferred from a prior phase to make that phase coherent.
- Focused validation: Run the library and CLI suites, all four configuration formats, applicable TUI feature checks, example execution, export tests, format check, focused Clippy, and wiki validation. Compare effective plan/runtime values and resume behavior across two different invocations. Record live-provider checks separately from fake-runtime coverage.
- Handoff: Repair newly exposed defects in separately bounded phases. Once acceptance is complete, preserve the field matrix and migration notes in user docs, then retire this plan and remove all index/network/lane references.
