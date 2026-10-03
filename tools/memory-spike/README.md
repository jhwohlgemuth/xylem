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

### Verify an Ubuntu Docker host

The tracked Docker probe validates cgroup-v2 process-tree accounting before a
real corpus is staged. It uses the `vergil` Docker context by default; the
context must already exist, and its SSH endpoint must resolve from the client.

```shell
tools/memory-spike/docker-probe.sh > /tmp/xylem-memory-probe.json
jq -e '.peak_bytes >= .expected_minimum_bytes and .expected_minimum_bytes == 67108864' \
  /tmp/xylem-memory-probe.json
```

Set `DOCKER_CONTEXT` to target another arm64 or amd64 Ubuntu Docker host. The
script selects the native image platform, builds the probe on that host, and
emits only the resulting JSON on standard output. Build and preflight progress
is written to standard error.

The probe container is privileged because it must create nested cgroups. It
uses a private cgroup namespace, disables networking, mounts no host paths,
uses a read-only root filesystem, limits memory and process count, and is
removed after the run. A successful synthetic probe validates the measurement
harness only; it does not support a production estimator decision.

### Measure the pinned imagery corpus on Oklahoma

`real/manifest.json` records the four current projects in the private
`GSED/common/imagery-processing/test-cases` group and pins the Module A and
Module P revisions used by the workflow. The corpus must be materialized
without Git metadata or `expected_a.tiff` under
`/home/o9w/xylem-memory/corpus`, and the pinned module checkouts must be under
`/home/o9w/xylem-memory/modules`. An empty `modules/data` directory is the
mountpoint for the shared DEM and atmospheric-data cache.

```shell
tools/memory-spike/docker-measure.sh
```

The script builds the amd64/arm64 measurement image on the selected Ubuntu
Docker context, uses `oklahoma` by default, and retains outputs and evidence in
`/home/o9w/xylem-memory/results` on the host. Override `DOCKER_CONTEXT`,
`WORKLOAD_ROOT`, `CACHE_VOLUME`, or `MEMORY_LIMIT` when needed. It refuses to
overwrite a results directory containing `observations.jsonl`. A host bind is
used so automated Docker image and volume cleanup cannot discard the evidence.

Only four approved cases currently exist, so the manifest provides the minimum
three training cases and one holdout case but cannot define a two-case holdout
mix. Module O is excluded from this run because its DEM acquisition requires
owner-supplied Earthdata/ASTER credentials. Its current process-pool boundary
also fails to propagate worker errors, so a missing DEM can otherwise appear
as a successful module command without an orthorectified raster.

The accepted 2026-10-03 Oklahoma run, sanitized observations, generated
analysis, and operator scope are retained in
`evidence/oklahoma-2026-10-03/`. The GO decision applies only to the pinned
Module A plus Module P sequence and measured input-byte range.

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
