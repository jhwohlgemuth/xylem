# Python with Pixi

This example demonstrates Pixi environment creation, variable interpolation,
and running a Python module inside the declared environment. The configuration
defines `MESSAGE` as `{{ GREETING }} {{ AUDIENCE }}`, so the module receives the
interpolated `Hello from Xylem` along with the reserved `MODULE_LIST` value.

Install [Pixi](https://pixi.sh/latest/installation/), then run the example:

```shell
cargo run --example python-with-pixi
```

The equivalent command using the CLI directly is:

```shell
cd examples/python-with-pixi
xylem -vvvv run
```

With an installed `xylem` binary, the shorter equivalent is:

```shell
xylem -vvvv run
```

The CLI forms must be run from the example directory because relative module
URIs resolve against the working directory. The example target changes into
that directory on its own, so it works from any directory.

On the first run, Xylem creates the `default` environment from `pixi.toml`.
Later runs reuse it. The module prints a [Polars](https://pola.rs/) DataFrame
followed by the values it received:

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
Modules: pixi_example
Namespaced environment: Hello from Xylem
```

The Pixi manifest sets `PYTHONUTF8=1` under `[activation.env]`. Without it, the
environment inherits the Windows `cp1252` stdout encoding, which cannot encode
the box-drawing characters in the DataFrame.
