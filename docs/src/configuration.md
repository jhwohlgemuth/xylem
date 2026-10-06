# Workflow configuration

Xylem reads JSON, strict JSONC, YAML, and YML configurations. JSONC accepts
line and block comments plus trailing commas while retaining strict JSON syntax.
A workflow describes requirements,
zero or more inputs, shared variables, ordered modules, and execution options.

```json
{
  "version": "1.0.0",
  "description": "Run a local greeting module",
  "requirements": ["sh"],
  "input": ["first", "second"],
  "output": "/data/artifacts",
  "variables": {
    "GREETING": "Hello"
  },
  "modules": [
    {
      "name": "Greeting",
      "type": "script",
      "programmingLanguage": "generic",
      "uri": "file:./modules/greeting",
      "template": {
        "command": "sh",
        "arguments": ["run.sh", "{{ GREETING }}", "{{ INPUT }}"]
      }
    }
  ]
}
```

Relative `file:` module paths resolve from the directory where Xylem is
launched. Modules execute in array order for each active input.

## Opt-in sandboxing

Sandboxing is disabled by default. Omitting `sandbox`, or setting it to
`false`, preserves the existing host execution path and requires no sandbox
policy or container runtime.

Set the single top-level field below to require sandboxed execution:

```yaml
sandbox: true
```

An opted-in run also requires the operator-controlled global option
`--sandbox-policy <PATH>`. Supplying a policy does not activate sandboxing, and
supplying one for a workflow without `sandbox: true` is an error. A requested
sandbox never falls back to host execution.

Policy documents use strict JSON, JSONC, YAML, or YML and select rootless
Docker or Podman, a digest-pinned image, allowed host roots and child
environment names, resource ceilings, preparation networking, and cleanup.
Relative policy roots resolve from the policy file. Dry-run validates the
workflow and policy and records their effective values without contacting the
runtime. The initial slice requires cleanup retention `delete`.
The checked-in [`sandbox-policy-v1` schema](../schema/sandbox-policy-v1.schema.json)
defines the complete document shape. Real execution is currently qualified on
native x86_64 Linux with a local rootless runtime. The pinned image must
already be present; the guarded slice never performs an implicit pull.

The initial OCI execution slice supports local generic script modules with
read-only module and input mounts. Workflow requirements must be supplied by
the pinned image. Remote, binary, Python-environment, nested-configuration,
application-container, GPU, completion-marker, and artifact-publication
features fail closed in sandbox mode while their existing host behavior
remains available when sandboxing is disabled.

Explicit input lists preserve their declared order and duplicate values keep
their first occurrence. Directory-derived inputs are sorted by path. CLI
`--input` replaces configured input values rather than appending to them.

## CEL input selection

A workflow can discover local files and directories beneath explicit roots and
select inputs with a CEL expression:

```yaml
input:
  select:
    engine: cel
    options:
      roots:
        - ./scenes
      recursive: true
    script: >-
      items
        .filter(item,
          item.kind == "file" &&
          item.size > 0 &&
          (item.extension == ".tif" || item.extension == ".tiff"))
        .map(item, item.path)
```

For readable multiline YAML expressions, `>-` folds ordinary line breaks into
spaces and removes the final newline. Use `|-` when the expression's newline
characters should be preserved; CEL accepts them as whitespace.

Relative roots resolve from the declaring configuration's directory. Roots
must be existing local directories and cannot be symlinks. Discovery never
follows symlinks, does not read file contents, and skips symlinks and special
files. Candidates retain configured root order and are sorted by `/`-separated
relative path within each root. Overlapping roots keep the first occurrence.

CEL receives only three immutable bindings:

- `items`, an ordered list of candidate records;
- `options`, containing canonical `roots` and the effective `recursive` value;
- `variables`, the configured workflow variables without reading the process
  environment.

Each item provides `index`, `path`, `relative_path`, `name`, `extension`,
`kind`, `size`, and `depth`. JSON/YAML null, boolean, number, string, list, and
map variable values map recursively to the corresponding CEL values. Nested
values passed to child processes are encoded as compact JSON strings.

The supported CEL subset includes literals, field selection, indexing,
conditionals, core comparison/arithmetic/logical operators, the list macros
`has`, `all`, `exists`, `exists_one`, `filter`, and `map`, and the pure
functions `size`, `contains`, `startsWith`, `endsWith`, `int`, `uint`,
`double`, and `string`. Macros may iterate over lists only. Bytes, regular
expressions, time, optional values, custom functions, and filesystem, process,
network, environment, clock, random, or locale access are unavailable.

The result must be `list<string>`, and every string must exactly equal a
discovered item's `path`. Result order is preserved and later duplicates are
removed. An empty result runs the workflow once without an input, matching the
existing zero-input behavior.

Discovery and evaluation use fixed limits: 16 roots, depth 32, 100,000 visited
entries, 50,000 candidates, 16 MiB of candidate paths, 250,000 metadata
operations, a 16 KiB script, 64 AST levels, 1,024 AST nodes, 10,000,000 units
of statically estimated evaluation cost, and 50,000/16 MiB intermediate and
result records/bytes. Variables are limited to depth 32, 10,000 nodes, and 1
MiB per scalar. Exceeding any limit fails before workflow execution.

