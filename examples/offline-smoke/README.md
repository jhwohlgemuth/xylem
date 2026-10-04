# Offline smoke example

This is the smallest runnable Xylem workflow in the repository. It uses only
the local POSIX shell, performs no network access, and writes two greeting
files under `output/`.

From a clean checkout, run:

```shell
cargo run --example offline-smoke
```

With Xylem installed, the configuration is discovered from the example
directory:

```shell
cd examples/offline-smoke
xylem run \
  --offline \
  --no-progress
```

Inspect the same workflow without executing it or writing files:

```shell
xylem run \
  --offline \
  --dry-run \
  --reporter json
```

The executable script is deliberately checked in with the workflow so the
example does not depend on Conda, Pixi, a container runtime, or remote modules.
