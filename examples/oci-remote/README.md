# Remote OCI artifact example

This example runs the `oci-remote-v2` script artifact published to
`savannah.ornl.gov/research-enablement/xylem/example`.

Requirements: ORAS, network access to Savannah, and registry credentials when
the artifact is not publicly readable. Xylem requires `oras` on `PATH`.

Run from the repository root:

```sh
cargo run --example oci-remote
```

The workflow allowlists only `savannah.ornl.gov`. Its URI pins the published
manifest digest, so changing a registry tag does not change the module Xylem
executes.
