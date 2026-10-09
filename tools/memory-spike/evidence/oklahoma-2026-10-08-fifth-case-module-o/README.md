# Oklahoma fifth-case Module A + P + O evidence

This directory preserves the candidate real-corpus reevaluation started on
2026-10-08. Its generated decision is **NO-GO**. It does not replace the
accepted 2026-10-06 evidence or change the `module-a-p-o-v1` estimator
identity.

## Corpus coverage

Each individual case received one unrecorded warm-up followed by five recorded
runs. The concurrent holdout mix received one warm-up followed by three
recorded runs.

| Subject | Partition | Files | Input bytes | Decoded raster bytes | Peak range |
|---|---|---:|---:|---:|---:|
| `ge01-medium-low-mid` | Training | 47 | 529,854,516 | 2,210,398,208 | 17.867–17.867 GiB |
| `wv02-medium-mid-high` | Holdout | 48 | 678,219,617 | 2,805,989,376 | 22.104–22.105 GiB |
| `wv03-large-high-mid` | Training | 47 | 1,037,649,019 | 4,357,881,856 | 33.786–33.788 GiB |
| `wv03-large-mid-low` | Training | 47 | 1,143,813,163 | 4,357,881,856 | 33.787–33.867 GiB |
| `wv02-large-low-mid` | Holdout | 139 | 2,588,922,236 | 11,666,456,576 | 63.997–63.998 GiB |
| `wv02-concurrent-holdout` | Holdout mix | 187 | 3,267,141,853 | 14,472,445,952 | 64.000–64.001 GiB |

The fifth case was materialized from revision
`fc4cd2647bdc8ca9c561c44bb038ab71bb9c1cb7` as a clean Git-free tree. Its
aggregate SHA-256 is
`cf899764c2243b2539d7878ebef5f368d8b1c34fc298286433e0cb92ce0c8f1f`.
The other four revisions remain those recorded by the accepted 2026-10-06
evidence. The mix executes the two WV02 holdouts in one Xylem invocation with
distinct generated output directories.

## Measurement boundary and noise

The run used Oklahoma's amd64 Ubuntu host, Linux 6.8.0-138-generic, Xylem
1.0.3, and cgroup v2. The privileged measurement container delegated
`/sys/fs/cgroup` and created one exact process-tree cgroup at
`/sys/fs/cgroup/xylem-memory-1-<subject>-<run>` per invocation. The decision
metric was that cgroup's `memory.peak`; every recorded command succeeded and
reported zero OOM and OOM-kill events.

Across the five individual repetitions, non-baseline peak ranges were 806,912
bytes for GE01, 1,736,704 bytes for the medium WV02 holdout, 1,478,656 bytes
for WV03 high/mid, 86,241,280 bytes for WV03 mid/low, and 2,068,480 bytes for
the new large WV02 holdout. The three mix peaks spanned 1,679,360 bytes.

Module O revision `404ab3bccb64e0c7823f5eb577cc491f695bbf2b` ran through
the existing verification wrapper, which requires each non-empty Module P
raster to produce a non-empty Module O raster. The Earthdata token was passed
only in the measurement environment and is absent from these artifacts. The
SSH stream disconnected after all 33 observations were durable but before
analysis; the offline analyzer was rerun against the unchanged observation
file after reconnecting.

## Decision and operating scope

No candidate model passed. The input-byte candidate used an intercept of
3,697,283,379.507 bytes, a slope of 29.775 bytes per input byte, and a 1.10
safety multiplier. It covered every individual case, but the new case was one
extrapolation beyond the 1,143,813,163-byte training maximum and the concurrent
mix was underpredicted. The decoded-byte and size-bucket candidates failed the
same two gates. The fixed worst-case reservation was 68,715,487,232 bytes.

Keep production input concurrency at one and retain the accepted v1 estimator
only within its existing single-input calibration range. Before enabling
weighted multi-input admission, collect representative training cases across
the larger range and produce a new estimator identity that passes both the
individual and concurrent holdout gates.

## Integrity

The artifacts contain sanitized identities and aggregate metrics only. They do
not contain corpus paths, repository credentials, image metadata filenames, or
the Earthdata token.

| Artifact | SHA-256 |
|---|---|
| `observations.jsonl` | `84b3ae56ec3b8b3b4055c0eb1c0860c20c2844b6f634a4ca96b38e1e42565e78` |
| `analysis.json` | `25aa05752927de117a7ae6892850f7ac58ac539881e69c2a4a683cb09f13b69c` |
| `report.md` | `e802c2f3652e2b1059d5d7fb03b496c536dff8690ec901fe74eb096353c9d5c0` |
