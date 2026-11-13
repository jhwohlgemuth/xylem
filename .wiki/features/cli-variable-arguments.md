---
type: Plan
title: CLI variable overrides and positional workflow arguments
description: Bind invocation values to workflow variables with repeatable named overrides first and config-declared positional arguments afterward.
difficulty: medium
impact: high
package: [xylem-cli, xylem-lib]
tags: [cli, configuration, variables]
status: planned
priority: next
execution: phased
phase_budget_hours: 5
---

# CLI variable overrides and positional workflow arguments

## Outcome

Run one workflow with different variable values without editing its configuration.
Ship repeatable named overrides first, then add ordered positional bindings
declared by the workflow. The user confirmed this delivery order on 2026-11-13.
Each invocation must use the same effective values for input selection, planning,
command preparation, execution, and completion/resume decisions.

## Current evidence

Initially inspected on 2026-11-13 at `1c4a75e` (Xylem 1.0.7). Binding timing
was rechecked against the subsequent uncommitted basename/root-expansion work.
CodeGraph and targeted source/document inspection establish these boundaries;
implementation tests have not been run for this proposed feature.

- `crates/xylem-cli/src/cli.rs` exposes `run [OPTIONS] [CONFIG]`, `--input`,
  `--artifact-output`, and global dotenv `--env`, but no variable override or
  trailing workflow-argument interface.
- `crates/xylem-cli/src/commands/run/preparation.rs` reads configuration,
  resolves nested configurations, validates, selects inputs, builds a plan,
  and creates the execution context. It is the shared dry-run/runtime binding
  boundary. Override syntax can be validated before acquisition and workspace
  creation; effective values must be applied before input selection and planning.
- `Config::resolve_at` merges variables through nested configuration modules.
  Both `Config::module_environment` and `Config::resolve_context_variables`
  give module variables precedence over workflow variables. Merely adding CLI
  values to the root map would leave module and nested defaults in control.
- `Config::resolve_inputs` supplies workflow variables to CEL input selection.
  CEL receives configured values rather than the command interpolation result;
  this feature must preserve that distinction.
- Selection roots now expand workflow-variable placeholders before discovery
  and sandbox authorization. CLI overrides must reach those bindings as well
  as the CEL script. `INPUT_BASENAME` replaces the removed `PARENT_DIRECTORY`
  built-in; only the new name is reserved by the invocation interface.
- Variable expansion already supports recursive `{{ NAME }}` references and
  `_XYLEM_` command-environment names. Plans and completion fingerprints consume
  effective module variables. Reuse these paths rather than adding another
  substitution engine or mutating the process environment.
- `main.rs` has both direct-run and TUI run callers; `lib.rs` has an example
  adapter. These callers must remain valid when run request plumbing changes.
- The workflow contract is documented in `MODULE_SPEC.md` and
  `docs/src/configuration.md`; CLI help is checked in under
  `docs/src/generated/`. JSON, JSONC, YAML, and YML must stay consistent.

## Recommended interface and decisions

The syntax below is the proposed implementation contract, not an existing CLI.

### Named overrides first

```sh
xylem run workflow.json --var DATASET=basin_range --var MODE=quick
xylem run workflow.json --dry-run --var 'LABEL=Trial one'
```

- Add repeatable, run-scoped `--var NAME=VALUE`, with exactly one value per
  occurrence and no short alias. `-v` retains its verbosity meaning.
- Split on the first `=`. Values are strings: preserve empty values, spaces,
  commas, and additional `=` characters after normal shell argument handling.
  Do not infer JSON, numbers, or booleans. CEL users explicitly convert string
  values when they need numeric or boolean expressions.
- Require names matching `[A-Z][A-Z0-9_]*`. Reject missing `=`, empty or invalid
  names, and engine-owned `INPUT`, `INPUT_BASENAME`, `MODULE_LIST`, `RUN_ID`, and
  `WORK_DIRECTORY`. Also reject `OUTPUT_DIRECTORY` at this new invocation
  interface and direct users to `--artifact-output`; existing configuration
  support for that variable remains unchanged.
- Repeating a name uses the last `--var` value. Undeclared names are allowed.
  There is no implicit lookup from the host environment for a bare name.
- Apply validated overrides to the root workflow map before structural
  preparation, and reapply them to every resolved leaf map after nested
  inheritance. Retain the original immutable override map separately for
  resolution stages that need invocation precedence. Preserve other entries and types.
  Run-wide overrides take precedence over module defaults, including values in
  nested configuration modules and runs selected by `--module-index`.
