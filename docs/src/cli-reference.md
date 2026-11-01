# Command-line reference

The `xylem` executable exposes headless `run` and `export` commands and an interactive `tui` command:

```text
xylem [OPTIONS] [COMMAND]
xylem [OPTIONS] run [CONFIG] [RUN OPTIONS]
xylem [OPTIONS] export --gitlab [CONFIG] [EXPORT OPTIONS]
xylem [OPTIONS] tui [CONFIG]
```

Global options include verbosity flags, `-t, --threads <N>`, `--env <PATH>`,
`--sandbox-policy <PATH>`, and `--no-progress`. The worker count defaults to 10, must be greater than
zero, and limits Xylem's Rayon work rather than threads created by child
programs. `--env` selects a specific dotenv file and fails if that file cannot
be loaded; existing process variables take precedence over values in the file.
Without `--env`, discovery of a default `.env` file remains optional.
`--sandbox-policy` is read only when the workflow explicitly sets
`sandbox: true`; a missing required policy or an unused policy is an error.

A workflow may additionally set the positive `options.batch` value to cap
active top-level inputs. The effective input concurrency is the smaller of
`batch` and `--threads`; without `batch`, `--threads` remains the input limit.
Neither setting constrains threads or work created inside child programs.

Interactive terminals display aggregate module-preparation and workflow-
execution progress. Progress is hidden for redirected output and CI, and can
be disabled explicitly with `--no-progress`.

The `run` command accepts:

- an optional positional `CONFIG` path using JSON, strict JSONC, YAML, or YML;
- `--input <LIST>` for comma-delimited workflow inputs;
- `--module-index <N>` to run one zero-based module from the resolved workflow;
- `--artifact-output <PATH>` to override the workflow artifact root and default
  `OUTPUT_DIRECTORY`;
- `--dry-run` to describe work without executing module commands;
- `--failure-policy <fail-fast|continue>` to replace the configured input
  failure policy for this run;
- `--gpu-monitor` to collect run-wide NVIDIA GPU utilization and memory metrics;
- `--offline` to prohibit remote module and sandbox-workload acquisition; the
  digest-pinned workload must already be available locally;
- `--resume` to skip modules with complete declared outputs and a matching marker;
- `--retain-workspace` to keep the disposable run workspace and print its path;
- repeatable or comma-delimited `--allowed <HOST[:PORT]>` values to replace
  the workflow's configured remote-host allowlist;
- `--allow-insecure-http` to permit cleartext HTTP for local development;
- `--skip-verify-checksum` as a conspicuous emergency escape hatch for remote
  binaries; it emits a warning and records the observed digest in provenance;
- `--skip-verify-requirements` for an explicit requirements troubleshooting
  bypass;
- repeatable or comma-delimited `--reporter <console|json>` values; and
- `--report-output <PATH>` to export additional file-based reports.

`console` is the default reporter. `json` writes the versioned final summary
to standard output unless `--report-output` is present; with an output directory it
is atomically written as `<run-id>.json`. Repeated reporter names are
deduplicated while preserving their first occurrence.

By default, every valid run gets a canonical bundle beneath Xylem's platform cache with a
manifest, append-only journal, one-second metrics, terminal summary, and
attributed task logs. Xylem flushes transitions and JSONL evidence while work
runs, so a run terminated without a summary remains recoverable as
`interrupted`. Tests and embedded deployments can set `XYLEM_RUN_STORE` to an
isolated store root. `--report-output` exports additional requested reports without
relocating canonical evidence. The configuration `output` field continues to
describe workflow artifacts. The legacy `--output` spelling remains an alias
for `--report-output`.

Real runs also use `${TMPDIR}/xylem/workspaces/<run-id>` for copied modules,
input-local environments, and scratch data. Xylem removes this disposable tree
after success or failure. `--retain-workspace` transfers cleanup responsibility
to the operator and prints the retained path; canonical journals and summaries
remain outside that tree.

