# Oklahoma geospatial sandbox artifact evidence

This directory preserves the sanitized closeout evidence for the disk-backed
sandbox artifact phase on Oklahoma. The qualified workload is the immutable
Linux/amd64 image:

`savannah.ornl.gov/research-enablement/xylem/geospatial@sha256:d0a3e444817303f418e5829ba805b945476932e456f966b8fc9d80eeee21ac03`

## Rootless conformance

The native ignored conformance test passed with the same digest under both
supported rootless runtimes after output-existence validation was enabled.
Docker run `uQJZWob1Yr` and Podman run `wRrWEIat23` each published one output
and its synthesized completion marker atomically. Both summaries record 1,197
published bytes, two entries, and positive cgroup v2 peaks of 6,124 KB and
3,104 KB, respectively.

## Real-input fail-closed check

The measured A/P holdouts were planned in `memory_aware` mode with reservations
of 28,634,438,485 and 41,120,796,343 bytes. The pinned Module A revision makes
an ASTER DEM metadata request to `cmr.earthdata.nasa.gov` during execution, but
sandbox execution is deliberately network-denied. Module A also does not
observe its process-pool futures, so the unmodified command returns zero after
the workers fail.

Docker run `exCLF1uRav` proves that this cannot become a false publication:
the declared `*_MODA.TIF` output matched no files, the run and artifact outcome
are both `failed`, and the final destination remained absent. Execution
networking was not weakened.

## Offline DEM gate

Docker run `QxdoFw1crP` used version `003.2` of the GitLab Generic Package
`aster-gdem-tiles`, pinned in `tools/memory-spike/real/aster-gdem-tiles.env`.
The package URL is
`https://code-int.ornl.gov/api/v4/projects/GSED%2Fcommon%2Fimagery-processing%2Fmodule-a/packages/generic/aster-gdem-tiles/003.2/aster-gdem-tiles-003.2.oci.tar`.
The cold-downloaded archive matched SHA-256
`33cb78ff18cbd69a4cf8fe725c37cb9bd2ccf2abea71ad0685ba6106a3ac3f80`,
resolved OCI manifest
`sha256:5e941db17fab5354c49ba9cf6ef15d28c5e1b322451cc06e9c968725962ed7f8`,
and passed all five raster checksums before staging. Module A resolved all four
input-specific mosaics from its prepared read-only cache without reaching CMR.
Its MODIS and VIIRS listing requests remained network-denied, and the patched
missing-response handling selected Module A's documented default AOT of
`0.226`.

Both holdouts and both A/P modules succeeded. Atomic publication committed 42
entries totaling 29,714,049,602 bytes. The recorded peaks were 23,650,928 KB
for WV02 and 35,573,292 KB for WV03, both below their memory-aware
reservations. The summary records required sandbox mode, denied preparation
networking, realized network denial, a read-only root, and the same immutable
geospatial workload digest.

Podman run `rbpUE3-vny` then exercised the same two holdouts and workload under
rootless Podman 3.4.4. Both inputs were admitted concurrently, every A/P module
succeeded, and the recorded peaks were 22,993,396 KB and 34,549,708 KB against
reservations of 27,963,318.833007813 KB and 40,157,027.67871094 KB. Atomic
publication committed 42 entries totaling 29,714,049,714 bytes. The final
destination exists, and no run-owned staging tree or container remains.

The Podman run also qualified two portability changes in the sandbox lifecycle:
archives are exported through an exact writable file bind instead of the
runtime's stdout stream, and Podman returns raw staging ownership to the host
through its user namespace after archive validation. Three pre-qualification
attempts failed closed while those paths were diagnosed; none published a
destination, and their private raw staging data was removed after retaining the
durable result.

The retained Module A logs establish the cache and default-AOT paths without
retaining generated rasters. The successful Docker and Podman journals and
summaries retain the real-input comparison without retaining generated rasters.

Generated raster products and diagnostic module copies remain off-repository.
The earlier invalid 26 KB task-generated destination was removed after the
missing-output invariant was identified.

## Integrity

A scan found no credentials, authorization headers, tokens, passwords,
secrets, or private keys in the retained evidence.

| Artifact | SHA-256 |
|---|---|
| `docker-journal.jsonl` | `48de7c4e788c806e724ef1363ad3a70a837463b2f0c96bd525b6b93f70752557` |
| `docker-summary.json` | `c56bb3d26de9562fc5b52602175c7b073be5406589a7a8152d2ed7c40b047552` |
| `offline-dem-logs/wv02-module-a.jsonl` | `e70364817f1b895b014032730192ff99e1da6ee424c8ff5b63d1f648eac3bb93` |
| `offline-dem-logs/wv03-module-a.jsonl` | `013458d6195562b9cd7f32639908249ab7a0c62dbb1fbe09fd88773342aac9c1` |
| `podman-journal.jsonl` | `8a52c61b554b0328c8ea994859384bf60e69a7dd9b2ae9074be0a951b1aa9578` |
| `podman-summary.json` | `1d3f72bc6c1bc53b066157624d73aa970b64c71eef28cf153984d822942788f0` |
| `real-input-docker-failclosed-journal.jsonl` | `a8b63229aa2cb1a973ae0142042059159446b8de39abe87c6e2c9d06dc4abe36` |
| `real-input-docker-failclosed-summary.json` | `7b878501ceae6a22374680bb7460691f0af8e05205720519916d4deadb2f5732` |
| `real-input-docker-offline-dem-journal.jsonl` | `7e945de9f2457202c94ebba8cd0bc89b215a478e07f000169a438f8808bbc1ac` |
| `real-input-docker-offline-dem-summary.json` | `331da972f53e09edd58557e9fa31876382714637c21424485022407d77a375ed` |
| `real-input-podman-offline-dem-journal.jsonl` | `3e31045cb25c3f16b365db3320d21667c8ea5eb63c0cf422144d4a8b5bd4fda9` |
| `real-input-podman-offline-dem-summary.json` | `b38496511bbd3d0fe6a7594b7f54eb32d3c868893e2382672b9a30e3ddc31d84` |
