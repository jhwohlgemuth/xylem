# Prompt-policy failure evidence

These retained runs exercise the native rootless sandbox conformance test on
Oklahoma with Rust 1.96.0 and the pinned workload:

```text
savannah.ornl.gov/research-enablement/xylem/geospatial@sha256:d0a3e444817303f418e5829ba805b945476932e456f966b8fc9d80eeee21ac03
```

The test first runs a successful control, then injects a module failure with
the upstream `Invalid prompt` diagnostic. Xylem treats the diagnostic as opaque
process output; it does not classify the provider by matching message text and
does not rewrite or retry the prompt.

| Runtime | Control | Control peak | Rejected input | Artifacts | Rejected peak event |
| --- | --- | ---: | --- | --- | --- |
| Docker | `UZrQ-NYW3t` succeeded | 6160 KiB | `uTyx8NekNn` failed | discarded | absent |
| Podman | `-wOgRy9ebY` succeeded | 16664 KiB | `DXR3f7VPEX` failed | discarded | absent |

The successful controls published two declared artifact entries. Both rejected
runs retained the diagnostic, reported `observed_peak: null`, finalized artifact
status as `discarded`, and left the publication destination absent.

Files ending in `-summary.json` are rendered run summaries. Files ending in
`-journal.jsonl` are the corresponding append-only journals. The
`*-policy-rejection-journal.jsonl` files contain no `input_memory_observed`
event.

An integrity scan found no credentials, authorization headers, passwords,
secrets, API keys, or private keys in the retained files.

| Artifact | SHA-256 |
| --- | --- |
| `docker-journal.jsonl` | `92a24ef0106d558cd6c710bf79277043578e7d674b035515a615605620881375` |
| `docker-policy-rejection-journal.jsonl` | `81da7c24f5e675e96999d2e1c8e2195c637db942bfbf512ecfdfa7d4e434d554` |
| `docker-policy-rejection-summary.json` | `1002d7a1d3b03f39439bd3a1b26e460bfc110b6d8eb40856603f67669c245526` |
| `docker-summary.json` | `e3fe82130e13e13b3ce285e30564d64cd179b24443257e603f3d81630a88d97a` |
| `podman-journal.jsonl` | `6ea04c95a40dfdd134ce8f6b1b70ddf30c8c660d298b590a2de59cb2f75f81f5` |
| `podman-policy-rejection-journal.jsonl` | `237bab3c1388267135db9fa666e49b26f7a6eeff6746927a1df708e67f57a2f1` |
| `podman-policy-rejection-summary.json` | `0dd198619c4fec56816115fafda951792e670ab5227792186b2605a00df4fc01` |
| `podman-summary.json` | `05ce89201690681ef707e435eb484dd225f545276d849aacd2d4e65e9407d656` |