The version 14 JSON summary includes the run and configuration identities,
timestamps and duration, dry-run flag, final status, and ordered input/module
states, verified configuration and module acquisition provenance, plus explicit
`sandbox` evidence (`mode: off` for compatibility runs).
It also records the effective failure policy, stable input-context and
run-local workspace identities, configured module timeouts, and the reason a
run stopped admitting work.
Required sandbox runs include the effective retention and stale-resource
policy, exactly-once terminal resource outcomes, and any expired resources
reconciled before the session opened. Retained resource identifiers remain in
the summary for operator recovery.
When GPU monitoring is enabled, the journal retains one raw sample per
second and the summary includes per-device sample counts, average and peak
utilization, and average and peak frame-buffer memory usage. A missing or
invalid initial `nvidia-smi` query prevents module execution; a later sampling
failure marks monitoring as degraded without changing an otherwise successful
workflow result. Diagnostics remain on standard error, so `--reporter json` can be
piped without tracing text corrupting the document. A required module failure
finalizes the journal as failed, stops admitting new inputs, leaves work that
was never admitted as `not_started`, and returns a non-zero status after the
selected reporters run.

When `--failure-policy` is omitted, `options.failure_policy` remains effective;
when both are omitted, it resolves to `fail-fast`. The CLI accepts and previews
`continue`, which keeps admitting eligible inputs after failures. Fail-fast
stops new admission after the first failure and cancels active siblings while
preserving the triggering failure.

Exit codes are selected at the CLI boundary. Unsupported command usage maps to
usage errors, malformed or semantically invalid configurations to data errors,
missing requirements and acquisition failures to unavailable-service errors,
child module failures to software errors, and journal/report persistence
failures to I/O or creation errors. Diagnostics retain the relevant
configuration path, module name, command, and working directory.

Configured memory-aware runs reject any per-input reservation larger than the
run budget as a data error before module work begins. If any input cannot be
sized safely or falls outside the estimator's calibrated range, the run emits
one warning and uses fixed input batching for the whole run.

Dry-run is write-free. It renders a deterministic execution-plan document to
the console or JSON standard output without creating a workspace, journal,
report, marker, or artifact. Consequently, `--report-output` is rejected with
`--dry-run`; `--artifact-output` remains valid because it changes only the
in-memory plan. `--gpu-monitor` is likewise represented in the plan without
probing the host.

`xylem export --gitlab [CONFIG]` writes `.gitlab-ci.yml` for a local workflow.
The generated pipeline runs from GitLab's **Run pipeline** action and assigns
each resolved executable module to its own ordered stage. Jobs pass the
configured output directory forward as an artifact and run that module through
`xylem run --module-index`. The first job checks that the committed GitLab file
still matches the source configuration. Use `--output <PATH>` to choose another
GitLab file, `--image <IMAGE>` to choose the job image, `--force` to replace an
existing file, or `--check` to compare the existing file without changing it.
The image must provide Cargo, curl, and SHA-256 tooling on Linux x86_64;
generated jobs install the corresponding published Xylem release and any
declared Conda or Pixi tools. Modules that share files must use the configured
output directory for those files.

GitLab export accepts local binary and script modules, including leaves in
local nested configurations, and explicit input lists. All source modules and
the output directory must be inside the repository. Remote modules, sandboxed
workflows, and dynamic input selection are not supported by this exporter.
Commit the generated file along with the workflow configuration and modules.

When `CONFIG` is omitted, `run` and `export` check `.xylem.json`,
`.xylem.jsonc`, `.xylem.yaml`, and `.xylem.yml` in that order. They report an
error when none exists; `tui` still opens retained history. Run the generated help for
version-specific spelling and defaults:

```shell
xylem --help
xylem export --help
xylem run --help
xylem tui --help
```

The checked-in [`xylem --help`](./generated/xylem-help.txt),
[`xylem export --help`](./generated/xylem-export-help.txt), and
[`xylem run --help`](./generated/xylem-run-help.txt) snapshots are regenerated
with `make help` and compared with the compiled CLI in tests.
