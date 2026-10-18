# Kitchensink

This runnable workflow exercises the main local workflow features in one place:

- two configured inputs (`alpha` and `bravo`);
- workflow and module-scoped variables with recursive interpolation;
- Mamba-managed Conda Python and Pixi Python modules;
- a generic script loaded from a `template.json` file;
- a local configuration module with a nested executable leaf;
- a native local binary module built from portable Rust source;
- a digest-pinned script module loaded from a checked-in local OCI layout;
- a Podman container module;
- non-empty output verification and completion markers for every stage; and
- resumable execution with `--resume`.

For each input, the modules pass files in order:

```text
Mamba → mamba.txt → Pixi → pixi.txt → script → script.txt → binary → final.txt → OCI → oci.txt → Podman → podman.txt
```

Install [Mamba](https://mamba.readthedocs.io/en/latest/installation/),
[Pixi](https://pixi.sh/latest/installation/), [ORAS](https://oras.land/), and
[Podman](https://podman.io/). The OCI stage reads the checked-in layout in
`oci/bundle` without contacting a registry. Xylem creates the Podman container,
mounts the repository, keeps the Alpine container idle with `sleep infinity`,
waits for readiness, and removes it when the run finishes. The `keep_alive` option requires
a Linux image whose `sleep` implementation accepts `infinity`. On macOS,
initialize and start the Podman VM once:

```shell
podman machine init
podman machine start
```

The checked-in `oci/bundle` was built from `oci/run.sh`. After changing that
script, rebuild the layout from `examples/kitchensink/oci` with
`oras push --oci-layout bundle:example run.sh`, then update the manifest digest
in `.xylem.json`.

Run the workflow. The example runner compiles the local binary with the Rust toolchain
that `cargo run` already requires, producing a native opaque executable on
Windows, Linux (including musl hosts), and macOS.

```shell
cargo run --example kitchensink
```

To run the CLI directly, build the native binary first:

```shell
rustc --edition=2024 -O examples/kitchensink/binary/finalize.rs -o examples/kitchensink/binary/finalize
cd examples/kitchensink
xylem -vvvv run
```

The output files are written below `output/alpha/` and `output/bravo/`. Re-run
with `--resume` to skip stages whose declared outputs and completion markers
still match the workflow configuration.
