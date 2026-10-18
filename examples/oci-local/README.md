# Local OCI bundle example

This example uses the checked-in OCI image layout in `module/bundle` directly.
The script module points at its digest-pinned `oci-layout:` URI. Xylem uses
ORAS to extract the artifact and executes its `run.sh` file. No registry is
needed.

Requirements: Xylem, ORAS, and a POSIX shell.

Run from the repository root:

```sh
cargo run --example oci-local
```

The example writes greeting files below `output/`. For comparison, see
[`oci-local-registry`](../oci-local-registry/README.md), which publishes the
same layout to a local registry and acquires it through Xylem's `oci://`
registry module support.