- Feed the resulting configuration into existing validation, input selection,
  planning, and execution. Recursive references follow existing behavior;
  unresolved references and cycles retain their existing semantics. Do not
  extend interpolation to previously unsupported fields in this plan; the
  [configuration expansion plan](configuration-variable-expansion.md) owns
  those additions and their field-specific error semantics.
- Invalid invocation bindings return the CLI usage exit code before acquisition,
  workspace creation, or process launch. Do not echo the full supplied value in
  a diagnostic. Ordinary variable visibility in plans/reports is unchanged;
  this interface does not introduce secret storage.

### Config-declared positional arguments afterward

Add an optional workflow-level `arguments` list, shown here as a configuration
fragment:

```json
{
  "arguments": ["DATASET", "MODE"],
  "variables": {
    "DATASET": "default_dataset",
    "MODE": "full"
  }
}
```

```sh
xylem run workflow.json -- basin_range quick
xylem run workflow.json --var MODE=full -- basin_range quick
```

The second command binds `DATASET=basin_range` and `MODE=full`.

- List order maps values after `--` to names. Require an explicit config path
  when positional values are supplied, preserving unambiguous optional config
  discovery for ordinary named-only runs. All Xylem options precede `--`.
- Reject excess values, duplicate declarations, invalid names, and reserved
  names using the same binding rules. Supplying values with no declarations is
  a usage error. Invalid declarations are configuration errors.
- Omitted trailing positions make no binding, so existing workflow and module
  defaults apply. No new required-argument, default-value, or type annotation
  system is introduced. An empty string is an explicit value, not omission.
- Bind positions by index before merging named overrides; a named override
  always wins over the same positional name. It does not shift or consume a
  positional slot. Hyphen-leading values after `--` are literal values.
- Only the invocation's root configuration declares its positional interface.
  Nested configurations do not append or replace that list; their declarations
  can be used when invoked directly. Declarations do not change runtime
  behavior without supplied bindings, including for export and TUI callers.
- Use the same validated override representation and application operation as
  named bindings. No passthrough of arbitrary arguments to child programs.

Effective precedence for user-controlled variables is existing inherited
workflow/module defaults, then positional bindings, then named overrides.
Engine-owned context and artifact-output handling remain authoritative.

## Scope and compatibility

Keep `--input` as the existing batch-input selector, and `--env` as dotenv
loading. Do not add module-specific override selectors, argument files, dynamic
flags generated from config, new environment inheritance, export-time binding,
or a general parameter schema. Existing invocations and configurations without
`arguments` must behave as before.

Preserve the existing public run adapter signature with a wrapper supplying
empty bindings; add a binding-aware entry point for CLI invocation as needed.
Avoid turning invocation plumbing into an unrelated public API redesign.

Store effective values through existing configuration/plan fields. No new
execution-plan or run-summary schema fields, schema version, journal format,
dependency, or platform provider is expected. If implementation proves one is
necessary, stop and split a bounded design/repair phase before changing it.

## Project code boundaries

Update:

- `crates/xylem-lib/src/config/mod.rs`
- `crates/xylem-lib/src/config/validation.rs`
- `crates/xylem-lib/src/config/tests/mod.rs`
- `crates/xylem-lib/src/tests/plan.rs`
- `crates/xylem-lib/src/tests/completion.rs`
- `crates/xylem-cli/src/cli.rs`
- `crates/xylem-cli/src/cli/tests.rs`
- `crates/xylem-cli/src/main.rs`
- `crates/xylem-cli/src/lib.rs`
- `crates/xylem-cli/src/commands/run/mod.rs`
- `crates/xylem-cli/src/commands/run/preparation.rs`
- `crates/xylem-cli/src/commands/run/tests/mod.rs`
- `crates/xylem-cli/tests/workflow.rs`
- `crates/xylem-cli/tests/export.rs`
- `crates/xylem-cli/tests/help.rs`
- `MODULE_SPEC.md`
- `docs/src/configuration.md`
- `docs/src/generated/xylem-run-help.txt`

Create:

- `crates/xylem-lib/src/config/overrides.rs`: validated, deterministic bindings
  and a side-effect-free operation returning an effective configuration.
- `crates/xylem-lib/src/config/arguments.rs`: ordered declaration validation
  and positional-to-variable binding after the named interface lands.
- `crates/xylem-cli/examples/variable-arguments.json`: runnable named override
  example, extended with positional declarations in the final phase.

Delete: None.

Every phase also owns this plan, `.wiki/features/index.md`, `.wiki/index.md`,
`.wiki/plan-network.md`, and `.wiki/parallel-execution.md` solely for lifecycle
reconciliation. These five paths are included in every phase write set below
by this explicit shared ownership rule. Serialize those edits with other plan
owners. Test fixtures should be generated in temporary directories by the
listed test files; do not change the user's validation repository.

