# Memory-aware scheduling spike results

## Decision

**GO**

Synthetic observations validate the harness only. The decision above uses real training and held-out observations, requires zero held-out underprediction, and requires at least a 20% median reservation reduction versus fixed worst-case reservation.

## Measurement host

- Xylem: xylem-cli 1.0.3
- Kernel: 6.8.0-138-generic
- Architecture: x86_64
- CPU: 13th Gen Intel(R) Core(TM) i9-13900K
- Available parallelism: 32
- Total memory bytes: 134739816448
- Page size bytes: 4096
- Cgroup version: 2

## Estimators

| Key | Decision | Selected model | Baseline bytes | Fixed worst-case bytes |
|---|---|---|---:|---:|
| module-a-p-v2 | Go | decodedbytes | 2449408 | 68718493696 |

## Candidate evidence

| Key | Model | Accepted | Intercept | Slope | Safety multiplier | Mean absolute error bytes | Case underpredictions | Mix underpredictions | Extrapolations | Median reduction |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| module-a-p-v2 | DecodedBytes | true | 10795272368.121 | 5.034 | 1.20 | 4587612160 | 0 | 0 | 0 | 49.7% |
| module-a-p-v2 | InputBytes | true | 9083341409.049 | 23.320 | 1.15 | 4872824832 | 0 | 0 | 0 | 49.2% |
| module-a-p-v2 | SizeBucket | false | 0.000 | 0.000 | 1.05 | 14358978560 | 0 | 1 | 1 | 44.6% |

Exact feature ranges, bucket peaks, maximum underprediction, and p95 overreservation are retained in the JSON analysis artifact.

## Required operator additions

Before accepting this report, record the corpus coverage, delegated cgroup path, sanitized observation checksum, measurement noise, limitations, and operational recommendation.
