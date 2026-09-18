# Python with Pixi

This example demonstrates Pixi environment creation, Python execution, nested
workflow-variable interpolation, the built-in `MODULE_LIST` value, and
namespaced variables exposed to the child process.

Install [Pixi](https://pixi.sh/latest/installation/), then run this command from
the Xylem repository root:

```shell
cargo run --package xylem-cli -- -vvvv run --config examples/python-with-pixi/config.json
```

With an installed `xylem` binary, the shorter equivalent is:

```shell
xylem -vvvv run --config examples/python-with-pixi/config.json
```

On the first run, Xylem executes `pixi install --environment default` in this
directory. Pixi reuses the resulting `.pixi` environment on later runs. The
trace output contains these module results:

```text
Hello from Xylem
Workflow modules: pixi_example
Namespaced environment: Hello from Xylem
```