## Dependencies and parallel work

No external service, credential, runner, or other active plan blocks ARG-01.
Execute ARG-01 → ARG-02 → ARG-03 → ARG-04. The library operations in ARG-01
and ARG-03 are independently usable and documented through rustdoc; their
merges do not promise a CLI interface before its owning phase lands.

[Configuration expansion](configuration-variable-expansion.md) can extract
its bounded interpolation core in EXP-01 alongside ARG-01 with disjoint code
ownership and serialized wiki edits. Its field phases depend on ARG-02.
ARG-02 applies root overrides before output normalization/source preparation
and final overrides after flattening; EXP-04 subsequently carries the same
immutable bindings into each source-resolution boundary. Do not introduce a
second override representation. ARG-03/04 can proceed alongside expansion
only when exact code, test, documentation, and planning write sets are disjoint.

Configuration changes overlap K8S-04. CLI plumbing overlaps
K8S-03/09/10/11/14. Documentation changes overlap BRW-03 and K8S-19. These are
shared-file coordination constraints, not implementation dependencies. External
runner qualification and A/P/O measurement may proceed alongside this plan
when their files are disjoint. Do not run automatic workspace fixes during
concurrent implementation.

## Acceptance and validation

| Behavior | Required evidence |
|---|---|
| Parsing and precedence | Repeated names, empty values, whitespace, commas, extra `=`, invalid names, and reserved names; no host-env fallback. |
| Effective variables | Root/module/nested defaults, undeclared names, recursive references, and no-override equivalence through both plan and runtime resolvers. |
| Actual execution | Temporary generic Python or shell workflow records the overridden value in argv and `_XYLEM_` environment; inline and external templates are covered. |
| Planning and selection | Dry-run command and effective variables agree with execution; expanded selection roots and CEL input selection see CLI strings; `--input` and `--module-index` keep their meanings. |
| Completion and isolation | Changing a relevant override invalidates the prior completion fingerprint; unchanged values retain resume behavior; bindings do not leak between two runs or mutate config files/process environment. |
| Sandbox and containers | Shared preparation/provider-request tests prove the same effective bindings reach sandbox and container command environments, including `--no-container-env`; do not rely solely on a host execution test. |
| Positional interface | Ordered mapping, omitted defaults, named precedence, empty and hyphen-leading values, explicit-config requirement, unknown/excess values, and nested declaration isolation. |
| Compatibility | JSON, JSONC, YAML, YML, library adapter, TUI compilation, export defaults, generated help, and existing no-binding workflows remain valid. |

Use focused tests per phase, then run the xylem-lib and xylem-cli suites and
the repository's applicable feature build checks at final integration. Use
`cargo fmt --all -- --check` and focused Clippy without `--fix`; regenerate
run help with `make help-cli` when flags change and inspect all generated
output. Only the run-help snapshot is expected to change. Report unavailable
live sandbox/container providers separately; mocked request coverage is not
live provider qualification. Preserve current lint policy and report existing
warnings rather than adding broad suppressions.

# Execution phases

## ARG-01 — Add the library variable override operation

