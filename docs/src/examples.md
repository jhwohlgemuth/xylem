# Examples

The repository's [`examples/`](https://code.ornl.gov/research-enablement/xylem/-/tree/main/examples)
directory contains single-module and composed workflows.

## Python with Pixi

The self-contained Python example demonstrates environment creation, nested
variable interpolation, `MODULE_LIST`, and namespaced child-process variables.
Install [Pixi](https://pixi.sh), then run it from the repository root:

```shell
cargo run --package xylem-cli -- -vvvv run --config examples/python-with-pixi/config.json
```

With Xylem installed:

```shell
xylem -vvvv run --config examples/python-with-pixi/config.json
```

The first execution creates the declared Pixi environment. Later executions
reuse it.

## Geospatial pipelines

The atmospheric-correction, pansharpening, and orthorectification examples show
how ordered modules exchange paths through variables. Their module URIs are
deployment-specific placeholders and must be updated before use.
