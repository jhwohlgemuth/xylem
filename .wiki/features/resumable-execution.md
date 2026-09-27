---
type: Plan
title: "Land typed execution foundations and add resumable execution"
description: "Execute the YAML, typed-error, and configuration-validation prerequisites in order, then declare expected module outputs and skip completed work during explicit resume runs."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [feature, resume, output, completion, fingerprint, idempotency, foundation]
status: planned
priority: next
---

# Goal

Give Xylem a validated, side-effect-free execution plan and use that plan to skip work that is already complete.

The end state is a workflow configuration that is rejected before any host action when it is wrong, a dry run that mutates nothing, and a resume run that recognizes a module whose declared outputs are already complete and never spawns it.

# How to work this plan

This is the single work item. It absorbs three prerequisite plans, so nothing here should be scheduled or started from anywhere else.

- Work the phases in order. Each one ends at a green `cargo nextest run` and is landed before the next begins.
- [Maintained YAML parser](../migrations/maintained-yaml-parser.md), [typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md), and [configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md) are executed as Phases 1 through 3. Their own documents are the detailed reference for their phase, not separate work items, and their `status` flips when their phase lands.
- Phases 8 and 9 may be prepared in parallel with each other once Phase 7's public behavior is stable, but not before it.
- The manifest lock, the configuration lock, the process lock, the run-state lock, and the CLI lock are all held for the duration. No other lane may claim them while this plan is in progress.

# Baseline

Landed at `005aa73` and available to build on:

- The durable execution journal, `RunSummary`, the console and JSON reporters, `--reporter`, `--output`, and `docs/schema/run-summary-v1.schema.json`.
- `ExecutionLimits`, `ExecutionContext`, the rolling `options.batch` input ceiling, and the bounded input dispatch in `xylem-cli/src/commands/run/mod.rs`.
- Partial typed error work: `ConfigReadError`, `JournalError`, `RunFailure`, `ModuleOutcome`, and `WorkflowOutcome`.
- The ACORN 0.2 contract crates, `acorn-core`, `acorn-lib`, and `acorn-schema` at 0.2.0.
- A green suite: `cargo test --workspace --all-targets` passes 68 tests at baseline, with no failures.

Not landed, and relevant to this plan:

- The ACORN dependency-narrowing work has not finished. `acorn-lib` still enables the `analysis` feature and the lockfile still resolves the analyzer stack, including Polars. That work stays in the dependency lane and is not part of this plan, but it shares the manifest lock.
- `xylem-cli` declares `glob` and never uses it. Phase 5 drops it while holding the manifest lock.
- Ten of the fourteen compiled-CLI tests in `xylem-cli/tests/workflow.rs` are gated behind `#[cfg(not(target_os = "windows"))]`, so a Windows host or runner executes only four. The gated set holds the durable-journal, batching, ordering, and module-ordering assertions, which is why the hermetic test plan is kept open alongside this one.

# Current evidence

- Phase 1 replaced `serde_yml`/`libyml` with `serde_norway` and removed the RUSTSEC-2025-0067/RUSTSEC-2025-0068 exceptions.
- `xylem-lib` still reaches `std::process::exit` from `config/mod.rs:258` and `script/mod.rs:755`, and `unimplemented!` for a serialized shell input, configuration modules, unknown extensions, and unknown schemes.
- `Script::run` returns `Vec<String>`, so a spawn failure, a non-zero exit, and a successful silent child are indistinguishable.
- `Config::read_json` and `Config::read_yaml` preserve path, format, and source-chain context through the owned configuration error.
- Deserialization rejects unknown fields, but still accepts shell input, configuration modules, seven module languages, `check`, `before_script`, `after_script`, and `output` without validating whether execution implements them.
- A dry run still creates a temporary root, and local binary preparation can call `make_executable`, so it is not side-effect free.
- `ExecutionState` has one terminal skip value, so a dry-run skip and a completion skip are indistinguishable in the journal and in the console report.
- `Config.output` documents an artifact path but is not used by execution, and modules receive `OUTPUT_DIRECTORY` as an ordinary user variable that Xylem knows nothing about.
- Xylem's scheduling unit is a top-level input path. It cannot promise pair-level resume when a child module internally loops over multiple image pairs.

