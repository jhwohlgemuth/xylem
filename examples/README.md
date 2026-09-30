# Xylem examples

Examples are grouped by the dependencies they require:

- [`offline-smoke`](./offline-smoke/) is the supported first-run example. It
  uses only a local POSIX shell and works with `--offline`.
- [`python-with-pixi`](./python-with-pixi/) and
  [`python-with-conda`](./python-with-conda/) demonstrate managed Python
  environments.
- [`multi-module`](./multi-module/) and [`kitchensink`](./kitchensink/) exercise
  composed workflows and optional environment/container integrations.
- The geospatial directories are deployment templates. Their `/data/...` inputs
  and `file:///path/to/...` module URIs must be replaced before execution.
- [`inception`](./inception/) demonstrates a pinned remote binary and therefore
  is not an offline example.

Every `config.json`, `config.yaml`, and `config.yml` below this directory is
parsed and semantically validated by the Rust test suite. The runnable smoke
example is also executed in both dry-run and offline modes from a temporary
clean directory.
