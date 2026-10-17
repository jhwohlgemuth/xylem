# Local OCI bundle example

This example uses the checked-in OCI image layout in `module/bundle` directly.
It does not contact a registry. The Xylem script module runs ORAS against the
layout directory, pins the artifact by its manifest digest, and executes the
`run.sh` file from the artifact.

Requirements: Xylem, ORAS, and a POSIX shell.

Run from the repository root:

```sh
cargo run --example oci-local
```

The example writes greeting files below `output/`. For comparison, see
[`oci-local-registry`](../oci-local-registry/README.md), which publishes the
same layout to a local registry and acquires it through Xylem's `oci://`
registry module support.
