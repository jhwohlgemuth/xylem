# Python with Conda

This example demonstrates Conda environment creation, variable interpolation,
and running a Python module inside the declared environment. The configuration
defines `MESSAGE` as `{{ GREETING }} {{ AUDIENCE }}`, so the module receives the
interpolated `Hello from Xylem` along with the reserved `MODULE_LIST` value.

Install [Conda](https://docs.conda.io/projects/conda/en/latest/user-guide/install/),
then run the example:

```shell
cargo run --example python-with-conda
```

The equivalent command using the CLI directly is:

```shell
cargo run --package xylem-cli -- -vvvv run --config examples/python-with-conda/config.json
```

On the first run, Xylem creates the `example` environment from
`environment.yml`. Later runs reuse it. The module prints a
[Polars](https://pola.rs/) DataFrame followed by the values it received:

```text
shape: (3, 2)
┌─────┬─────┐
│ a   ┆ b   │
│ --- ┆ --- │
│ i64 ┆ str │
╞═════╪═════╡
│ 1   ┆ a   │
│ 2   ┆ b   │
│ 3   ┆ c   │
└─────┴─────┘
Message: Hello from Xylem
Modules: conda_example
Namespaced environment: Hello from Xylem
```