# Project code boundaries

- **Update:** `Cargo.toml`, `xylem-lib/Cargo.toml`, `xylem-cli/Cargo.toml`, `Cargo.lock`, `deny.toml`, `Makefile`, `xylem-lib/src/{lib,config,script,execution,journal,progress,outcome}.rs`, `xylem-cli/src/{main,cli,commands/run}.rs`, `docs/schema/`, `MODULE_SPEC.md`, `docs/src/`, `examples/atmospheric_correction-pansharpening-orthorectification/`, and the planning wiki.
- **Create:** `xylem-lib/src/error.rs`, the parse/validate/plan modules extracted from `xylem-lib/src/config/`, `xylem-lib/src/completion/`, and the YAML parity and resume fixtures under `tests/`.
- **Delete:** the `serde_yml` and `libyml` dependency pair, their advisory exceptions, library `process::exit` and reachable `unimplemented!` sites, the synthetic empty-content read fallbacks, `docs/schema/run-summary-v1.schema.json` once the run-summary contract moves to version 2, and the unused `glob` declaration in `xylem-cli`.

# Dependencies and ordering

Two items outside this plan touch the same locks:

- The dependency lane's remaining ACORN work, which is feature narrowing. It shares the manifest lock with Phase 1 and is serialized against it, but blocks nothing here.
- Host-independent fixture and helper work from the [hermetic workflow test suite](../testing/hermetic-workflow-tests.md), which may proceed alongside this plan in files this plan does not touch.

Phases 1 through 7 are sequential. Phases 8 and 9 are independent of each other and both wait for Phase 7's public behavior.

# Decisions

- The three prerequisite plans are executed here rather than tracked separately, so the whole chain has one owner and one landing order.
- A completion declaration is registered into the Phase 3 validation stage rather than checked ad hoc, and completion is evaluated while the plan is rendered, before any module is admitted to the scheduler.
- The journal and run summary move to version 2 with a new `skipped_existing` terminal state. Xylem does not read version 1 records, so no compatibility path is carried.
- Completion identity is stored as canonical text in a sidecar marker file next to the module's outputs. Xylem compares those strings, so no hashing dependency is added and the recorded value stays auditable.
- Output patterns support globs. `glob` moves into `xylem-lib`; it is already present in `Cargo.lock` and adds no transitive dependency.
- Resume is explicit. `options.resume` and `--resume` are the only ways to enable skipping.

# Open assumptions

Three points are not settled here. Each needs a decision before the phase named in its bullet.

- **Allowed output roots, needed before Phase 5.** The goal asks Xylem to reject output paths outside allowed roots but never defines a root. The proposal is that roots are `Config.output` together with every module's `OUTPUT_DIRECTORY` variable, that an expanded output outside all of them is a validation error, and that a workflow declaring completion without any root anywhere is an error. This forces an output root to be declared before completion can be.
- **Marker location, needed before Phase 6.** The marker path is derived from the literal directory prefix of the first declared output rather than declared, so a declaration whose first pattern has no literal directory prefix writes the marker into the working directory. An explicit `marker` key is the alternative.
- **Trust-existing policy, needed before Phase 8.** `--resume-trust-existing` ignores a stale fingerprint. It is the first item to drop without breaking the acceptance criteria.

# Recommendation

The recommendation is the ordered phase list below, which is also the landing order. Each phase ends at a green suite and is landed before the next begins.

## Phase 1: Replace the unmaintained YAML parser

Absorbs [maintained YAML parser](../migrations/maintained-yaml-parser.md). Locks the manifest and the configuration.

