# Examples

The repository's [`examples/`](https://code.ornl.gov/research-enablement/xylem/-/tree/main/examples)
directory contains single-module and composed workflows.

Start with the offline smoke workflow. It uses only a checked-in POSIX shell
script and requires no environment manager, container runtime, or network:

```shell
cargo run --example offline-smoke
```

The equivalent CLI invocation uses automatic configuration discovery:

```shell
cd examples/offline-smoke
xylem run \
  --offline \
  --no-progress
```

The checked-in runnable examples below also have Cargo example targets, so they can be
run without spelling out a configuration path:

```shell
cargo run --example failure-logging
cargo run --example offline-smoke
cargo run --example kitchensink
cargo run --example multi-module
cargo run --example python-with-conda
cargo run --example python-with-pixi
```

These targets work from any directory because they change into the workspace
root before reading the configuration. Listing `cargo run --example` without a
name shows every available target. They disable progress bars and pin the
verbose configuration dumps to `INFO` so the run reads as a workflow narrative;
set `XYLEM_LOG_LEVEL` to override the filter.

## Failure logging

The shell-only failure logging example runs four inputs through Prepare,
Validate, and Publish modules. Validation intentionally rejects `bravo` and
`delta`, while `alpha` and `charlie` finish the workflow. The command exits
with a failure status by design:

```shell
cargo run --example failure-logging
```

See the [example README](https://code.ornl.gov/research-enablement/xylem/-/blob/main/examples/failure-logging/README.md)
for a representative terminal transcript and the resulting module states.

## Multi-module

The multi-module example chains a Mamba Python module, Pixi Python module,
generic script module, and local binary module. Each stage reads the artifact
from the preceding stage and writes the next one. Install Mamba and Pixi, then
run:

```shell
cd examples/multi-module
xylem -vvvv run
```

## Kitchensink

The kitchensink example processes two inputs through Mamba-managed Conda, Pixi,
generic script, local binary, and Docker modules. It also demonstrates
module-scoped variables, template-file loading, completion output verification,
and resume. Its Docker module uses an existing container and repository bind
mount; see the [kitchensink README](https://code.ornl.gov/research-enablement/xylem/-/blob/main/examples/kitchensink/README.md)
for setup.

```shell
cd examples/kitchensink
xylem -vvvv run
```

## Python with Pixi

The self-contained Python example demonstrates environment creation, variable
interpolation, and runs a Python module that prints a Polars DataFrame. Install
[Pixi](https://pixi.sh), then run it from the repository root:

```shell
cd examples/python-with-pixi
xylem -vvvv run
```

With Xylem installed:

```shell
xylem -vvvv run
```

The first execution creates the declared Pixi environment. Later executions
reuse it.

## Python with Conda

The Conda example creates the `example` environment from `environment.yml` and
runs the same interpolated module. Install
[Conda](https://docs.conda.io/projects/conda/en/latest/user-guide/install/),
then run it from the repository root:

```shell
cd examples/python-with-conda
xylem -vvvv run
```

## Geospatial pipelines

The atmospheric-correction, pansharpening, and orthorectification examples show
how ordered modules exchange paths through variables. Their module URIs are
deployment-specific placeholders and must be updated before use.
