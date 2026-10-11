# Memory-aware scheduling example

This offline example shows how a workflow opts into memory-aware admission. It
uses two deterministic local inputs, a two-input batch ceiling, and an
illustrative fixed-point estimator:

```text
reservation = ceil((1 KiB + input bytes) × 1.25)
```

The 1 KiB and 2 KiB inputs reserve 2.5 KiB and 3.75 KiB. Their combined 6.25
KiB reservation exceeds the 6 KiB budget, so Xylem admits only one at a time
even though `options.batch` permits two.

Run the example from the repository root:

```shell
cargo run --example memory-aware
```

Inspect its memory plan without executing the module:

```shell
cd examples/memory-aware
xylem run \
  --offline \
  --dry-run \
  --reporter json
```

The plan reports `memory_aware` mode and a reservation for each input. The
companion configuration uses a 3 KiB input outside the calibrated 1–2 KiB
range, which selects fixed batching for the whole run:

```shell
xylem run .xylem-fallback.json \
  --offline \
  --dry-run \
  --reporter json
```

The coefficients are teaching values for these fixtures, not a reusable memory
model. A production workflow must measure its own modules and representative
inputs, choose a conservative safety multiplier and range, and give the
resulting estimator a distinct identity.
