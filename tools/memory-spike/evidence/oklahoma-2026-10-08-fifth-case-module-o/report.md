# Memory-aware scheduling spike results

## Decision

**NO-GO**

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
| module-a-p-o-v1 | NoGo | none | 2347008 | 68715487232 |

## Candidate evidence

| Key | Model | Accepted | Intercept | Slope | Safety multiplier | Mean absolute error bytes | Case underpredictions | Mix underpredictions | Extrapolations | Median reduction |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| module-a-p-o-v1 | DecodedBytes | false | 1543767208.000 | 7.980 | 1.10 | 2596757504 | 0 | 1 | 1 | 61.7% |
| module-a-p-o-v1 | InputBytes | false | 3697283379.507 | 29.775 | 1.10 | 2547474432 | 0 | 1 | 1 | 61.8% |
| module-a-p-o-v1 | SizeBucket | false | 0.000 | 0.000 | 1.05 | 14358634496 | 0 | 1 | 1 | 44.6% |

Exact feature ranges, bucket peaks, maximum underprediction, and p95 overreservation are retained in the JSON analysis artifact.

## Required operator additions

Before accepting this report, record the corpus coverage, delegated cgroup path, sanitized observation checksum, measurement noise, limitations, and operational recommendation.
