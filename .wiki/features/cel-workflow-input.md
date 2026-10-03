---
type: Plan
title: "Scripted workflow input discovery and filtering"
description: "Discover and filter workflow inputs with a bounded selection engine over an immutable filesystem candidate inventory, using CEL as the initial engine."
difficulty: medium
impact: medium
package: [xylem-lib, xylem-cli]
tags: [feature, input, cel, discovery, filtering, planning, determinism]
status: planned
priority: backlog
---

# Goal

Let a workflow provide a selection script that finds and filters its inputs.
Xylem performs bounded, deterministic discovery beneath explicit local roots
and passes the resulting immutable candidate inventory to a selection engine.
The initial engine evaluates CEL. The script selects the files or directories
that become workflow inputs.

This is the primary use case for `InputAttribute::Select`: replace bespoke shell
scripts used to scan a data tree, filter by name or metadata, and emit paths.
The selected paths use the same ordered, deduplicated input model as configured
lists, directory inputs, and CLI inputs.

The selection feature replaces the reserved `InputAttribute::ShellScript`
variant. The feature is not active today, so there is no shell-input
compatibility contract to retain. Xylem owns filesystem discovery, path
containment, and limits. Selection engines receive data and cannot perform I/O.

# Non-goals

- Give a selection engine direct filesystem, process, network, environment,
  clock, random, or locale-sensitive capabilities.
- Read file contents, inspect domain-specific formats, calculate file hashes,
  fetch remote candidates, or watch directories for changes.
- Let the script return paths that were not present in the discovered candidate
  inventory.
- Turn a selection script into a general workflow language or use it for module
  execution, completion checks, policy decisions, or template expansion.
- Preserve the unpublished `ShellScript(ScriptTemplate)` configuration shape.
- Add user-defined functions, external script files, or a dynamic engine ABI in
  the first release.

# User-facing contract

Replace the reserved shell payload with a structured selection request. CEL is
the only initial engine:

```yaml
input:
  select:
    engine: cel
    options:
      roots:
        - /data/scenes
      recursive: true
    script: >-
      items
        .filter(item,
          item.kind == "file" &&
          item.size > 0 &&
          (item.extension == ".tif" || item.extension == ".tiff"))
        .map(item, item.path)
```

The equivalent JSON shape is:

```json
{
  "input": {
    "select": {
      "engine": "cel",
      "options": {
        "roots": ["/data/scenes"],
        "recursive": true
      },
      "script": "items.filter(item, item.kind == 'file').map(item, item.path)"
    }
  }
}
```

Define deny-unknown-fields `InputSelection`, `SelectionRequest`, and
`SelectionOptions` types, then add
`InputAttribute::Select(InputSelection)`. `SelectionRequest` has a required
closed `engine: SelectionEngine` value, a required
`options: SelectionOptions` value, and a required, non-empty `script`.
`SelectionEngine` initially contains only `Cel`. `SelectionOptions` contains:

- `roots`: a required, non-empty ordered list of local directories;
- `recursive`: whether discovery descends below each root, defaulting to
  `false`.

Pass the normalized `SelectionOptions` value to the selected engine as the
immutable `options` binding. This lets a script inspect the effective roots and
recursion policy without granting authority to change them.

The object wrapper keeps selection unambiguous within the existing untagged
string/list/object input syntax. Resolve each root relative to the configuration
file that declares it, using the configuration-owned path rules established by
the immutable run context. Roots must be local directories. Remote URIs and
non-directory roots fail validation.

# Candidate inventory

Build one immutable candidate record for each discovered regular file or
directory. A root is a search boundary and is not itself a candidate. Each
selection item exposes:

- `index`: the zero-based index of its configured root;
- `path`: the resolved UTF-8 path that becomes the workflow input when selected;
- `relative_path`: the `/`-separated path beneath its root;
- `name`: the final path component;
- `extension`: the final extension including `.`, or an empty string;
- `kind`: `file` or `directory`;
- `size`: the regular-file size in bytes, or `0` for a directory; and
- `depth`: the number of relative path components beneath the root.

