# Homebrew custom tap helpers

This directory contains the formula template and generator used to publish
Xylem to the `research-enablement/homebrew-xylem` tap.

## Generate a formula locally

```shell
./scripts/homebrew/generate-formula.sh 1.0.0 ./Formula/xylem.rb
```

The version must match the workspace version in `Cargo.toml`. The generator
downloads the immutable GitLab tag archive, calculates its SHA-256 digest, and
renders `formula.template.rb`.

## CI publication

The `publish-homebrew-formula` GitLab job runs only for `vMAJOR.MINOR.PATCH`
tags. It supports a newly created empty tap as well as updates to an existing
tap branch.

Required CI variable:

- `HOMEBREW_TAP_ACCESS_TOKEN`: masked, protected project access token for the
  tap with `write_repository` scope. `ACCESS_TOKEN` is accepted as a legacy
  fallback.

Optional CI variables:

- `HOMEBREW_TAP_REPO_URL` (default:
  `https://code.ornl.gov/research-enablement/homebrew-xylem.git`)
- `HOMEBREW_TAP_BRANCH` (default: `main`)
- `HOMEBREW_TAP_FORMULA_PATH` (default: `Formula/xylem.rb`)

When the access token is protected, protect release tags so tagged pipelines
can use it.
