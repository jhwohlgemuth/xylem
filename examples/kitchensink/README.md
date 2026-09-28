# Kitchensink

This runnable workflow exercises the main local workflow features in one place:

- two configured inputs (`alpha` and `bravo`);
- workflow and module-scoped variables with recursive interpolation;
- Mamba-managed Conda Python and Pixi Python modules;
- a generic script loaded from a `template.json` file;
- a local configuration module with a nested executable leaf;
- a native local binary module built from portable Rust source;
- a Podman container module;
- non-empty output verification and completion markers for every stage; and
- resumable execution with `--resume`.

For each input, the modules pass files in order:

```text
Mamba → mamba.txt → Pixi → pixi.txt → script → script.txt → binary → final.txt → Podman → podman.txt
```

Install [Mamba](https://mamba.readthedocs.io/en/latest/installation/),
[Pixi](https://pixi.sh/latest/installation/), and
[Podman](https://podman.io/). The Podman module uses `podman exec`, so Xylem
does not create the container or mount the repository; you must start it before
running the workflow. On macOS, initialize and start the Podman VM once:

```shell
podman machine init
podman machine start
```

Start a container with the repository mounted at `/workspace/xylem`, then run the
workflow. The example runner compiles the local binary with the Rust toolchain
that `cargo run` already requires, producing a native opaque executable on
Windows, Linux (including musl hosts), and macOS.

```shell
podman run --rm --detach --name xylem-kitchensink --volume "$PWD:/workspace/xylem" alpine:3.22 sleep infinity
cargo run --example kitchensink
```

To run the CLI directly, build the native binary first:

```shell
rustc --edition=2024 -O examples/kitchensink/binary/finalize.rs -o examples/kitchensink/binary/finalize
cargo run --package xylem-cli -- -vvvv run --config examples/kitchensink/config.json
```

The output files are written below `output/alpha/` and `output/bravo/`. Re-run
with `--resume` to skip stages whose declared outputs and completion markers
still match the workflow configuration.

Stop the container when finished with `podman stop xylem-kitchensink`.