Use filesystem metadata only for these fields. Do not open candidate files.
Inspect each root without following it, reject a symlink or special-file root,
and then canonicalize the accepted directory. Never follow symlinks during
traversal. Do not emit symlinks, sockets, devices, or other special entries. An
unreadable directory or metadata failure is a typed discovery error instead of
a partial inventory.

Preserve configured root order. Within each root, sort candidates by normalized
`relative_path` before evaluation. When multiple roots expose the same resolved
path, retain its first occurrence. Reject a path that cannot be represented as
UTF-8 at the candidate boundary.

Bound discovery before it begins and while it runs. The effective limit profile
must include root count, traversal depth, visited entries, candidate records,
aggregate path bytes, and metadata operations. Exceeding a limit returns a typed
error and no partial result.

# Selection environment and result

The expression environment contains three bindings:

- `items`: the ordered immutable candidate records;
- `options`: the normalized `SelectionOptions` value used for discovery; and
- `variables`: the workflow's configured `variables` object, converted
  recursively from JSON/YAML values without reading `_XYLEM_*` or the parent
  process environment.

Evaluate after nested configuration resolution has produced the effective root
workflow and after candidate discovery, but before module-specific environment
strings are constructed. The initial CEL engine must document the accepted
JSON/YAML-to-CEL mapping for null, boolean, integer, unsigned integer,
floating-point, string, list, and map values. Reject values that the selected
engine cannot represent without loss.

The script result must be `list<string>`. Every string must exactly match the
`path` field of a discovered candidate. Reject unknown paths, null, scalar, map,
bytes, mixed lists, empty strings, and values containing NUL. Preserve script
order and remove later duplicates with the first-occurrence policy used for
configured lists and CLI inputs. An empty result retains the existing single
no-input execution represented internally as `vec![None]`.

Selected files and directories receive the same planning, workspace, and
sandbox path handling as ordinary configured inputs. Scripted selection grants
no additional execution-time access.

# Evaluation boundary

Create a small capability-owned selection boundary with separate discovery,
compile, and evaluate operations. Configuration validation checks the wrapper,
engine, roots, and non-empty script. After CLI precedence selects configured
scripted input, planning:

1. resolves and validates the roots;
2. builds the bounded candidate inventory;
3. compiles the script and reports syntax or declaration errors at
   `input.select.script`;
4. evaluates it against `items`, `options`, and `variables`; and
5. validates and normalizes the selected candidate paths.

The initial `SelectionEngine::Cel` implementation uses only CEL standard
operators, macros, and pure functions implemented with the semantics required
by the CEL specification. Do not register filesystem, process, network,
environment, time, random, or locale-sensitive functions. Pin the interpreter
version and verify the required language subset with upstream conformance cases
before accepting the dependency. If the Rust interpreter cannot enforce the
limits below or differs on required list, map, numeric, string, or macro
behavior, narrow and document the supported subset.

In addition to discovery limits, enforce limits for script bytes, parsed AST
depth and node count, evaluation cost, result records, and aggregate result
bytes. The interpreter must expose deterministic cost accounting or an
equivalent bounded evaluation mechanism. A wall-clock timeout alone is not an
acceptable evaluation limit.

Return a typed selection error that distinguishes unsupported engine, root
resolution, discovery, parse, check, binding, evaluation, result-type,
unknown-candidate, and limit failures. Preserve engine diagnostics with source
spans where available, but translate them at the Xylem boundary so the public
error contract is not an interpreter crate's error type.

# Planning and CLI precedence

CLI `--input` remains highest precedence. A non-empty CLI input list bypasses
root validation, discovery, selection compilation, and evaluation. An invalid,
unreadable, or expensive unused selection request cannot prevent an explicit
CLI override from running.

Without a CLI override, ordinary execution and `--dry-run` use the same
read-only discovery, compile, evaluation, and normalization path. Dry-run may
read configured roots and candidate metadata, as the existing directory input
does. It performs no writes, network requests, process spawns, or runtime
provisioning. One invocation uses one candidate snapshot; execution does not
repeat discovery after the plan is built.

Extend the versioned execution-plan input section with:

- source kind `selection` and engine `cel`;
- normalized discovery options, including resolved roots and recursive policy;
- a digest of the original script;
- the selection engine, language/version, and interpreter identity;
- the effective discovery and evaluation limit profile;
- candidate count, skipped unsupported-entry count, and inventory digest; and
- the normalized selected inputs.

