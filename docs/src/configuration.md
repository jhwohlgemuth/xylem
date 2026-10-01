# Workflow configuration

Xylem reads JSON and YAML configurations. A workflow describes requirements,
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

Explicit input lists preserve their declared order and duplicate values keep
their first occurrence. Directory-derived inputs are sorted by path. CLI
`--input` replaces configured input values rather than appending to them.

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
