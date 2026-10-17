# Local OCI registry example

This example acquires a digest-pinned module from a registry on the local
machine. It uses a checked-in ORAS OCI layout so the artifact can be loaded
into a local registry without rebuilding it.

Requirements: Docker or Podman, ORAS, and Xylem.

Start a local registry:

```sh
docker run --detach --rm --name xylem-oci-local-registry -p 5000:5000 registry:2
```

Copy the checked-in layout into it:

```sh
oras cp --from-oci-layout --to-plain-http examples/oci-local-registry/artifact:example \
  localhost:5000/research-enablement/xylem/oci-local-registry:example
```

Then run the example from the repository root:

```sh
cd examples/oci-local-registry
xylem run --allow-insecure-http
```

The configuration pins the artifact manifest digest and permits only
`localhost:5000`. Xylem requires `oras` on `PATH` and verifies the extracted
tree before caching or running it. The operator-only `--allow-insecure-http`
flag is required because the local registry serves HTTP.
