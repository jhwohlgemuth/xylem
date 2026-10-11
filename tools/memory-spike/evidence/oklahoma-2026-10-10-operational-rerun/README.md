# Oklahoma A/P operational rerun evidence

This directory preserves the production-style observed-versus-estimated rerun
of the accepted `module-a-p-v2` workflow on Oklahoma. Docker run `UAGcVSj6wQ`
and Podman run `dZyvyDx2Ot` used the same current-source Xylem debug binary,
whose SHA-256 was
`90de817a4a3835f6d94d12e86a46e7da7d711a0333075a707b6eafebf83ccf63`.

Both runs used the immutable Linux/amd64 workload
`savannah.ornl.gov/research-enablement/xylem/geospatial@sha256:d0a3e444817303f418e5829ba805b945476932e456f966b8fc9d80eeee21ac03`,
the verified offline ASTER DEM cache, a 72 GiB admission budget, and the two
accepted holdouts. Both inputs were admitted concurrently. Every module and
input succeeded, and each runtime atomically published 42 entries totaling
29,714,050,098 bytes. No sandbox container remained after either run.

## Estimate comparison

The configured safety multiplier is 1.15. A negative base-estimate error means
the unsafeguarded estimate was below the observed peak; the enforced
reservation still covered every observation.

| Runtime | Input | Base estimate | Observed peak | Base error | Reservation | Headroom |
|---|---:|---:|---:|---:|---:|---:|
| Docker | 0 | 24,315,929 KB | 23,651,516 KB | +2.81% | 27,963,319 KB | +18.23% |
| Docker | 1 | 34,919,155 KB | 35,663,024 KB | -2.09% | 40,157,028 KB | +12.60% |
| Podman | 0 | 24,315,929 KB | 22,498,308 KB | +8.08% | 27,963,319 KB | +24.29% |
| Podman | 1 | 34,919,155 KB | 35,400,180 KB | -1.36% | 40,157,028 KB | +13.44% |

Both summaries record required sandbox mode, denied preparation networking, a
read-only root, rootless execution, resource limits, positive per-input cgroup
v2 peaks, and published artifacts. The operational comparison is **GO**: every
observed peak remained below its enforced reservation.

## Stale-binary diagnostic

Before the build-identical reruns, the older staged binary with SHA-256
`0da29c408257fc53d92379705f4a5bbb918930e624f98f6af1febe4d027b57f3`
failed closed under Podman while validating a prepared archive. Run
`yODz4CB3A0` started no module, published no artifact, recorded no observed
peak, and retained `unexpected EOF during skip`. A fresh current-source binary
completed the same concurrent request, so the stale result is diagnostic only.

## Integrity

A scan found no credentials, authorization headers, tokens, passwords,
secrets, or private keys in the retained files.

| Artifact | SHA-256 |
|---|---|
| `docker-journal.jsonl` | `d271280c7e9777c536bec60e9e859138f84c5769ec9e3a9624a24344b04bd2d7` |
| `docker-summary.json` | `f5775a0e5eb984bb17ef48c4763b682a4da036a48bdaf9c547db99604a4716f5` |
| `podman-journal.jsonl` | `9f08d3594d0368118c50dd27f6eec8ccb4a38378b0cee940607aadc8a183bc36` |
| `podman-stale-binary-journal.jsonl` | `2ca061e6de65f418f3a46b5195718b43f5efa5da24e599c9e40e252368c5b168` |
| `podman-stale-binary-summary.json` | `183f3c85f663fbabc98249fb2210e9c41b12f895106877a53444c01c14b2977f` |
| `podman-summary.json` | `d9414224cea2fdd6a92bc3639bbde9a2f55e9b8b4b681cc1617ac2a7d97ef2b8` |