Evaluate `serde_norway` first, because it is already in the lockfile through ACORN, and accept it only after fixture parity and advisory checks pass. Replace `serde_yml` and `libyml` in `xylem-lib`, and stop exposing a third-party parser error in a public signature: `Config::read_yaml` returns the owned `ConfigReadError` with path, format, and source-chain context.

Define the supported YAML behavior and test it: aliases, merge keys, quoted scalars, nulls, booleans, numeric variables, duplicate keys, and multi-document input, rejecting ambiguous or unsupported constructs explicitly. Apply one missing-file, empty-file, and unknown-field policy to both formats, which means removing the synthetic empty-content fallbacks from `read_json` and `read_yaml`. Delete the `RUSTSEC-2025-0067` and `RUSTSEC-2025-0068` entries from `deny.toml` and the two matching `cargo audit` flags from the `Makefile` rather than renaming them.

Verify: every checked-in YAML fixture parses to the same `Config` the old parser produced, with differences reviewed; paired JSON and YAML tests deserialize to equal values; malformed, duplicate-key, multi-document, alias, and resource-limit cases are covered; `cargo audit` and `cargo deny check` pass without the two exceptions.

## Phase 2: Introduce typed errors and workflow outcomes

Absorbs [typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md). Locks the configuration, the process, and the CLI.

Create `xylem-lib/src/error.rs` with capability-owned classifications rather than one stringly type: configuration, planning and acquisition, process, and workflow. Each keeps its source chain and the relevant path or module name, carries no terminal color policy, and never calls `tracing` in place of returning a failure. Consolidate the `ConfigReadError` and `JournalError` already present into that taxonomy instead of leaving them as parallel roots.

Convert every remaining library exit and panic. `config/mod.rs` stops exiting on an unreadable configuration, and the shell input, configuration module, unknown extension, and unknown scheme branches become classified rejections rather than `unimplemented!`. `Script::run` stops exiting with `exitcode::SOFTWARE` and stops collapsing failure into an empty `Vec<String>`: wrap its current result in a typed process error carrying spawn, non-zero exit, and capture failures, and leave output capture design, timeouts, and cancellation to the process-execution plan. Preparation and workflow iteration collect and propagate results, and the CLI owns the mapping from classification to exit code.

Deny `process::exit` outside the binary and add the Clippy lints for production `unwrap`, `expect`, and panic paths, with an allow list for tests.

Verify: every classification has a unit test covering display text, source chain, and context fields; compiled-binary tests cover invalid configuration, missing requirements, invalid module location, spawn failure, non-zero module exit, and partial multi-input failure; exit status and diagnostic text are asserted without ANSI color or absolute host paths; `cargo clippy --workspace --all-targets` is clean under the new lints.

## Phase 3: Validate configuration and make dry-run side-effect free

Absorbs [configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md). Locks the configuration, the process, and the CLI.

Split the current `Config` into four stages. Parse the named file and deserialize its format with path and source context. Validate schema-independent invariants and classify every declared capability as supported or rejected. Plan path resolution, input order, variable precedence, acquisition actions, and command arguments without touching the host. Execute the accepted plan and return structured outcomes.

Preserve Phase 1's unknown-field policy. Validate the configuration version, unique and non-empty module names, URI and location kinds, required checksums, manager and runtime support, template command and arguments, variable types and reserved names, and input and output path relationships. A declared but unimplemented capability becomes a named rejection, never a silent no-op, which is what finally retires the `unimplemented!` branches Phase 2 removed the panics from. Define CLI-over-configuration precedence for every option and make `--input` an explicit override rather than a fallback hidden in control flow. Sort path-derived inputs and make duplicate handling explicit.

Dry run stops after planning and serializes the plan. It creates no temporary directory, performs no `chmod`, clones nothing, and spawns no module. Where it must read the host, the output labels the read as an observation.

