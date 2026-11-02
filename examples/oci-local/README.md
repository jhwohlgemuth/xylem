# Local OCI bundle example

This example uses the checked-in OCI image layout in `module/bundle` directly.
The script module points at its digest-pinned `oci-layout:` URI. Xylem verifies
the manifest and blobs, asks ORAS to extract the artifact into its immutable
cache, and executes `run.sh` from an input-local copy. No registry is needed.

`module/bundle.tar` is the equivalent uncompressed archive form. It contains
the same layout and uses the same manifest digest. To exercise it, change the
configuration URI from `oci-layout:./module/bundle@sha256:...` to
`oci-layout:./module/bundle.tar@sha256:...`.

Requirements: Xylem, ORAS, and a POSIX shell.

Run from the repository root:

```sh
cargo run --example oci-local
```

To make the no-network contract explicit with an installed Xylem binary, run:

```sh
cd examples/oci-local
xylem run --dry-run --offline --no-progress
xylem run --offline --no-progress
```

Dry-run validates the local reference without invoking ORAS or creating an
input workspace. A real offline run may populate the cache from the local
directory or archive; later runs reuse the verified entry. A malformed layout,
missing blob, digest mismatch, or acquisition-limit failure is rejected before
the module executes and does not publish a partial cache entry.

The real run writes greeting files below `output/`. For comparison, see
[`oci-local-registry`](../oci-local-registry/README.md), which publishes the
same layout to a local registry and acquires it through Xylem's `oci://`
registry module support.
