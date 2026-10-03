# Oklahoma Module A + P measurement evidence

This directory preserves the accepted real-corpus run performed on 2026-10-03.
The generated report's decision is **GO** for the pinned Module A followed by
Module P workflow on inputs within the measured range. It is not evidence for
Module O or for other module revisions and combinations.

## Corpus coverage

Each case received one unrecorded warm-up followed by five recorded runs.

| Case | Partition | Files | Input bytes | Decoded raster bytes | Peak range |
|---|---|---:|---:|---:|---:|
| `ge01-medium-low-mid` | Training | 47 | 529,854,516 | 2,210,398,208 | 17.865–17.866 GiB |
| `wv02-medium-mid-high` | Holdout | 48 | 678,219,617 | 2,805,989,376 | 22.101–22.104 GiB |
| `wv03-large-high-mid` | Training | 47 | 1,037,649,019 | 4,357,881,856 | 33.784–33.786 GiB |
| `wv03-large-mid-low` | Training | 47 | 1,143,813,163 | 4,357,881,856 | 33.784–33.786 GiB |

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
`sha256:90c87b7365012d4bcc7e57ed93a83240c2df98b0a4bb40f18b2382bcd7b90db2`.
The container used a private cgroup namespace, moved its control process to
`/sys/fs/cgroup/xylem-memory-control`, delegated `/sys/fs/cgroup`, and created
one `/sys/fs/cgroup/xylem-memory-1-<subject>-<run>` sibling per invocation.
Exact process-tree `memory.peak` was the decision metric.

Across the five recorded repetitions, non-baseline peak ranges were 815,104
bytes (0.004%) for GE01, 2,838,528 bytes (0.012%) for WV02, 2,510,848 bytes
(0.007%) for WV03 high/mid, and 1,724,416 bytes (0.005%) for WV03 mid/low.

## Decision and operating scope

The selected input-byte model has an intercept of 3,754,352,537.535 bytes, a
slope of 29.680 bytes per input byte, and a 1.10 safety multiplier. It produced
zero training or held-out underpredictions, no extrapolations, and a 27.6%
median reservation reduction against the 36,276,289,536-byte fixed worst case.

Proceed with production scheduler work using this estimator only when all of
the following match:

- ordered modules and pinned revisions recorded in `analysis.json`;
- aggregate input size between 529,854,516 and 1,143,813,163 bytes;
- Module A followed by Module P with one active input; and
- Linux cgroup v2 telemetry available for observed-versus-estimated validation.

Use fixed batching outside that scope. Module O remains excluded because its
DEM acquisition needs owner-supplied Earthdata/ASTER credentials, and its
worker pool does not currently propagate a failed DEM worker to the module exit
status. Re-measure after module revisions, material workflow changes, or adding
representative cases; add a concurrent holdout mix before validating weighted
multi-input admission.

## Integrity

The observation file contains opaque case identities, module/config checksums,
host characteristics, and aggregate memory metrics. It contains no corpus
paths, repository credentials, or image metadata filenames.

| Artifact | SHA-256 |
|---|---|
| `observations.jsonl` | `15f11bad004efe325039ef79ee8335208180932c3fdc9dba87d60d56b3146a8b` |
| `analysis.json` | `24c9c8660c4022fb65aa38465520c0eb2d6de848b659aa17f966563f829eefbf` |
| `report.md` | `f200ea95b7a28d1d6ce00cabefe4e3670f1c560850599ff04ede96bdd05356e0` |
| `environment.explicit.txt` | `4763dbe71f656e381064c882016065b551209dec715a2118315ff063bfbe5a1f` |
| `pip.freeze.txt` | `b71617a169b60f4ed559deb48ec2306c20b6708c0e0bd9a0e7c2cccbb83b570a` |
