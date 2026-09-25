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
- `--dry-run` to describe work without executing module commands;
- `--offline` to prohibit remote acquisition;
- repeatable or comma-delimited `--allowed <HOST[:PORT]>` values to replace
  the workflow's configured remote-host allowlist;
- `--skip-verify-checksum` and `--skip-verify-requirements` for explicit
  troubleshooting bypasses;
- repeatable or comma-delimited `--reporter <console|json>` values; and
- `--output <PATH>` to select a directory for the journal and file-based
  reports.

`console` is the default reporter. `json` writes the versioned final summary
to standard output unless `--output` is present; with an output directory it
is atomically written as `<run-id>.json`. Repeated reporter names are
deduplicated while preserving their first occurrence.

Every valid run configuration gets an append-only
`<run-id>.journal.jsonl`. Xylem flushes each input and module transition before
continuing, so a run that is terminated without a finalization record remains
recoverable as `interrupted`. When `--output` is absent, the journal is kept in
the run's temporary workspace and the default console report prints its path.
When `--output` is present, both the durable journal and JSON report are kept
in that directory. The configuration `output` field continues to describe
workflow artifacts and does not override this CLI reporting destination.

The version 1 JSON summary includes the run and configuration identities,
timestamps and duration, dry-run flag, final status, and ordered input/module
states. Diagnostics remain on standard error, so `--reporter json` can be
piped without tracing text corrupting the document. A required module failure
finalizes the journal as failed, stops admitting new inputs, leaves work that
was never admitted as `not_started`, and returns a non-zero status after the
selected reporters run.

When `--config` is omitted, Xylem looks for `config.json` in the current
directory. Run the generated help for version-specific spelling and defaults:

```shell
xylem --help
xylem run --help
```
