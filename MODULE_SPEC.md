# Xylem Module Specification

**Specification version:** 1.0  
**Status:** Stable

This document defines the module contract supported by Xylem 1.x. The key words
**MUST**, **SHOULD**, and **MAY** are normative.

## 1. Module declaration

A workflow declares modules in its top-level `modules` array. Modules execute in
array order for each workflow input.

```json
{
  "name": "Example Module",
  "type": "script",
  "programmingLanguage": "python",
  "uri": "file:./modules/example",
  "variables": {
    "OUTPUT_DIRECTORY": "{{ WORK_DIRECTORY }}/output"
  },
  "template": {
    "environment": {
      "name": "default",
      "manager": "pixi"
    },
    "command": "python",
    "arguments": ["-m", "lib.run", "--output", "{{ OUTPUT_DIRECTORY }}"]
  }
}
```

The fields have these meanings:

| Field | Required | Contract |
| --- | --- | --- |
| `name` | Yes | Human-readable module name. Xylem converts it to snake case in `MODULE_LIST`. |
| `type` | Yes | `script`, `binary`, or the reusable workflow wrapper `configuration`. |
| `uri` | Yes | A scalar local `file:` URI or a structured remote location with `scheme`, `uri`, and an immutable Git `revision` where applicable. |
| `programmingLanguage` | No | `python` or `generic` in version 1.0. Omission selects generic behavior; the JSON aliases `language` and `programming_language` are also accepted. |
| `description` | No | Human-readable purpose. |
| `variables` | No | Module-scoped scalar variables. These override workflow variables. |
| `template` | No | An inline template or a path relative to the module directory. |
| `completion` | No | Explicit marker and output declarations used by resumable execution. |
| `timeout_seconds` | No | Positive execution timeout for `script` and `binary` modules. Omission applies no module timeout. |
| `checksum` | Remote binary and direct HTTP configuration modules | Lowercase SHA-256 digest prefixed with `sha256:`. |

`configuration` modules do not execute and MUST NOT declare `timeout_seconds`.
Xylem currently validates and preserves executable-module timeouts; the
remaining environment execution work will apply them to module processes.
They do not include acquisition or environment preparation.

## 2. Module URI and directory

A local module MUST use a `file:` URI. Relative paths resolve from the directory
where Xylem is launched, so workflows intended for repository-root execution
SHOULD use paths such as `file:./modules/example`.

Module sources are copied into isolated workspaces for host and sandbox
preparation. These copies omit installed `.pixi/envs/` entries, including in
nested projects, before traversing directories or resolving their symlinks.
This also applies to generic script modules using `file:.`. Source files,
datasets, `pyproject.toml`, `pixi.toml`, `pixi.lock`, and `.pixi/config.toml`
remain available. Xylem does not apply `.gitignore` rules to these copies.
Broken, escaping, or cyclic symlinks in included content are still rejected.

```text
example/
├── template.json       # optional command interface
├── pixi.toml           # optional Pixi manifest
├── environment.yml     # optional Conda manifest
└── lib/                # module implementation; layout is language-specific
```

Remote script modules MUST select a full lowercase 40- or 64-hex commit ID in
`revision`. Xylem verifies that commit and normalized origin in its immutable
cache, then copies the repository into the per-run working directory before
execution and, for Python modules, environment preparation. HTTPS is accepted
by default. HTTP requires `--allow-insecure-http`; SSH requires an explicit
operator credential policy.

## Configuration modules

A `configuration` module references another JSON or YAML workflow and expands
in place to its executable leaves. Local `file:` references are relative to the
configuration that declares them. An HTTP(S) URI ending in `.json`, `.jsonc`,
`.yaml`, or `.yml` is a direct document and MUST provide `checksum`; Xylem reads
it only after verifying that digest. Other remote locations are treated as Git
repositories whose root must contain `config.json` and MUST select a full
lowercase 40- or 64-hex commit ID in `revision`. Direct HTTP documents MUST NOT
declare `revision`.

Expansion is depth-first and limited to 16 nested wrappers. Xylem rejects
source cycles and reports each leaf with its full `module_path`, such as
`Preprocess::Normalize`. A wrapper may declare `variables`, but not `template`
or `completion`. Child `input` is ignored because the root input remains the
execution unit. Requirements are de-duplicated across all reachable workflows.

Variables are merged from the parent workflow, wrapper, child workflow, and
child leaf in that order. A child `output` replaces the inherited output
default, while an explicit `OUTPUT_DIRECTORY` variable and the CLI
`--artifact-output` override remain authoritative. `MODULE_LIST` contains the
qualified, depth-first leaf identities.

The root dry-run, resume, batch, and artifact-output choices apply at every
depth. A child may tighten `options.allowed` and `acquisition_limits` or enable
`offline`; it cannot loosen inherited policy. Dry-run MAY read a matching
verified cache entry but MUST NOT connect, invoke Git, create a destination, or
populate the cache. Offline mode follows the same cache-only rule and reports a
typed cache miss rather than falling through to the network.

## Completion and resume

A module may declare the files that establish completion and an optional marker:

```yaml
completion:
  marker: "{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/.xylem/module.json"
  outputs:
    - "{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/*.tif"
```

When `marker` is omitted, Xylem places it in a `.xylem` directory beneath the
literal directory prefix of the first output pattern and names it after the
module in snake case. For example, the output pattern above defaults to
`{{ OUTPUT_DIRECTORY }}/{{ PARENT_DIRECTORY }}/.xylem/<module_name>.json`.

Every output pattern must match at least one non-empty regular file, and the
marker identity must match the module, resolved template, variables, input, and
completion declaration. The marker and outputs must remain below `output` or
the module's effective `OUTPUT_DIRECTORY`. Xylem writes the marker only after a
successful module execution. Resume is opt-in through `options.resume` or
`--resume`; ordinary runs always execute the module.

