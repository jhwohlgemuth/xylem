# Offline smoke example

This is the smallest runnable Xylem workflow in the repository. It uses only
the local POSIX shell, performs no network access, and writes two greeting
files under `output/`.

From a clean checkout, run:

```shell
cargo run --package xylem-cli -- run \
  --config examples/offline-smoke/config.json \
  --offline \
  --no-progress
```

Inspect the same workflow without executing it or writing files:

```shell
cargo run --package xylem-cli -- run \
  --config examples/offline-smoke/config.json \
  --offline \
  --dry-run \
  --reporter json
```

The executable script is deliberately checked in with the workflow so the
example does not depend on Conda, Pixi, a container runtime, or remote modules.
