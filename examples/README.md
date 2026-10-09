# Xylem examples

Examples are grouped by the dependencies they require:

- [`offline-smoke`](./offline-smoke/) is the supported first-run example. It
  uses only a local POSIX shell and works with `--offline`.
- [`failure-logging`](./failure-logging/) is an intentionally failing,
  shell-only workflow that demonstrates concurrent inputs and module logs.
- [`python-with-pixi`](./python-with-pixi/) and
  [`python-with-conda`](./python-with-conda/) demonstrate managed Python
  environments.
- [`multi-module`](./multi-module/) and [`kitchensink`](./kitchensink/) exercise
  composed workflows and optional environment/container integrations.
- The [`atmospheric_correction-pansharpening`](./atmospheric_correction-pansharpening/)
  deployment template includes the measured two-input A/P memory profile. The
  other geospatial directories remain deployment templates. Their `/data/...`
  inputs and `file:///path/to/...` module URIs must be replaced before
  execution.
- [`inception`](./inception/) demonstrates a pinned remote binary and therefore
  is not an offline example.

Every top-level `.xylem.json` and nested workflow configuration below this
directory is parsed and semantically validated by the Rust test suite. The
runnable smoke example is also executed in both dry-run and offline modes from
a temporary clean directory using automatic configuration discovery.
