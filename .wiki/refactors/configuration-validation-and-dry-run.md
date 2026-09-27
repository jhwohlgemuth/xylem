---
type: Plan
title: "Validate configuration and make dry-run side-effect free"
description: "Separate parsing, semantic validation, planning, and execution so accepted configurations cannot panic and dry-run only renders a validated plan."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli]
tags: [refactor, configuration, validation, dry-run, planning, paths]
status: planned
---

# Execution

This plan is executed as Phase 3 of [resumable execution](../features/resumable-execution.md), which owns the whole prerequisite chain. Do not schedule it separately. Its `status` flips when that phase lands.

# Goal

Turn a JSON or YAML document into a validated workflow plan before performing any host action. Reject unsupported or contradictory configuration with precise diagnostics, and implement dry-run by rendering that plan without filesystem, network, process, permission, or environment mutation.

# Current evidence

- Public Serde enums accept `InputAttribute::ShellScript`, `ModuleType::Configuration`, seven module languages, lifecycle scripts, checks, resources, output, and option fields, but execution implements only part of that surface.
- Accepted shell input, configuration modules, unsupported languages, schemes, and extensions reach `unimplemented!` rather than validation errors.
- `verify_resources` is a no-op, while `check`, `before_script`, `after_script`, and several output/reporting fields are silently unused.
- Deserialization does not deny unknown fields or validate non-empty module names, unique names, semantic versions, required checksums, template/manager compatibility, or variable names.
- JSON and YAML readers replace I/O failure with synthetic empty content and then parse it, obscuring the original cause.
- `--config` and reporter `--output` use `DirPath` hints even though the former is a file and the latter may be a file destination.
- CLI inputs are consulted only when configuration-derived inputs are absent, so override precedence is implicit.
- A dry run still creates a temporary root and local binary preparation can call `make_executable`; it is therefore not generally side-effect free.
- Directory children from `read_dir` are not sorted, making path-input execution order filesystem-dependent.

# Project code boundaries

- **Update:** configuration types and logic in `xylem-lib/src/config/mod.rs`, related script enums only where validation ownership requires it, CLI argument definitions/orchestration, fixtures, examples, and schema documentation.
- **Create:** separate parse, validate, and execution-plan modules; path-aware validation errors; plan rendering tests.
- **Delete:** synthetic empty-content fallbacks, validation-by-side-effect, panic branches for accepted configuration, and side effects from dry-run code paths.

# Dependencies and ordering

Consumes the [YAML migration](../migrations/maintained-yaml-parser.md) and [typed error boundary](typed-error-and-outcome-boundary.md). It precedes [secure acquisition](secure-module-acquisition.md) and [run reporters](../features/run-reporters.md).

[Resumable execution](../features/resumable-execution.md) waits on this plan and names it as its Phase 1, while keeping ownership of the work here. Resume adds its completion-declaration checks to the stage 2 validation set and evaluates completion while stage 3 renders the plan, so both changes must extend this contract rather than replace it.

# Recommendation

## Four-stage contract

1. **Parse:** read the named file and deserialize its format with path/source context.
2. **Validate:** check schema-independent invariants and classify every declared capability as supported or rejected.
3. **Plan:** resolve paths, input order, variable precedence, acquisition actions, command arguments, and required capabilities without host mutation.
4. **Execute:** carry out the accepted plan and return structured outcomes.

Dry-run stops after stage 3 and serializes the plan. It may inspect explicitly allowed read-only host facts only if the output labels them as observations; it never downloads, clones, chmods, creates environments/directories, spawns modules, or writes environment variables.

## Schema policy

- Add defaults only where omission is semantically valid; distinguish an absent list from an accidentally missing required field.
- Deny unknown fields for stable configuration objects, with a versioned escape/migration policy if forward compatibility is required.
- Validate configuration version, unique/non-empty module names, URI/location kinds, checksums, manager/runtime support, template command/arguments, variable types and reserved names, and input/output path relationships.
- For declared but unimplemented capabilities, either schedule a separate implementation plan or reject them with a named capability error. Do not silently ignore them.
- Define CLI-over-config precedence for every option. CLI `--input` should be either an explicit override or an explicit append mode, not a fallback hidden in control flow.
- Sort path-derived inputs and make duplicate handling explicit.

# Validation

- Add table-driven JSON/YAML parity tests for every field, alias, default, unknown field, invalid combination, and unsupported capability.
- Add snapshots for multi-error diagnostics with configuration paths such as `modules[2].template.command`.
- Run dry-run under filesystem/network/process spies and assert zero mutations, including no temp-directory creation or chmod.
- Test deterministic input ordering and CLI/config precedence.
- Fuzz or property-test parser-to-plan conversion for panic freedom.

# Acceptance criteria

- Every successfully parsed configuration either validates to a complete execution plan or returns one or more actionable semantic errors.
- No deserializable public variant reaches `unimplemented!` during planning or execution.
- Unknown or silently ignored fields are rejected or explicitly versioned.
- Dry-run performs no mutations and its output is deterministic for the same configuration and declared host facts.
- Input ordering, duplicate policy, path base, and CLI precedence are documented and tested.
- JSON and YAML produce equivalent validated plans for equivalent content.

# Related plans

- [Maintained YAML parser](../migrations/maintained-yaml-parser.md)
- [Typed errors and outcomes](typed-error-and-outcome-boundary.md)
- [Secure module acquisition](secure-module-acquisition.md)
- [Resumable execution](../features/resumable-execution.md)