Do not place the full script, configured variable values, or unselected
candidate inventory in plan or journal metadata merely to provide provenance.
Those values may be sensitive or large. A plan-schema change must use a new
schema version and file rather than modifying a released schema in place.
Scripted selection needs no discovery journal transition because input
discovery finishes before journal creation.

# Implementation phases

## Phase 1: Selection engine and item qualification

Define the closed `SelectionEngine` boundary and choose and pin a Rust CEL
interpreter as its first implementation through the dependency-review process.
Define the supported CEL subset, `SelectionOptions` and item schemas,
JSON/YAML conversion table, resource limits, compile/evaluate interface, and
typed errors. Add conformance tests for the operators, macros, value types,
record access, options access, and deterministic cost behavior used by
selection scripts.

## Phase 2: Bounded deterministic discovery

Add the structured selection request, `SelectionOptions`, and a pure inventory
model over an injected filesystem walker. Resolve roots relative to their
declaring configuration, enforce containment and traversal limits, reject
partial discovery, and produce stable candidate ordering and inventory digests.

## Phase 3: Filtering and normalization

Replace `ShellScript(ScriptTemplate)` with `Select(InputSelection)`. After CLI
precedence is resolved, discover candidates, evaluate the selected script with
the configured engine, require every result to match the inventory, and route
selected paths through the shared ordered deduplication path.

## Phase 4: Plan, CLI, and documentation

Record bounded selection-engine and inventory provenance in a new execution-plan schema.
Preserve the pre-journal input boundary, scheduler admission, resume behavior,
and reporter semantics. Document the request shape, candidate fields, root and
symlink behavior, supported engine and CEL subset, limits, CLI precedence, and
diagnostics. Add equivalent JSON and YAML fixtures plus compiled-CLI coverage.

# Validation

- Unit tests for missing, duplicate, relative, absolute, unreadable, and
  non-directory roots; direct and recursive discovery; root ordering; stable
  relative-path ordering; files and directories; extensions; sizes; depth;
  non-UTF-8 paths; symlinks; special files; and every discovery limit.
- Unit tests for parse and declaration errors, missing variables, item and
  options field access, result type, unknown candidate paths, empty strings,
  NUL, mixed lists, duplicates, script ordering, empty output, numeric
  conversion, and every evaluation limit.
- Conformance tests for each supported CEL operator, macro, and value type.
- Property tests that traversal never escapes a root, normalization preserves
  first occurrence order, and normalization is idempotent.
- Integration proof that CLI inputs bypass root access, discovery, compilation,
  and evaluation.
- Dry-run proof that selection reads only the declared roots and metadata and
  performs no write, process, network, environment, or runtime action.
- Determinism proof that the same configuration, candidate snapshot, and
  platform path mapping produce byte-identical selected inputs and provenance.
- Drift proof that a changed candidate inventory changes its digest and the
  resulting plan without reusing stale selections.
- Regression proof that string, list, directory, batching, and no-input
  behavior remain unchanged.
- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`

# Acceptance criteria

- A workflow can provide explicit local roots and a selection script using the
  CEL engine that filters discovered files or directories into
  `Vec<Option<String>>`.
- Every selected input came from the bounded candidate inventory; the selection
  engine cannot inject an undiscovered path.
- Discovery is deterministic, containment-checked, read-only, and bounded by a
  tested limit profile.
- Selection evaluation can access only the documented immutable bindings and
  pure language subset.
- Invalid discovery, scripts, and results produce typed, path-qualified
  diagnostics.
- Dry-run produces the final resolved input plan without writes, network,
  processes, or runtime provisioning.
- CLI input precedence and all existing input forms remain unchanged.

# Dependencies

- Engine implementation selection, pinning, licensing, and advisory policy must satisfy
  the required dependency and security checks.
- Preserves the input scheduling and configuration-owned path contracts
  established by [workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md).
- Does not depend on [sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md).
  Xylem performs bounded local discovery in its trusted planning process, and
  the selection evaluator has no process, network, or filesystem capability.
- Remote discovery is outside this plan and would require the secure
  acquisition policy and a separate cache/provenance contract.
