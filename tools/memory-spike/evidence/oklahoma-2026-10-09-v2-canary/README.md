# Oklahoma Module A + P v2 activation canary

This directory preserves the controlled `module-a-p-v2` activation canary run
on Oklahoma on 2026-10-09. Run `X110ljMr1R` completed successfully in 5 minutes
and 51 seconds with two concurrent inputs.

## Result

The activation canary **PASSED**:

- The run selected `memory_aware` mode with estimator identity
  `module-a-p-v2`, a 72 GiB budget, and a 1.15 safety multiplier.
- The medium WV02 and WV03 mid/low holdouts reserved 28,634,438,485 and
  41,120,796,343 bytes in input order. Their combined reservation was
  69,755,234,828 bytes.
- Both inputs entered `running` at journal sequences 8 and 10, before the first
  module terminal event at sequence 13. Both Module A executions overlapped,
  and both Module P executions entered `running` at sequences 14 and 16 before
  the first Module P terminal event at sequence 17.
- Both inputs and all four module executions succeeded. The container exited
  with status 0 and was not OOM-killed.
- The parent container's live cgroup v2 `memory.peak` counter reached
  52,214,681,600 bytes, 74.85% of the combined reservation and within the
  52,209,172,480–52,842,696,704-byte range measured by the earlier concurrent
  holdout runs.

The run used the pinned configuration at
`tools/memory-spike/real/canary-module-a-p-v2.json`. Its outputs were isolated
under one directory per input so concurrent modules did not share completion
markers or products.

## Evidence boundary

This was an unsandboxed Xylem run inside a privileged measurement container.
Consequently, the durable summary correctly retains `observed_peak: null` for
each input; Xylem did not own a per-input OCI cgroup from which to collect a
peak. The parent-container value above was read directly from
`/sys/fs/cgroup/memory.peak` while the run was active. Rootless native Linux OCI
conformance remains a separate gate for durable per-input peak collection.

The repository retains the journal, summary, metrics, manifest, and module
logs. Generated raster products remain on Oklahoma and are not copied into the
repository.

## Integrity

A scan found no credentials, authorization headers, tokens, passwords,
secrets, or private keys in the retained evidence.

| Artifact | SHA-256 |
|---|---|
| `runs/X110ljMr1R/journal.jsonl` | `03d6014618361492cf0da7601f72f74eac2af59d1d3b25850ff5d96f63318adb` |
| `runs/X110ljMr1R/manifest.json` | `fa88ac6cb5df951a443a00e8baa80b5cc819612f8b52910657cb1d167bf0d0ca` |
| `runs/X110ljMr1R/metrics.jsonl` | `483cddcc144c86e781d9b3a7ac3b5b45896400ef3e706f15db3643c6b5557937` |
| `runs/X110ljMr1R/summary.json` | `17fd0e745964cfc174dc68eeb7da9cfa01729f0a1a70030fa83f3d6750c3e6fd` |
| `runs/X110ljMr1R/logs/input-0/module-0.jsonl` | `22fbebbb44a5977366034acffa883645aef5472abe721bc67f40b036e80e62df` |
| `runs/X110ljMr1R/logs/input-0/module-1.jsonl` | `11c1c8f97065fdd81c0b4c98f2e52651dde5062d5988403fbf321e4829b5ff23` |
| `runs/X110ljMr1R/logs/input-1/module-0.jsonl` | `947d129a33680abffdf7b0386de9209491fe756cf09b898bc48fdb3470f219c9` |
| `runs/X110ljMr1R/logs/input-1/module-1.jsonl` | `0cef7ad92975032e61407ef492e0324ad429a2e38d486c587727a236de7aaf8e` |