Verify: table-driven JSON and YAML parity tests cover every field, alias, default, unknown field, invalid combination, and rejected capability; multi-error diagnostics are snapshotted with configuration paths such as `modules[2].template.command`; a dry run under filesystem, network, and process spies records zero mutations; input ordering and CLI precedence are tested; every configuration checked into `examples/` and `tests/` still validates.

## Phase 4: `skipped_existing` run-state vocabulary

Locks the run state.

Add `ExecutionState::SkippedExisting` beside the existing dry-run `Skipped`, and add an `is_complete` helper covering succeeded, skipped, and skipped-existing. The helper replaces the three repeated `is_succeeded() || is_skipped()` guards in `transition_input`, `finish`, and summary recovery, so an input whose modules were all skipped-existing still finalizes a successful run. `validate_transition` needs the new value admitted from the same starting states as the existing skip, because `strum`'s derived `is_skipped` does not cover it.

`ProgressState` gains a matching variant, `WorkflowOutcome::is_success` accepts it, and the CLI progress bar counts it toward the module total without printing a failure. The console reporter prints a separate skipped-existing count so a resume run is distinguishable from a dry run.

Set `JOURNAL_SCHEMA_VERSION` to 2, replace `docs/schema/run-summary-v1.schema.json` with `run-summary-v2.schema.json`, and add the new value to its `state` enum.

Verify: journal round-trip reaches `skipped_existing` from `NotStarted`; an input cannot succeed while a module is unfinished or failed; the progress mapping table and the console counts are covered.

## Phase 5: Completion declaration and validation

Locks the manifest and the configuration.

Add a `Completion` declaration holding a non-empty list of output path or glob templates to the configuration types extracted by Phase 3, and a `completion` field to `Module`. Register the declaration's checks in the Phase 3 validation stage so it is rejected before any module is prepared:

```yaml
completion:
  outputs:
    - "{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/*_MODO.tif"
```

The checks reject an empty output set, an unresolved reserved variable, an invalid glob pattern, and an output outside every allowed root. Expansion goes through the existing variable interpolation contract and returns outputs in a stable order. The declaration carries no `require` key, because the predicate has exactly one meaning: every declared pattern must match at least one entry, and every matched entry must be a non-empty file. A pattern matching nothing is not complete.

The validation itself is configuration work and stays with the declaration, but the predicate that reads the filesystem belongs in `xylem-lib/src/completion/` alongside the rest of the runtime behavior. Xylem never infers completion from whatever happens to sit in an output directory. Declare `glob` in `xylem-lib/Cargo.toml` and drop the unused declaration from `xylem-cli`.

Verify: every validation error arm has a test reported through the Phase 3 diagnostic path; JSON and YAML round-trip the declaration; the canonical fingerprint is stable across equivalent configurations and changes when the module, template, variables, or declaration changes.

## Phase 6: Completion marker persistence

Create the marker half of `xylem-lib/src/completion/`, deriving the marker path from the first declared output's literal directory prefix and the module's index and sanitized name, so the same declaration always resolves to the same file. Read the fingerprint if the marker exists, and write it after a module that declares completion finishes successfully, creating parent directories as needed. Write through a temporary file and a rename, matching the atomic reporter output already used by `RunSummary::write_json_atomic`.

Verify: write and read round-trip; a missing marker reads as absent rather than failing; a marker under a prefix directory that does not exist yet is created; the derived path is deterministic for a repeated declaration.

## Phase 7: Resume decisions in execution

Add `resume` to `ConfigOptions`. A pure planner maps one module plus the resolved variables and options to either run it or skip it because it is already complete. A module without a completion declaration always runs, and the reason for that is logged.

Evaluate that decision while the Phase 3 plan stage renders, so no skipped module is ever admitted to the scheduler. A skip records only the `skipped_existing` terminal transition, emits the matching progress state, and returns an outcome without building or spawning a script, which is what makes "skipped modules are never spawned" provable. After a module that executes successfully and declares completion, write its marker.

