# Command-line reference

The `xylem` executable currently exposes the `run` command:

```text
xylem [OPTIONS] [COMMAND]
xylem [OPTIONS] run [RUN OPTIONS]
```

Global options include verbosity flags and `-t, --threads <N>`. The worker
count defaults to 10, must be greater than zero, and limits Xylem's Rayon work
rather than threads created by child programs.

The `run` command accepts:

- `--config <PATH>` for a JSON or YAML configuration;
- `--input <LIST>` for comma-delimited workflow inputs;
- `--dry-run` to describe work without executing module commands;
- `--offline` to prohibit remote acquisition;
- `--skip-verify-checksum` and `--skip-verify-requirements` for explicit
  troubleshooting bypasses; and
- reporter and output options reserved for structured result presentation.

When `--config` is omitted, Xylem looks for `config.json` in the current
directory. Run the generated help for version-specific spelling and defaults:

```shell
xylem --help
xylem run --help
```
