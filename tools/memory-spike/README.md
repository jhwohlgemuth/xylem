# Memory-aware scheduling spike

This non-publishable tool measures Xylem workloads without changing Xylem's
runtime, configuration, execution-plan, or journal contracts. Synthetic cases
validate the harness; only owner-approved real training and holdout cases can
support a go decision.

## Manifest

Copy `manifest.example.json` and replace its opaque case IDs, module identities,
configuration paths, input paths, and pre-admission features. Raster dimensions,
bands, bit depth, format, and compression are recorded explicitly; the declared
decoded byte total must match checked arithmetic over those raster records.
Paths are resolved relative to the manifest. Every estimator needs one input-free baseline case,
at least three real training cases, and at least one real holdout case. Holdout
mixes must reference cases that share an estimator and configuration.

Measurement configurations must be JSON and must omit `options.batch`; the tool
sets `--threads` to one for individual observations and to the mix width for
mixed observations. Input paths containing commas are rejected because the
current CLI represents multiple override inputs as a comma-delimited value.

## Measure on Linux

Build Xylem and the spike tool, ensure GNU `sha256sum` is available, then
provide a cgroup-v2 subtree delegated to
the current user. The tool creates and removes only uniquely named child
cgroups below that path and refuses to run when the memory controller or
`memory.peak` is unavailable.

Validate whole-process-tree accounting before measuring the real corpus:

```shell
cargo run --locked --package xylem-memory-spike --bin xylem-memory-spike -- \
  probe \
  --cgroup-root /path/to/delegated/cgroup \
  --probe target/debug/xylem-memory-probe \
  --parent-mib 32 \
  --child-mib 32 \
  --hold-millis 500 \
  --json /path/to/probe.json
```

The probe fails unless `memory.peak` includes at least the 64 MiB requested
across the overlapping parent and child allocators.

```shell
cargo build --locked --package xylem-cli --package xylem-memory-spike
cargo run --locked --package xylem-memory-spike --bin xylem-memory-spike -- \
  measure \
  --manifest tools/memory-spike/manifest.json \
  --cgroup-root /path/to/delegated/cgroup \
  --xylem target/debug/xylem \
  --work-root /path/to/empty/work-root \
  --observations /path/to/observations.jsonl
```

Each case receives one warm-up and five recorded runs. Each mix receives one
warm-up and three recorded runs. Every run uses fresh artifact, report, and
cgroup directories. The exact `memory.peak` value is authoritative; sampled
current, anonymous, and file-backed values explain its composition.

Use `xylem-memory-probe PARENT_MIB CHILD_MIB HOLD_MILLIS` as a synthetic module
command to confirm that overlapping parent and child allocations are both
charged to the measurement cgroup.

## Analyze

```shell
cargo run --locked --package xylem-memory-spike --bin xylem-memory-spike -- \
  analyze \
  --manifest tools/memory-spike/manifest.json \
  --observations /path/to/observations.jsonl \
  --json /path/to/analysis.json \
  --markdown /path/to/report.md
```

Analysis uses the maximum successful peak for each subject, subtracts the
workload baseline, and fits only real training cases. A candidate is accepted
only when it does not extrapolate, underpredict any held-out case or mix, and
reduces median held-out reservation by at least 20% versus the observed fixed
worst case. The JSON artifact retains coefficients, training feature ranges,
bucket peaks, the safety multiplier, mean absolute error, maximum
underprediction, and p95 overreservation. Output size and synthetic
observations never enter the decision.