- Status: ready
- Budget: 5 active hours maximum
- Prerequisites: none; recheck baseline and reserve Configuration ownership.
- Exact write set: `crates/xylem-lib/src/config/overrides.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `crates/xylem-lib/src/tests/plan.rs`, `crates/xylem-lib/src/tests/completion.rs`; shared planning paths listed above.
- Primary integration lock: Configuration
- Outcome: A documented validated string-binding type and consuming/copying operation apply run-wide overrides to workflow and leaf maps without environment mutation. Callers validate bindings before resolution, apply them to the root preview before structural preparation, and reapply them after nested resolution. Keep the original bindings available for later resolution stages. Existing resolvers remain the sole interpolation paths. Empty overrides preserve existing behavior.
- Focused validation: Add `variable_override` tests and run `cargo test -p xylem-lib variable_override`; cover reserved/name validation, duplicate policy, nested/module precedence, recursive references, plan/runtime parity, CEL strings, completion hashes, and isolation. Run focused Clippy and rustdoc for the new public API.
- Handoff: Record API and precedence evidence in rustdoc/tests, prune this phase after acceptance, and make ARG-02 ready. No CLI flag is advertised yet.

## ARG-02 — Ship repeatable named CLI variable overrides

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: ARG-01 accepted; reserve CLI and the listed documentation/test files.
- Exact write set: `crates/xylem-cli/src/cli.rs`, `crates/xylem-cli/src/cli/tests.rs`, `crates/xylem-cli/src/main.rs`, `crates/xylem-cli/src/lib.rs`, `crates/xylem-cli/src/commands/run/mod.rs`, `crates/xylem-cli/src/commands/run/preparation.rs`, `crates/xylem-cli/src/commands/run/tests/mod.rs`, `crates/xylem-cli/tests/workflow.rs`, `crates/xylem-cli/tests/help.rs`, `crates/xylem-cli/examples/variable-arguments.json`, `MODULE_SPEC.md`, `docs/src/configuration.md`, `docs/src/generated/xylem-run-help.txt`; shared planning paths listed above.
- Primary integration lock: CLI
- Outcome: `run --var NAME=VALUE` is usable end to end. Parse once, reject invalid bindings before side effects, and pass owned invocation bindings through the run request. Apply them to the root preview before output normalization/source preparation and reapply them to the resolved config before selection/planning; retain the immutable original for the expansion plan's later per-source integration. Keep TUI/example callers supplying empty bindings. Document string semantics, precedence, artifact-output handling, and quoted examples.
- Focused validation: Add `cli_variable` parser/preparation/workflow tests; run `cargo test -p xylem-cli cli_variable` and `cargo test -p xylem-cli --test help`. Prove an actual host run, matching dry run, temporary nested config, CEL selection, and sandbox/container request parity. Check repeat invocation isolation and invalid-binding early failure. Regenerate help, run the example, and compile the existing TUI feature path.
- Handoff: Named overrides are independently releasable with tested user docs. Record provider coverage and any environment limitations, prune this phase, and make ARG-03 ready.

## ARG-03 — Add ordered configuration argument declarations and binding

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: ARG-02 accepted; reserve Configuration and the listed documentation paths.
- Exact write set: `crates/xylem-lib/src/config/arguments.rs`, `crates/xylem-lib/src/config/mod.rs`, `crates/xylem-lib/src/config/validation.rs`, `crates/xylem-lib/src/config/tests/mod.rs`, `MODULE_SPEC.md`, `docs/src/configuration.md`; shared planning paths listed above.
- Primary integration lock: Configuration
- Outcome: Optional ordered `arguments` declarations deserialize and validate consistently, and a documented pure library binder converts positional strings to the ARG-01 representation, then merges named overrides. Preserve only root invocation declarations during flattening. Document the library/config contract and explicitly state that CLI positional invocation arrives in the following phase.
- Focused validation: Add `workflow_arguments` tests and run `cargo test -p xylem-lib workflow_arguments`; cover all four formats, round trips, missing/empty declarations, duplicate/invalid/reserved names, too many values, omitted defaults, named precedence, and nested-root ownership. Run configuration tests and rustdoc for the binding API.
- Handoff: A mergeable configuration/library capability with no changes to existing CLI invocation behavior; prune this phase and make ARG-04 ready.

## ARG-04 — Ship positional CLI binding and qualify the complete interface

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: ARG-03 accepted; reserve CLI and the listed docs/test files. Live provider checks require an available provider but do not block deterministic local coverage.
- Exact write set: `crates/xylem-cli/src/cli.rs`, `crates/xylem-cli/src/cli/tests.rs`, `crates/xylem-cli/src/main.rs`, `crates/xylem-cli/src/lib.rs`, `crates/xylem-cli/src/commands/run/mod.rs`, `crates/xylem-cli/src/commands/run/preparation.rs`, `crates/xylem-cli/src/commands/run/tests/mod.rs`, `crates/xylem-cli/tests/workflow.rs`, `crates/xylem-cli/tests/export.rs`, `crates/xylem-cli/tests/help.rs`, `crates/xylem-cli/examples/variable-arguments.json`, `MODULE_SPEC.md`, `docs/src/configuration.md`, `docs/src/generated/xylem-run-help.txt`; shared planning paths listed above.
- Primary integration lock: CLI
- Outcome: `run CONFIG [OPTIONS] -- VALUES...` binds via root declarations before preparation, requires an explicit config path, and reuses the named-override path. Omitted values preserve defaults; named values win. Examples and help describe the finished interface; export and TUI continue to use defaults.
- Focused validation: Add `cli_arguments` parser/preparation/workflow cases and run `cargo test -p xylem-cli cli_arguments`; cover literal hyphens, empty strings, omitted/extra values, named-slot precedence, default-config ambiguity, and usage/data error distinction. Run the executable example, help/export tests, then xylem-lib and xylem-cli suites, applicable feature checks, format check, and focused Clippy. Any newly exposed defect outside this write set gets a bounded repair phase.
- Handoff: Record actual checks and provider limitations in the implementation review; preserve decisions in docs/tests, then retire this plan and remove its index/network/lane entries when all acceptance criteria are satisfied.
