# Examples

The repository's [`examples/`](https://code.ornl.gov/research-enablement/xylem/-/tree/main/examples)
directory contains single-module and composed workflows.

The self-contained examples below also have Cargo example targets, so they can be
run without spelling out a configuration path:

```shell
cargo run --example python-with-pixi
cargo run --example python-with-conda
```

These targets work from any directory because they change into the workspace
root before reading the configuration. Listing `cargo run --example` without a
name shows every available target. They disable progress bars and pin the
verbose configuration dumps to `INFO` so the run reads as a workflow narrative;
set `XYLEM_LOG_LEVEL` to override the filter.

## Python with Pixi

The self-contained Python example demonstrates environment creation, variable
interpolation, and runs a Python module that prints a Polars DataFrame. Install
[Pixi](https://pixi.sh), then run it from the repository root:

```shell
cargo run --package xylem-cli -- -vvvv run --config examples/python-with-pixi/config.json
```

With Xylem installed:

```shell
xylem -vvvv run --config examples/python-with-pixi/config.json
```

The first execution creates the declared Pixi environment. Later executions
reuse it.

## Python with Conda

The Conda example creates the `example` environment from `environment.yml` and
runs the same interpolated module. Install
[Conda](https://docs.conda.io/projects/conda/en/stable/user-guide/install/index.html),
then run it from the repository root:

```shell
cargo run --package xylem-cli -- -vvvv run --config examples/python-with-conda/config.json
```

## Geospatial pipelines

The atmospheric-correction, pansharpening, and orthorectification examples show
how ordered modules exchange paths through variables. Their module URIs are
deployment-specific placeholders and must be updated before use.
