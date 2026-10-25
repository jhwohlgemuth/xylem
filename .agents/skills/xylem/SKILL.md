---
name: xylem
description: >-
  Operate and develop Xylem workflows. Use when creating, editing, validating,
  exporting, running, or diagnosing Xylem configuration; working with JSON, JSONC,
  YAML, module requirements, workflow execution, retained run history, journals,
  artifacts, containers, sandboxes, or the Xylem dashboard.
---

# Xylem

Xylem runs structured data workflows and retains durable execution state. Treat the current configuration reference, module specification, and CLI help as authoritative; do not invent fields, module options, flags, or output locations from memory.

When working in the Xylem repository, read `AGENTS.md` first and follow its required skills and verification guidance.

## Workflow

```text
1. Identify the operation and whether it reads, changes, downloads, or executes.
2. Confirm the configuration, selected modules and inputs, output locations, and network policy.
3. Inspect current CLI help and the configuration reference when syntax or shape matters.
4. Use dry-run, export check mode, or another preview when supported.
5. Run the narrowest command that satisfies the request.
6. Inspect the resulting configuration, run summary, journal, or artifact.
7. Report the command, affected paths, and verification performed.
```

## Invocation

Use an installed binary:

```shell
xylem --version
xylem --help
xylem run --help
```

Run the checkout without installing:

```shell
cargo run --package xylem-cli -- <command> [arguments]
```

Examples:

```shell
xylem run workflow.yaml --dry-run
xylem run workflow.json --offline --report-output ./reports
xylem export workflow.json --gitlab --check
xylem tui workflow.json
```

Global options such as `--threads`, `--env`, `--no-progress`, and `--sandbox-policy` are defined by current top-level help. Use generated help under `docs/src/generated/` as the checked-in command reference and refresh it with `make help` after changing CLI syntax.

## Configuration

Xylem discovers `.xylem.json`, `.xylem.jsonc`, `.xylem.yaml`, then `.xylem.yml` in the current directory when a command accepts an omitted configuration path. Prefer an explicit path when multiple configurations may exist.

Before creating or substantially editing configuration:

- Read `docs/src/configuration.md` and `MODULE_SPEC.md`.
- Confirm exact keys, types, defaults, and validation rules from the current code and documentation.
- Preserve the user's existing module order and authored values.
- Keep JSON, strict JSONC, YAML, and YML behavior consistent with the documented contract.
- Run the narrowest command or test that validates the changed configuration.

## Run workflows

```shell
xylem run workflow.yaml --dry-run
xylem run workflow.yaml --input sample-a,sample-b
xylem run workflow.yaml --module-index 0
xylem run workflow.yaml --offline
xylem run workflow.yaml --resume
```

Execution may create artifacts, acquire remote sources, start containers, and write a durable run journal and reports. Confirm the selected configuration, output paths, remote hosts, and network settings before an operation with external effects. Use `--dry-run` to inspect a run when available. Use `--offline` to prevent internet access; explain any functionality unavailable offline.

Run reports and journals are retained in the configured report output or Xylem's platform cache. Resolve the actual path from the command output/configuration and inspect the stored result rather than assuming an output location.

## Export workflows

```shell
xylem export workflow.yaml --gitlab --check
xylem export workflow.yaml --gitlab --output .gitlab-ci.yml
xylem export workflow.yaml --gitlab --force
```

Use `--check` to detect whether the generated file differs. `--force` replaces an existing output. After export, inspect the resulting file and report its exact path.

## Dashboard and retained history

```shell
xylem tui
xylem tui workflow.yaml
```

The dashboard can display retained run history and preload a configuration for a new run. Verify that a selected history entry corresponds to the expected workflow and run before using its logs or summaries.

## Common workflows

Edit configuration:

```text
read configuration reference -> edit -> run dry-run -> inspect diff/output -> execute if requested
```

Troubleshoot a run:

```text
confirm config and run ID -> inspect summary and journal -> inspect attributed logs -> reproduce with narrow scope
```

Export a runner configuration:

```text
check configuration -> export --check or dry-run -> write output -> inspect generated file
```

Work offline:

```text
add --offline -> identify unavailable remote sources -> run local validation or supported work -> report limits
```

## Guiding principles

- Prefer an explicit configuration path and narrow module/input scope.
- Preserve user-authored configuration and module ordering.
- Use previews before overwriting files or starting external work.
- Distinguish configuration failures, requirement failures, execution failures, and tool/runtime failures.
- Verify durable outcomes through summaries, journals, artifacts, and logs.
- Report skipped work and offline limitations.

## Common errors

- **Unknown configuration field or invalid value** — Check `docs/src/configuration.md`, `MODULE_SPEC.md`, and current validation code.
- **Unexpected configuration selected** — Pass the intended file explicitly and check discovery precedence.
- **Module requirement failure** — Inspect the named requirement and environment before considering troubleshooting overrides.
- **Acquisition or network failure** — Check offline mode, allowed hosts, credentials, source URL, and checksum policy.
- **No artifact where expected** — Resolve the configured artifact and report output paths; inspect the run summary and journal.
- **Export refuses to overwrite** — Compare with `--check`; use `--force` only when replacement is intended.
- **CLI flag rejected** — Read `xylem <command> --help`; the installed binary and checkout can differ.
