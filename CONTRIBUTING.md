# Contributing

## Development setup

Install the Rust toolchain declared by the workspace, clone the repository, and
install the development commands used by the `Makefile`:

```shell
git clone git@code.ornl.gov:research-enablement/xylem.git
cd xylem
make setup
```

`make setup` installs the Cargo and mdBook tools listed in `DEV_DEPENDENCIES`.
Docker or Podman is needed only for container-specific tests and examples;
Conda/Mamba and Pixi are needed only for their corresponding examples.

## Standard checks

Run these before opening a merge request:

```shell
make lint
make test
make doctest
make book
make doc
make audit
```

`make lint` runs `cargo fmt` and Clippy fixes, so review its changes before
committing. `make test-ci` runs the test command used by the main CI test job.
`make book-write` serves the mdBook locally, and generated `docs/book` output is
ignored. `make cover` writes `lcov.info`, which is also ignored.

Dependency changes must satisfy the package-count, source, license, and
advisory gates described in
[`security/DEPENDENCY_POLICY.md`](./security/DEPENDENCY_POLICY.md). Repository
security, dependency, and release helpers are implemented in the `xtask` crate;
run `cargo xtask` to see its commands.

Changes to public configuration or CLI behavior must update the relevant book
page and regenerate the checked-in help snapshots:

```shell
make help
cargo test --package xylem-cli --test help
```

Inspect the current interface directly with:

```shell
cargo run --package xylem-cli -- --help
cargo run --package xylem-cli -- run --help
```

The smallest end-to-end check is the local offline example:

```shell
cd examples/offline-smoke
cargo run --manifest-path ../../Cargo.toml --package xylem-cli -- run \
  --offline \
  --no-progress
```

## Documentation and examples

Every retained example has its own README and dependency contract. The test
suite recursively parses and semantically validates example configuration
files, and executes the offline smoke workflow from a clean temporary
directory. Keep deployment-only paths clearly labeled as placeholders.

Workspace lints deny missing documentation for public APIs. A small set of
legacy modules has explicit module-level exceptions in `xylem-lib/src/lib.rs`;
do not broaden those exceptions when adding public API.

## Coverage, release, and Homebrew tasks

- `make cover` generates local LCOV output.
- `make homebrew-formula` renders a formula for `VERSION`.
- `make build` builds the configured Linux and Windows release artifacts and
  requires their Rust targets plus PowerShell.
- `make changelog` refreshes `CHANGELOG.md` using `git-cliff` and a GitLab token.

Version-tag pipelines publish both crates and submit a formula update to the
organization Homebrew tap. Configure `CARGO_REGISTRY_TOKEN` as a masked,
protected crates.io token and `HOMEBREW_TAP_ACCESS_TOKEN` as a masked,
protected token with `write_repository` access. Optional tap overrides are
`HOMEBREW_TAP_BRANCH` and `HOMEBREW_TAP_FORMULA_PATH`.

See [`scripts/homebrew/README.md`](./scripts/homebrew/README.md) for the formula
contract, runner requirements, and local validation commands.