A non-empty CLI `--input` list has precedence and bypasses selection request
validation, root access, discovery, compilation, and evaluation. Dry-run uses
the same read-only selection snapshot as execution. Execution-plan schema v8
records canonical options, limits, engine/interpreter identity, counts, and
SHA-256 script and inventory digests without recording the script, configured
variable values, or unselected inventory.

## Reusable configuration modules

Use a `configuration` module to expand another workflow in place:

```json
{
  "name": "Preprocess",
  "type": "configuration",
  "uri": "file:./workflows/preprocess.json",
  "variables": {
    "METHOD": "normalized"
  }
}
```

Nested workflows expand depth-first to executable leaves. Local paths are
relative to the declaring configuration. HTTP(S) `.json`, `.yaml`, and `.yml`
locations are read directly; Git locations load repository-root `config.json`
and may use the structured form
`{"scheme":"https","uri":"https://example.org/workflow.git","revision":"v1.2.3"}`.

The root input and run options remain authoritative. Variables merge in
parent, wrapper, child, then leaf order; child outputs replace inherited output
defaults unless `OUTPUT_DIRECTORY` was explicitly set. Child requirements are
de-duplicated into the root workflow, and reports identify leaves with a
qualified `module_path`. Wrappers cannot declare `template` or `completion`.
Cycles and nesting beyond 16 wrappers are rejected before execution.

## YAML behavior

YAML configuration files may use anchors, aliases, and merge keys. Explicit
mapping fields take precedence over merged fields; for a merge sequence, the
first mapping takes precedence over later mappings. Quoting a scalar prevents
YAML from interpreting values such as `null`, `true`, or numbers as other
types.

Xylem accepts exactly one YAML document per file. It rejects duplicate keys,
unknown configuration fields, malformed merges, unresolved aliases, and input
that exceeds the parser's recursion or alias-expansion limits. JSON uses the
same required-field, unknown-field, and semantic validation policy.

## Variables

Workflow variables are available to every module. Module variables override
workflow values. The top-level `output` field supplies `OUTPUT_DIRECTORY` when
the workflow has not defined it; a module may override that default. Xylem then
supplies reserved values such as `MODULE_LIST`, `INPUT`, and
`PARENT_DIRECTORY`. Placeholders use `{{ VARIABLE_NAME }}` syntax.

Resolved values are passed to child processes with an `_XYLEM_` prefix. For
example, `GREETING` is available as `_XYLEM_GREETING`.

## Options

Configuration options include `batch`, `dry_run`, `gpu_monitor`, `offline`, `allowed`,
`resume`, `skip_verify_checksum`, and `skip_verify_requirements`. Enabling a
Boolean CLI flag overrides a false or absent configuration value. When
supplied, `--allowed`, `--input`, and `--artifact-output` replace their
configured values.

`gpu_monitor` enables run-wide NVIDIA GPU telemetry at a fixed one-second
interval. Xylem invokes `nvidia-smi` on the host, journals raw utilization and
frame-buffer memory readings, and reports compact per-GPU averages and peaks.
The readings are device-wide and may include activity from unrelated processes
on a shared host. Monitoring is represented in dry-run plans without invoking
`nvidia-smi`.

`resume` evaluates each module at top-level input granularity. A module is
skipped only when all declared output globs resolve to non-empty files and its
explicit completion marker contains the current canonical identity. Missing,
empty, partial, or stale output state runs normally.

`batch` is an optional positive integer that caps concurrently active top-level
inputs. Its effective limit is `min(batch, --threads)`; when omitted, input
execution retains the `--threads` limit. Admission is rolling, so another input
may start as soon as one finishes. For example:

```json
{
  "options": {
    "batch": 2
  }
}
```

The scheduling unit is each configured input value, or each immediate child of
a configured input directory. A module remains sequential within one input.
Xylem cannot count or limit image pairs, threads, or other work that a child
module processes internally.

`allowed` is an optional list of exact `HOST` or `HOST:PORT` authorities:

```json
{
  "options": {
    "allowed": ["code.ornl.gov", "artifacts.example.org:8443"]
  }
}
```

Host matching is case-insensitive and does not support wildcards or domain
suffix matching. A host without a port permits the default port for the URL's
scheme; a non-default port must be listed explicitly. When this option is
absent or empty, remote hosts remain unrestricted for backward compatibility.
The allowlist is enforced before remote work and across HTTP redirects.

Unless `offline` is active, Xylem checks an HTTP(S) binary or Git repository
for reachability before downloading or cloning it. The execution plan records
the acquisition action without performing it. Dry-run does not create a run
directory, probe the network, change permissions, spawn a process, or write an
environment variable.

Nested workflows may only tighten the inherited allowlist and may enable
offline mode. A remote configuration wrapper fails without making a request in
dry-run mode; local wrappers still produce a complete flattened plan.

See the repository's
[module specification](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md)
for the normative Xylem 1.x module contract.
