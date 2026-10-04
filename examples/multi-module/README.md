# Multi-module

This workflow passes one message through four ordered modules:

```text
Conda module → conda.txt → Pixi module → pixi.txt → script module → script.txt → binary module → final.txt
```

The first two modules run Python in separately managed Conda and Pixi
environments. The generic script and local binary then read the preceding
artifact and add their own prefix. All artifacts are written to `output/`.
Each module declares its artifact in `completion.outputs`, so Xylem verifies it
is a non-empty file after the module succeeds and can safely skip it with
`--resume` on a later run.

Install [Mamba](https://mamba.readthedocs.io/en/latest/installation/)
and [Pixi](https://pixi.sh/latest/installation/), then run from the repository
root:

```shell
cargo run --example multi-module
```

The equivalent CLI command is:

```shell
cd examples/multi-module
xylem -vvvv run
```

On success, `output/final.txt` contains:

```text
binary: script: pixi: conda: Hello from Xylem
```
