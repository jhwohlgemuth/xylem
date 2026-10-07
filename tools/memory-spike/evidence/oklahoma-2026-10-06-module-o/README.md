# Oklahoma Module A + P + O measurement evidence

This directory preserves the accepted real-corpus run performed on 2026-10-06.
The generated report's decision is **GO** for the pinned Module A, Module P,
and Module O workflow. It is independent of the earlier Module A plus Module P
estimator and is not evidence for other module revisions or combinations.

## Corpus coverage

Each case received one unrecorded warm-up followed by five recorded runs.

| Case | Partition | Files | Input bytes | Decoded raster bytes | Peak range |
|---|---|---:|---:|---:|---:|
| `ge01-medium-low-mid` | Training | 47 | 529,854,516 | 2,210,398,208 | 17.866–17.867 GiB |
| `wv02-medium-mid-high` | Holdout | 48 | 678,219,617 | 2,805,989,376 | 22.104–22.106 GiB |
| `wv03-large-high-mid` | Training | 47 | 1,037,649,019 | 4,357,881,856 | 33.787–33.787 GiB |
| `wv03-large-mid-low` | Training | 47 | 1,143,813,163 | 4,357,881,856 | 33.786–33.868 GiB |

The Git-free corpus was materialized from test-case revisions
`771347a9476c5e6147ca8532295026a1a97cba70` (GE01),
`80c43ece048cb996987792e02c9320240ac6df2a` (WV02),
`a801775b9ce3cd7b4285400b2b6ebbadcdc76bd8` (WV03 high/mid), and
`fa6bd573de90292db3dd1c425db467f18011a70b` (WV03 mid/low).

Only four approved test-case projects existed at measurement time. The corpus
therefore satisfies the minimum three-training/one-holdout gate but does not
contain the two distinct compatible holdout cases required for a concurrent
holdout mix.

## Measurement boundary and noise

The run used Oklahoma's Ubuntu 22.04.5 LTS amd64 Docker host, Linux
6.8.0-138-generic, Xylem 1.0.3, and cgroup v2. The privileged measurement
image ID was
`sha256:18adc8fd3784603e18156d835dc2a9ffb744b8881b92463279cb9d0d3ed436be`.
The container used a private cgroup namespace, moved its control process to
`/sys/fs/cgroup/xylem-memory-control`, delegated `/sys/fs/cgroup`, and created
one `/sys/fs/cgroup/xylem-memory-1-<subject>-<run>` sibling per invocation.
Exact process-tree `memory.peak` was the decision metric.

Across the five recorded repetitions, non-baseline peak ranges were 868,352
bytes (0.005%) for GE01, 2,023,424 bytes (0.009%) for WV02, 544,768 bytes
(0.002%) for WV03 high/mid, and 88,043,520 bytes (0.242%) for WV03 mid/low.

Module O revision `404ab3bccb64e0c7823f5eb577cc491f695bbf2b` does not
propagate process-pool worker failures reliably. For this measurement it ran
through a wrapper that required every non-empty Module P raster to produce a
non-empty Module O raster. The Earthdata token was supplied only through the
container environment and is not retained in these artifacts.

## Decision and operating scope

The selected input-byte model has an intercept of 3,696,455,730.646 bytes, a
slope of 29.776 bytes per input byte, and a 1.10 safety multiplier. It produced
zero training or held-out underpredictions, no extrapolations, and a 27.7%
median reservation reduction against the 36,363,116,544-byte fixed worst case.

Proceed with production scheduler work using this estimator only when all of
the following match:

- Module A revision `2396c36644a89170bde2da2f29a65a364b21338c`, followed by
  Module P revision `a80b166736e8174171e398653fda925d3c124fae`, followed by
  Module O revision `404ab3bccb64e0c7823f5eb577cc491f695bbf2b`;
- aggregate input size between 529,854,516 and 1,143,813,163 bytes;
- one active input; and
- Linux cgroup v2 telemetry available for observed-versus-estimated validation.

Use fixed batching outside that scope. Re-measure after module revisions,
material workflow changes, or adding representative cases. Add a concurrent
holdout mix before validating weighted multi-input admission.

## Integrity

The observation file contains opaque case identities, module/config checksums,
host characteristics, and aggregate memory metrics. It contains no corpus
paths, repository credentials, image metadata filenames, or Earthdata token.

| Artifact | SHA-256 |
|---|---|
| `observations.jsonl` | `3ab9f0048c25dd399a502ac309f7d1b9effa65bcd91714d1ead08a341b789939` |
| `analysis.json` | `fcb7a3c75207130c65933a42003b2e116e6fb034d5441c89f1068f04a2eb51ed` |
| `report.md` | `92cb77f9028a7204f63e7e1c5ee0eb7302c9048271ead9b687e882449a1cde20` |
