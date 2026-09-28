---
type: Plan
title: "Add reusable configuration modules"
description: "Compose local, HTTP(S), and Git-backed workflows as flattened, policy-constrained module groups."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli]
tags: [acorn, configuration, modules, workflow, composition]
status: planned
---

# Goal

Make a `configuration` module execute a referenced workflow as a reusable
group of executable leaf modules. Nested workflows must preserve top-level
input, variable, policy, completion, progress, journal, and reporting
contracts while exposing their ancestry deterministically.

# Current evidence

- `ModuleType::Configuration` is accepted by deserialization but rejected by
  validation, module preparation, planning, and execution.
- `ExecutionPlan`, `WorkflowOutcome`, progress updates, and the execution
  journal currently identify modules with a top-level name and index only.
- The CLI resolves options, validates configuration, builds the side-effect-free
  plan, then creates the managed run workspace. Existing remote policy handles
  allowlists, redirects, bounded reachability, downloads, and Git probes.
- `acorn_core::Location` already models local and remote URI schemes, stable
  source hashes, and optional revisions. Xylem still exposes module URIs as
  strings.

# Recommendation

- Replace the URI fields of binary, script, and configuration modules with
  `acorn_core::Location`. Preserve scalar JSON/YAML URI input and add the
  structured `Location` form, including its optional `revision` field.
- Add a source-aware, recursive configuration resolver before immutable
  planning. It resolves `file:` locations relative to the declaring
  configuration, reads direct HTTP(S) JSON/YAML documents in memory, and
  clones Git sources into the managed run workspace. Git sources load required
  repository-root `config.json` and check out `Location.revision`; direct HTTP(S)
  configurations reject revisions.
- Resolve remote configuration sources with the root remote policy before
  planning. A child can only tighten policy: `allowed` intersects the inherited
  allowlist and `offline` is ORed; root-resolved dry-run, resume, batch, and
  verification settings remain authoritative. This source-resolution preflight
  is the narrow documented exception to host-write-free planning because Git
  configuration discovery requires a checkout.
- Reject remote nested sources before `--dry-run` planning, keeping dry run
  network-free. Local nested sources still produce a fully flattened dry-run
  plan.
- Expand wrappers depth-first in place. Each child runs once for the active
  parent input and ignores its own `input` field. Merge variables in this
  order: parent workflow, wrapper, child workflow, child leaf. Reserved values
  win last. A child `output` replaces an inherited output default, explicit
  inherited `OUTPUT_DIRECTORY` values win over that default, and
  `--artifact-output` wins at every depth.
- Set `MODULE_LIST` to the comma-separated, depth-first set of qualified leaf
  identities such as `Preprocess::Normalize`. Validate and verify the
  de-duplicated union of root and reachable child requirements.
- Reject `template` and `completion` on configuration wrappers. Leaf modules
  retain their normal preparation, execution, completion, resume, and
  fail-fast behavior.
- Detect canonical-source cycles with their full module/source chain and reject
  nesting deeper than 16 levels.
- Add `module_path: Vec<String>` to flattened module identities while retaining
  the leaf display name. Publish execution-plan schema v2 and journal/run-summary
  schema v3, updating plans, progress, outcomes, journal events, and reporters
  to use the same identity. Continue rejecting journal versions other than the
  currently supported version.
- Add contextual source-resolution errors that retain ancestor path and source.
  Invalid child documents, cycles, and depth failures are data errors; failed
  remote acquisition remains an availability error.

# Project code boundaries

## Update

- `xylem-lib/src/config/`, `xylem-lib/src/script/mod.rs`, and `xylem-lib/src/plan.rs` for typed locations, recursive resolution, validation, flattening, and plan v2.
- `xylem-lib/src/{outcome,progress}.rs`, `xylem-lib/src/journal/`, and `xylem-cli/src/commands/run/mod.rs` for leaf identity, source preflight, requirements, journal v3, and reporting.
- `docs/schema/`, `docs/src/configuration.md`, `docs/src/architecture.md`, and `MODULE_SPEC.md` for public schemas and the nested-workflow contract.

## Create

- Local, HTTP, and Git configuration-composition fixtures and focused library/CLI integration coverage.
- `docs/schema/execution-plan-v2.schema.json` and `docs/schema/run-summary-v3.schema.json`.

## Delete

None.

# Dependencies and coordination

- Consume the landed validated-planning, resumable-execution, typed-error,
  reporter, and fixed-batching contracts without reintroducing runtime parsing
  or process-global state.
- Coordinate with secure module acquisition over `RemotePolicy`, managed source
  roots, redirects, allowlists, and Git execution. This plan owns recursive
  configuration resolution; acquisition owns generic transport behavior.
- Hold the configuration, process, run-state, and CLI integration locks while
  changing the relevant runtime slices. Schema and documentation changes land
  with the matching data-model implementation.

# Validation

- Test scalar and structured `Location` round trips for all module types,
  including Git revisions.
- Test local nesting, nested-relative paths, variable and output precedence,
  ignored child inputs, qualified `MODULE_LIST`, wrapper-field rejection,
  requirement union, source cycles, and the depth limit.
- Test HTTP allowlist, redirect, offline, malformed-document, and unavailable
  cases; test Git `config.json` discovery and revision checkout.
- Verify local nested dry runs flatten to plan v2 without side effects; verify
  remote nested dry runs make no request and fail with the documented error.
- Verify depth-first leaf ordering, preparation, completion/resume, progress,
  journal transitions, text/JSON reports, and v2/v3 schema validation for both
  successful and failing nested executions.

# Acceptance criteria

- Local, direct HTTP(S), and Git configuration modules execute their leaf
  workflows in place and in deterministic depth-first order.
- Nested leaves receive the documented input, variable, output, requirements,
  and policy context; no child option can loosen root policy.
- Remote source checks honor allowlists, redirects, offline mode, Git revision
  pinning, and the strict network-free dry-run boundary.
- Plans, progress, journals, resumable execution, and reporters expose stable
  leaf names plus machine-readable `module_path` ancestry.
- Invalid child configurations, cycles, depth overflow, unsupported revisions,
  missing Git entrypoints, and inaccessible or disallowed sources fail with
  contextual errors before child execution begins.
