# Oklahoma Module A + P v2 concurrency evidence

This directory preserves the real-corpus `module-a-p-v2` measurement completed
on 2026-10-09. Its generated decision is **GO**. It does not modify the
accepted `module-a-p-v1` model in place.

## Corpus coverage

Each individual case received one unrecorded warm-up followed by five recorded
runs. The concurrent holdout mix received one warm-up followed by three
recorded runs.

| Subject | Partition | Files | Input bytes | Decoded raster bytes | Peak range |
|---|---|---:|---:|---:|---:|
| `ge01-medium-low-mid` | Training | 47 | 529,854,516 | 2,210,398,208 | 17.867–17.867 GiB |
| `wv02-medium-mid-high` | Holdout | 48 | 678,219,617 | 2,805,989,376 | 22.102–22.105 GiB |
| `wv03-large-high-mid` | Training | 47 | 1,037,649,019 | 4,357,881,856 | 33.786–33.788 GiB |
| `wv03-large-mid-low` | Holdout | 47 | 1,143,813,163 | 4,357,881,856 | 33.786–33.788 GiB |
| `wv02-large-low-mid` | Training | 139 | 2,588,922,236 | 11,666,456,576 | 63.997–64.001 GiB |
| `wv02-wv03-concurrent-holdout` | Holdout mix | 95 | 1,822,032,780 | 7,163,871,232 | 48.624–49.214 GiB |

The fifth case was materialized from revision
`fc4cd2647bdc8ca9c561c44bb038ab71bb9c1cb7` as a clean Git-free tree. Its
aggregate SHA-256 is
`cf899764c2243b2539d7878ebef5f368d8b1c34fc298286433e0cb92ce0c8f1f`.
The mix executes the medium WV02 and WV03 mid/low holdouts in one Xylem
invocation with distinct generated output directories.

## Measurement boundary and noise

The run used Oklahoma's amd64 Ubuntu host, Linux 6.8.0-138-generic, Xylem
1.0.3, and cgroup v2. The privileged measurement container delegated
`/sys/fs/cgroup` and created one exact process-tree cgroup at
`/sys/fs/cgroup/xylem-memory-1-<subject>-<run>` per invocation. The decision
metric was that cgroup's `memory.peak`; every recorded command succeeded and
reported zero OOM and OOM-kill events.

The five-run individual peak spans were 655,360 bytes for GE01, 2,871,296
bytes for the medium WV02 holdout, 2,707,456 bytes for WV03 high/mid,
2,613,248 bytes for WV03 mid/low, and 4,169,728 bytes for the large WV02
training case. The three mix peaks spanned 633,524,224 bytes.

## Decision and operating scope

Both the decoded-byte and input-byte candidates passed every gate. The analyzer
selected decoded bytes by mean absolute error, but Xylem's portable runtime
feature is logical input bytes. The accepted runtime-compatible candidate uses
an intercept of 9,083,341,409.049 bytes, a slope of 23.320130499 memory bytes
per input byte, and a 1.15 safety multiplier over the
529,854,516–2,588,922,236-byte range. It produced zero individual or mix
underpredictions, zero extrapolations, and a 49.2% median reservation reduction
versus the 68,718,493,696-byte fixed worst case.

Production fixed-point coefficients round conservatively to a
9,083,341,410-byte intercept and a `23320131` millionths slope. The medium WV02
and WV03 mid/low holdouts reserve 28,634,438,485 and 41,120,796,343 bytes. Their
69,755,234,828-byte sum fits a 72 GiB budget, while the largest measured
concurrent peak was 52,842,696,704 bytes. This supports `options.batch: 2` for
the pinned, ordered Module A plus Module P workflow on hosts where operators
can dedicate that budget. Inputs outside the calibrated range retain the
fixed-batching fallback.

This decision does not cover Module A + P + O, other revisions or module
orders, or unmeasured input classes. Those require independent evidence and
estimator identities.

## Integrity

The artifacts contain sanitized identities, container-local paths, and
aggregate metrics only. A scan found no credentials, authorization headers,
tokens, passwords, secrets, or private keys.

| Artifact | SHA-256 |
|---|---|
| `observations.jsonl` | `5de353fbd52b8c907bc1234950b1d77cb7bba3d4d197a828ace6455bf62b2e99` |
| `analysis.json` | `da241a235a8ccd660163469aaff391794878d80927b0eb041026d9fb99e57b97` |
| `report.md` | `2391f28500b0a426a6b4f1566dce430b308775860abc5a8a0aa50dcd151b44c8` |