An input ends in `skipped_existing` only when every one of its modules was skipped-existing; otherwise the input keeps its existing terminal state. That is what lets an early complete module be skipped while a later incomplete module still runs. A dry run evaluates the same decision and reports it as a labeled observation, but keeps recording `Skipped` so it never claims real completion, and it writes no markers.

Verify: a fixture module run twice re-runs without `resume` and is not spawned with it; partial, zero-length, and missing outputs re-run; a changed variable invalidates the fingerprint; an early complete module with a later incomplete module skips only the first.

## Phase 8: CLI surface

Locks the CLI boundary.

Add `--resume` and `--resume-trust-existing` to the `run` command using the same boolean style as `--dry-run`, thread them through `main.rs`, and combine them into `ConfigOptions` with the same precedence as the existing flags. Add a compiled-CLI test with a new fixture module that proves the second resume run does not spawn the module and that the journal records `skipped_existing`.

Verify: `cargo nextest run -p xylem-cli`, with the existing integration tests unchanged because the schema-version assertion tracks the constant.

## Phase 9: Documentation, specification, and examples

Locks the documentation.

Document the completion declaration, both options, the fingerprint policy, and the input-unit resume granularity in `MODULE_SPEC.md`, `docs/src/configuration.md`, `docs/src/cli-reference.md`, and `docs/src/architecture.md`. State the parser contract, the error taxonomy, and the CLI precedence rules that Phases 1 through 3 introduced, since those are user-visible and currently undocumented. Declare completion for the three modules in the `atmospheric_correction-pansharpening-orthorectification` example, and state there that the resume unit is the input directory rather than an image pair. Flip the `status` of the three absorbed plans as their phases land, then update this plan, the feature index, the plan network, and the parallel execution order.

# Validation

- Test complete, missing, zero-length, partial, stale-fingerprint, changed-variable, unresolved-template, invalid-glob, and outside-root cases.
- Test multiple modules where an early module is complete but a later module is not.
- Test dry-run, explicit resume disabled, duplicated inputs, non-Unicode paths, and deterministic skip ordering.
- Prove skipped modules are never spawned and their journal and progress states are distinct from a dry run and from successful execution.
- Confirm both JSON and YAML produce equivalent completion declarations and equivalent run summaries.
- Exercise a representative imagery workflow at its actual input granularity.
- Run `make audit`, `make lint`, and `make test` at every phase boundary, not only at the end.

# Acceptance criteria

- `xylem-lib` parses YAML through a maintained implementation and carries no advisory exception for it.
- `xylem-lib` contains no `std::process::exit` and no reachable `unimplemented!` for deserializable public configuration, and every fallible library API returns an owned contextual error.
- Every parsed configuration either validates to a complete execution plan or returns actionable semantic errors, and JSON and YAML produce equivalent plans.
- Dry run mutates nothing and its output is deterministic for the same configuration and declared host facts.
- Resume behavior is explicit and absent from ordinary runs unless enabled.
- Completion is declared outside module implementation and rejected by the same validation stage as every other configuration field.
- Complete work is skipped without spawning its module; partial or stale work reruns; missing and zero-length outputs do not count as complete.
- A completion skip is distinguishable from a dry-run skip in progress, journal state, and final reporters, and the run still finalizes as successful.
- Documentation states the parser contract, the error taxonomy, CLI precedence, the supported resume granularity, and the fingerprint policy.

# Related plans

Executed by this plan:

- [Maintained YAML parser](../migrations/maintained-yaml-parser.md), Phase 1
- [Typed errors and workflow outcomes](../refactors/typed-error-and-outcome-boundary.md), Phase 2
- [Configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md), Phase 3

Not in this plan, but sharing the manifest lock:

- [ACORN 0.2 upgrade](../migrations/acorn-0-2-upgrade.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)

Deferred, and dependent on contracts this plan lands:

- [Process execution contract](../refactors/process-execution-contract.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Structured run reporters](run-reporters.md)
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
