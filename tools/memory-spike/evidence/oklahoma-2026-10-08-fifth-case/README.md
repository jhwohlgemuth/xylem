# Oklahoma fifth-case Module A + P evidence

This directory preserves the candidate real-corpus reevaluation started on
2026-10-08. Its generated decision is **NO-GO**. It does not replace the
accepted 2026-10-03 evidence or change the `module-a-p-v1` estimator identity.

## Corpus coverage

Each individual case received one unrecorded warm-up followed by five recorded
runs. The concurrent holdout mix received one warm-up followed by three
recorded runs.

| Subject | Partition | Files | Input bytes | Decoded raster bytes | Peak range |
|---|---|---:|---:|---:|---:|
| `ge01-medium-low-mid` | Training | 47 | 529,854,516 | 2,210,398,208 | 17.867–17.867 GiB |
| `wv02-medium-mid-high` | Holdout | 48 | 678,219,617 | 2,805,989,376 | 22.103–22.105 GiB |
| `wv03-large-high-mid` | Training | 47 | 1,037,649,019 | 4,357,881,856 | 33.786–33.789 GiB |
| `wv03-large-mid-low` | Training | 47 | 1,143,813,163 | 4,357,881,856 | 33.787–33.787 GiB |
| `wv02-large-low-mid` | Holdout | 139 | 2,588,922,236 | 11,666,456,576 | 63.996–64.000 GiB |
| `wv02-concurrent-holdout` | Holdout mix | 187 | 3,267,141,853 | 14,472,445,952 | 63.998–63.998 GiB |

The fifth case was materialized from revision
`fc4cd2647bdc8ca9c561c44bb038ab71bb9c1cb7` as a clean Git-free tree. Its
aggregate SHA-256 is
`cf899764c2243b2539d7878ebef5f368d8b1c34fc298286433e0cb92ce0c8f1f`.
The other four revisions remain those recorded by the accepted 2026-10-03
evidence. The mix executes the two WV02 holdouts in one Xylem invocation with
distinct generated output directories.

## Measurement boundary and noise

The run used Oklahoma's amd64 Ubuntu host, Linux 6.8.0-138-generic, Xylem
1.0.3, and cgroup v2. The privileged measurement container delegated
`/sys/fs/cgroup` and created one exact process-tree cgroup at
`/sys/fs/cgroup/xylem-memory-1-<subject>-<run>` per invocation. The decision
metric was that cgroup's `memory.peak`; every recorded command succeeded and
reported zero OOM and OOM-kill events.

Across the five individual repetitions, non-baseline peak ranges were 442,368
bytes for GE01, 1,531,904 bytes for the medium WV02 holdout, 3,194,880 bytes
for WV03 high/mid, 548,864 bytes for WV03 mid/low, and 4,321,280 bytes for the
new large WV02 holdout. The three mix peaks spanned 688,128 bytes.

## Decision and operating scope

No candidate model passed. The input-byte candidate used an intercept of
3,753,780,717.734 bytes, a slope of 29.681 bytes per input byte, and a 1.10
safety multiplier. It covered every individual case, but the new case was one
extrapolation beyond the 1,143,813,163-byte training maximum and the concurrent
mix was underpredicted. The decoded-byte and size-bucket candidates failed the
same two gates. The fixed worst-case reservation was 68,717,719,552 bytes.

Keep production input concurrency at one and retain the accepted v1 estimator
only within its existing single-input calibration range. Before enabling
weighted multi-input admission, collect representative training cases across
the larger range and produce a new estimator identity that passes both the
individual and concurrent holdout gates.

## Integrity

The artifacts contain sanitized identities and aggregate metrics only. They do
not contain corpus paths, repository credentials, or image metadata filenames.

| Artifact | SHA-256 |
|---|---|
| `observations.jsonl` | `eeb133993c191ed2e1cd3b448c8fda3c35ada7346eaafd80f5884c71fd1f909e` |
| `analysis.json` | `39350ed95d72dd92f6309facc6147080a7f0335026a4465855f8605d3254745f` |
| `report.md` | `3b3934cf273bd3b501d2c1c70f368eb1059cb4ed5e7df3afb236a0a0655c4e8c` |
