# Atmospheric Correction and Pansharpening

This is the two-module variant of the
`atmospheric_correction-pansharpening-orthorectification` example. It removes
the final Module O configuration and applies the measured `module-a-p-v2`
memory profile.

The example admits at most two top-level input directories concurrently. Both
inputs must fit the estimator's calibrated 529,854,516–2,588,922,236-byte
logical input range, and their combined reservations must fit the 72 GiB
budget. An unavailable or out-of-range input makes the whole run retain fixed
batching instead of extrapolating the model.

Before running it, replace the Module A and Module P placeholder `file://` URIs
with local checkouts. The evidence covers Module A revision
`2396c36644a89170bde2da2f29a65a364b21338c` followed by Module P revision
`a80b166736e8174171e398653fda925d3c124fae`. Also update `input`, `output`, and
the 72 GiB budget to match storage layout and memory dedicated to Xylem.

From this directory, inspect the resolved plan before starting work:

```bash
xylem run --dry-run
```

Then run the workflow:

```bash
xylem run -vvvv
```

Use `xylem run --resume` to skip a module whose declared outputs and completion
marker are current.

## Two-input canary

After updating the deployment paths, inspect the two measured holdouts without
starting module work:

```bash
xylem --threads 2 run \
  --input /data/test-cases/test-case-WV02-medium-mid-high,/data/test-cases/test-case-wv03-large-mid-low \
  --dry-run
```

The plan must select `memory_aware`, identify `module-a-p-v2`, and reserve
28,634,438,485 and 41,120,796,343 bytes in input order. Then run the canary with
durable JSON evidence:

```bash
xylem --threads 2 run \
  --input /data/test-cases/test-case-WV02-medium-mid-high,/data/test-cases/test-case-wv03-large-mid-low \
  --reporter json \
  --report-output /data/reports/module-a-p-v2-canary
```

Accept the deployment only if both inputs start before either reaches a
terminal state, all modules succeed, and the summary retains the expected
estimator and reservations. Unsandboxed runs do not collect per-input cgroup
peaks; use Linux OCI sandbox evidence or the external cgroup measurement
harness when observed-versus-reserved validation is required.

The reference Oklahoma canary passed these admission and completion checks on
2026-10-09. Its journal, summary, and measurement notes are retained in the
[activation evidence](../../tools/memory-spike/evidence/oklahoma-2026-10-09-v2-canary/README.md).
