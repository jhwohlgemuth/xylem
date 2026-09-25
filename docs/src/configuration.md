# Workflow configuration

Xylem reads JSON and YAML configurations. A workflow describes requirements,
zero or more inputs, shared variables, ordered modules, and execution options.

```json
{
  "version": "1.0.0",
  "description": "Run a local greeting module",
  "requirements": ["sh"],
  "input": ["first", "second"],
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

## Variables

Workflow variables are available to every module. Module variables override
workflow values. Xylem then supplies reserved values such as `MODULE_LIST`,
`INPUT`, and `PARENT_DIRECTORY`. Placeholders use `{{ VARIABLE_NAME }}` syntax.

Resolved values are passed to child processes with an `_XYLEM_` prefix. For
example, `GREETING` is available as `_XYLEM_GREETING`.

## Options

Configuration options include `batch`, `dry_run`, `offline`, `allowed`,
`skip_verify_checksum`, and `skip_verify_requirements`. Equivalent Boolean CLI
flags are combined with the configuration values. When supplied, `--allowed`
replaces rather than extends `allowed`.

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
for reachability before downloading or cloning it. Dry runs perform this
preflight without acquiring content. Offline dry runs report remote modules as
skipped; an offline real run rejects them because Xylem's per-run module
directory has no persistent remote cache.

See the repository's
[module specification](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md)
for the normative Xylem 1.x module contract.
