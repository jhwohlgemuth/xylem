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

The v2 manifests contain five cases and a two-case concurrent holdout mix. They
promote the fifth case to training, move `wv03-large-mid-low` to the holdout
partition, and pair that holdout with `wv02-medium-mid-high`. Retain the v1
evidence directories and estimator identities unchanged. The base manifest
excludes Module O; use the separate A/P/O workflow below when owner-supplied
Earthdata credentials are available.

The accepted 2026-10-03 Oklahoma run, sanitized observations, generated
analysis, and operator scope are retained in
`evidence/oklahoma-2026-10-03/`. The GO decision applies only to the pinned
Module A plus Module P sequence and measured input-byte range.

### Stage the offline ASTER DEM cache

`real/aster-gdem-tiles.env` pins the GitLab Generic Package URL, archive
SHA-256, OCI tag, and OCI manifest digest for the five ASTER GDEM v3 tiles used
by the two measured holdouts. Store `GITLAB_TOKEN` in the ignored repository
root `.env`, then stage the verified tiles and the four input-specific mosaics:

```shell
tools/memory-spike/stage-aster-gdem.sh /absolute/module-a/data/DEM
```

Set `STAGE_HOST` to stage through SSH, for example:

```shell
STAGE_HOST=oklahoma tools/memory-spike/stage-aster-gdem.sh \
  /home/o9w/xylem-memory/geospatial-sandbox-gate/modules/module-a/data/DEM
```

Apply `module-a-offline-dem.patch` to the pinned Module A source used by the
gate. The patch resolves the cache relative to the copied module and returns a
prebuilt mosaic before making the CMR request, so preparation and execution can
remain network-denied. It also lets the existing default-AOT path handle absent
MODIS and VIIRS listings instead of passing a missing response to the parsers.

### Measure the pinned Module A, P, and O workflow

Store `EARTHDATA_API_TOKEN` in the ignored repository-root `.env`, then run:

```shell
tools/memory-spike/docker-measure-module-o.sh
```

The wrapper forwards only `EARTHDATA_API_TOKEN` to the measurement container,
uses `real/manifest-module-o.json`, and writes evidence under
`/home/o9w/xylem-memory/results-module-o`. Module O runs through a measurement
wrapper that fails unless every non-empty Module P image produces a non-empty
orthorectified image. This closes the pinned Module O revision's worker-failure
reporting gap for the spike without changing that external repository.

The accepted 2026-10-06 Oklahoma A/P/O run, sanitized observations, generated
analysis, and operator scope are retained in
`evidence/oklahoma-2026-10-06-module-o/`. Its independent GO decision applies
only to the pinned Module A, Module P, and Module O sequence, one active input,
and the measured 529,854,516–1,143,813,163 input-byte range.

### Run the expanded A/P/O gate manually on Oklahoma

The expanded v2 gate takes approximately seven hours and needs the
owner-supplied Earthdata token. Log in to Oklahoma, update its existing Xylem
checkout, and store `EARTHDATA_API_TOKEN` in the ignored repository-root `.env`
with mode `0600`. Do not put the token directly on the command line.

Run the gate through Oklahoma's rootful Docker context. The wrapper requires at
least 80 GiB of available memory, gives the measurement container a 96 GiB
limit, selects `manifest-module-o.json`, and creates a timestamped results
directory without overwriting prior evidence:

```shell
cd /home/o9w/dev/re/xylem
nohup tools/memory-spike/run-expanded-module-o-oklahoma.sh \
  > /home/o9w/xylem-memory/expanded-module-o.log 2>&1 < /dev/null &
printf 'measurement pid: %s\n' "$!"
```

Follow progress from another SSH session:

```shell
tail -f /home/o9w/xylem-memory/expanded-module-o.log
docker --context default ps --filter name=xylem-memory-measure
```

The log prints the unique results directory near startup. After completion,
inspect that directory's `report.md` and summarize the machine-readable
decision without exposing the token:

```shell
jq '{decision, estimators: [.estimators[] | {decision, key, selected_model}]}' \
  /home/o9w/xylem-memory/results-module-o-v2-*/analysis.json
```

Do not enable A/P/O input concurrency unless the fresh individual and
concurrent holdout gates are both GO.

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
