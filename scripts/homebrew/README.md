# Homebrew custom tap helpers

The Homebrew implementation follows the same small structure as ACORN: one
formula template and one generator. GitLab CI owns publication, while the tap
CI configuration owns Homebrew validation.

## Generate a formula locally

```shell
./scripts/homebrew/generate-formula.sh 1.0.0 ./Formula/xylem.rb
```

The version must match the workspace version in `Cargo.toml`. The generator
downloads the immutable `xylem-cli` crate from crates.io, calculates its SHA-256
digest, and renders `formula.template.rb`. For deterministic offline generation
from explicit release metadata, use:

```shell
./scripts/homebrew/generate-formula.sh 1.0.0 ./Formula/xylem.rb <sha256>
```

The `test-homebrew-formula` GitLab job checks deterministic output, immutable
URLs, Ruby syntax, invalid versions and checksums, and formula drift directly;
there is no separate Homebrew test program.

## CI publication

Release binaries are uploaded before GitLab creates the version tag. The tag
pipeline publishes `xylem-lib` and `xylem-cli` to crates.io in dependency order,
verifies both binary artifacts, renders the formula from the published CLI crate,
and runs the publication commands only for `vMAJOR.MINOR.PATCH` tags. Those commands
initialize a newly created empty tap when needed and push
`release/xylem-vMAJOR.MINOR.PATCH` with GitLab push options that open a merge
request against the configured tap branch. Formula updates are never pushed
directly to that branch.

Required CI variables:

- `CARGO_REGISTRY_TOKEN`: masked, protected crates.io API token authorized to
  publish both `xylem-lib` and `xylem-cli`.
- `HOMEBREW_TAP_ACCESS_TOKEN`: masked, protected project access token for the
  tap with only the `write_repository` scope.

Optional CI variables:

- `HOMEBREW_TAP_REPO_URL` (default:
  `https://code.ornl.gov/research-enablement/homebrew-xylem.git`)
- `HOMEBREW_TAP_BRANCH` (default: `main`)
- `HOMEBREW_TAP_FORMULA_PATH` (default: `Formula/xylem.rb`)

When the access token is protected, protect release tags so tagged pipelines
can use it. The tap's default branch should be protected and configured to
require successful merge-request pipelines.

## Tap validation

Each formula merge request installs and tests the formula on both Linuxbrew and
Homebrew for macOS. The tap pipeline runs `brew style`,
`brew audit --strict`, a source install, `brew test`, and, when a previous
formula exists, `brew upgrade`.

The tap expects GitLab runners tagged `cpu` and `macos`. Override
`HOMEBREW_LINUX_RUNNER_TAG` or `HOMEBREW_MACOS_RUNNER_TAG` in the tap project
when the runner tags differ. Homebrew's `brew test-bot` supports GitHub Actions
only, so the GitLab tap runs the equivalent lifecycle commands directly.