The workflow `output` field is the default artifact root and supplies
`OUTPUT_DIRECTORY` unless a workflow or module variable overrides it. The CLI
`--artifact-output` value replaces `output` for one run. Report and journal
placement is separate and uses `--report-output` (`--output` remains its legacy
alias).

Workflows MUST allow every remote module host with `options.allowed`, whose
entries are exact host names or IP literals with an optional non-default port.
Xylem checks the original URL and HTTP redirect targets against that list as
part of bounded acquisition; it does not perform a separate reachability
probe. An absent or empty allowlist denies remote acquisition. `--allowed`
replaces the configured list for a run.

## 3. Command template

A template MUST contain `command` and `arguments`. It MAY contain `environment`
and `container`.

A `container` MUST provide a logical `name` and MAY select `runtime` (`docker`
by default, `podman`, or externally managed `apptainer`). With the default
`create: false`, Xylem verifies a named Docker or Podman container and delegates
the command with `exec`; it never starts, stops, or removes that container.

With `create: true`, Xylem requires Docker or Podman plus an `image`, creates a
run-scoped container after module preparation, waits for running/healthy state,
and delegates every referencing module to it. A managed declaration MAY provide
either `command` or `keep_alive: true`, explicit `mounts` (`source`, absolute
`target`, and `read_only`), `startup_timeout_seconds`, and `cleanup` (`always`,
`on_success`, or `never`). `keep_alive` runs `sleep infinity` directly without a
shell and therefore requires a Linux image whose `sleep` implementation accepts
`infinity`; it cannot be combined with `command`. Omitting both fields preserves
the image's configured entrypoint and command.
Relative mount sources resolve against the file that declares them. No host path
is mounted implicitly. `--container-cleanup` overrides cleanup for one run.

Resolved `_XYLEM_*` values are forwarded to container commands by default.
`--no-container-env` disables that forwarding without changing variables already
defined by the image or existing container.

`arguments` supports two representations:

- Exec form, an array of argument strings. This form SHOULD be used because each
  entry remains one process argument after interpolation.
- Shell form, one whitespace-separated string. This form does not provide shell
  quoting or expansion semantics.

When the module declaration omits `template`, Xylem looks for `template.json` in
the module directory. Python modules without that file use Xylem's default
Python command interface.

## 4. Variables and interpolation

Variable names MUST use upper snake case. Values MAY be strings, numbers, or
booleans. Xylem constructs the environment in this order:

1. workflow `variables`;
2. workflow `output` as the default `OUTPUT_DIRECTORY` when that variable is absent;
3. module `variables`, overriding equal workflow keys and the output default;
4. reserved Xylem values, overriding user-defined equal keys.

String values may reference other values with `{{ VARIABLE_NAME }}`. Resolution
is recursive, so a module override also affects workflow values that reference
the overridden key. Unknown or cyclic references remain visibly unexpanded.
Interpolation replaces complete placeholders only; plain text matching a
variable name is not modified.

The resolved values are used both for template-argument interpolation and as
child-process environment variables with the `_XYLEM_` prefix. For example,
`OUTPUT_DIRECTORY` is exposed as `_XYLEM_OUTPUT_DIRECTORY`.

Xylem reserves these values:

| Value | Availability | Meaning |
| --- | --- | --- |
| `MODULE_LIST` | Always | Comma-separated snake-case module names in execution order. |
| `INPUT` | When an input is active | Current workflow input value. |
| `PARENT_DIRECTORY` | When an input is active | Final path component of `INPUT`. |

## 5. Python environments

### Pixi

A Pixi module MUST contain either `pixi.toml` or a `pyproject.toml` with a
`[tool.pixi]` table. Its template SHOULD declare the environment explicitly:

```json
"environment": {
  "name": "default",
  "manager": "pixi"
}
```

During preparation Xylem runs this command in the module directory:

```shell
pixi install --environment <name>
```

It then executes the module through:

```shell
pixi run --environment <name> <command> <arguments...>
```

When no template declares an environment, Xylem auto-detects a Pixi manifest
and uses the `default` environment.

### Conda

A Conda module MUST contain `environment.yml` with its environment `name` on the
first line. With `"manager": "conda"`, Xylem creates, discovers, and executes
the named environment with `conda env` and `conda run --name <name>`.

### Mamba

A Mamba module uses the same `environment.yml` contract as Conda. With
`"manager": "mamba"`, Xylem creates, discovers, and executes the named
environment with `mamba env` and `mamba run --name <name>`.

## 6. Binary modules

A local binary URI MUST identify an existing file. Xylem ensures it is
executable before running it. Under normal policy, a remote binary MUST supply
a valid algorithm-qualified SHA-256 checksum. Xylem streams and verifies
bounded bytes into an immutable cache, then copies the binary into the owned
run workspace. The operator MAY use `--skip-verify-checksum` as an emergency
escape hatch; Xylem MUST warn, enforce all non-identity acquisition limits,
record the observed digest, and MUST NOT use an offline cache entry without a
pretrusted identity. HTTPS is supported by default; HTTP requires
`--allow-insecure-http`.

## 7. Conformance checklist

A conforming module:

- has a unique, descriptive `name` within its workflow;
- uses a supported type, language, and URI scheme;
- provides a command template whose exec-form arguments need no shell parsing;
- treats `_XYLEM_*` variables as the process environment contract;
- includes the manifest required by its declared environment manager;
- can run repeatedly without depending on undeclared files outside its module
  directory; and
- keeps secrets out of committed workflow variables and templates.

The runnable [Python with Pixi example](./examples/python-with-pixi/README.md)
is the reference module for this specification.
