# Command-line reference

The `xylem` executable currently exposes the `run` command:

```text
xylem [OPTIONS] [COMMAND]
xylem [OPTIONS] run [RUN OPTIONS]
```

Global options include verbosity flags, `-t, --threads <N>`, `--env <PATH>`,
and `--no-progress`. The worker count defaults to 10, must be greater than
zero, and limits Xylem's Rayon work rather than threads created by child
programs. `--env` selects a specific dotenv file and fails if that file cannot
be loaded; existing process variables take precedence over values in the file.
Without `--env`, discovery of a default `.env` file remains optional.

A workflow may additionally set the positive `options.batch` value to cap
active top-level inputs. The effective input concurrency is the smaller of
`batch` and `--threads`; without `batch`, `--threads` remains the input limit.
Neither setting constrains threads or work created inside child programs.

Interactive terminals display aggregate module-preparation and workflow-
execution progress. Progress is hidden for redirected output and CI, and can
be disabled explicitly with `--no-progress`.

The `run` command accepts:

- `--config <PATH>` for a JSON or YAML configuration;
- `--input <LIST>` for comma-delimited workflow inputs;
- `--artifact-output <PATH>` to override the workflow artifact root and default
  `OUTPUT_DIRECTORY`;
- `--dry-run` to describe work without executing module commands;
- `--offline` to prohibit remote acquisition;
- `--resume` to skip modules with complete declared outputs and a matching marker;
- repeatable or comma-delimited `--allowed <HOST[:PORT]>` values to replace
  the workflow's configured remote-host allowlist;
- `--skip-verify-checksum` and `--skip-verify-requirements` for explicit
  troubleshooting bypasses;
- repeatable or comma-delimited `--reporter <console|json>` values; and
- `--report-output <PATH>` to select a directory for the journal and file-based
  reports.

`console` is the default reporter. `json` writes the versioned final summary
to standard output unless `--report-output` is present; with an output directory it
is atomically written as `<run-id>.json`. Repeated reporter names are
deduplicated while preserving their first occurrence.

Every valid run configuration gets an append-only
`<run-id>.journal.jsonl`. Xylem flushes each input and module transition before
continuing, so a run that is terminated without a finalization record remains
recoverable as `interrupted`. When `--report-output` is absent, the journal is kept in
the run's temporary workspace and the default console report prints its path.
When `--report-output` is present, both the durable journal and JSON report are kept
in that directory. The configuration `output` field continues to describe
workflow artifacts and does not override this CLI reporting destination. The
legacy `--output` spelling remains an alias for `--report-output`.

The version 2 JSON summary includes the run and configuration identities,
timestamps and duration, dry-run flag, final status, and ordered input/module
states. Diagnostics remain on standard error, so `--reporter json` can be
piped without tracing text corrupting the document. A required module failure
finalizes the journal as failed, stops admitting new inputs, leaves work that
was never admitted as `not_started`, and returns a non-zero status after the
selected reporters run.

Exit codes are selected at the CLI boundary. Unsupported command usage maps to
usage errors, malformed or semantically invalid configurations to data errors,
missing requirements and acquisition failures to unavailable-service errors,
child module failures to software errors, and journal/report persistence
failures to I/O or creation errors. Diagnostics retain the relevant
configuration path, module name, command, and working directory.

Dry-run is write-free. It renders a deterministic execution-plan document to
the console or JSON standard output without creating a workspace, journal,
report, marker, or artifact. Consequently, `--report-output` is rejected with
`--dry-run`; `--artifact-output` remains valid because it changes only the
in-memory plan.

When `--config` is omitted, Xylem looks for `config.json` in the current
directory. Run the generated help for version-specific spelling and defaults:

```shell
xylem --help
xylem run --help
```

The checked-in [`xylem --help`](./generated/xylem-help.txt) and
[`xylem run --help`](./generated/xylem-run-help.txt) snapshots are regenerated
with `make help` and compared with the compiled CLI in tests.
