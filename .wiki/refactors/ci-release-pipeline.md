---
type: Plan
title: "Repair the CI and release pipeline"
description: "Run required checks for merge requests and relevant changes, build explicit platform artifacts, and release only from immutable version tags."
difficulty: high
impact: critical
package: [workspace]
tags: [ci, gitlab, release, build, reproducibility, cross-platform]
status: planned
---

# Goal

Ensure every proposed code or dependency change is tested before merge, and ensure release artifacts are built, named, uploaded, and published from one immutable tagged commit with reproducible inputs.

# Current evidence

- The test job runs only on the default branch and only when `xylem-cli/**/*`, `xylem-lib/**/*`, or root `Cargo.toml` changes. Merge requests, `Cargo.lock`, tests/fixtures, `Makefile`, and CI changes can bypass it.
- Build and release jobs also exclude tags with `when: never` and run on default-branch `Cargo.toml` changes, while the release job creates a `v${PKG_VERSION}` tag. This can publish a release that was not initiated by an existing immutable version tag.
- `build-release` needs `test`, but their `changes` rules are different, allowing a selected build to depend on a skipped test job.
- CI uses floating `ubuntu`, `curl:latest`, `release-cli:latest`, and ACORN `rust:latest` images.
- `make build` runs a Linux target build and a Windows target build in the same recipe and assumes PowerShell plus the Windows GNU toolchain are present.
- `make lint` mutates the worktree with `cargo fmt` and `cargo clippy --fix --allow-dirty`; CI checks should be read-only.
- Test and documentation tasks clean build outputs before running, increasing cold-build cost and hiding cache behavior.
- `TARGETS` and contributor docs mention broader cross-platform testing, but no Make target or CI matrix implements it.

# Project code boundaries

- **Update:** `.gitlab-ci.yml`, `Makefile`, workspace package metadata if release extraction needs it, and contributing/release documentation.
- **Create:** target-specific build recipes or scripts, immutable artifact checksums, CI lint/smoke fixtures, and an optional release manifest/SBOM.
- **Delete:** branch-created release behavior, floating image references, mutating lint commands in CI, unconditional clean prerequisites, and undocumented dead Make targets.

# Dependencies and ordering

Consumes commands from [dependency and security gates](dependency-surface-and-security-gates.md) and the [hermetic test suite](../testing/hermetic-workflow-tests.md). Documentation updates finish in [documentation and examples](documentation-and-examples.md).

# Recommendation

## Pipeline coverage

- Run formatting check, Clippy, unit/integration tests, dependency policy, and docs for merge requests and default-branch pushes.
- Include `Cargo.lock`, all manifests, test fixtures, build scripts, `.gitlab-ci.yml`, `Makefile`, and relevant docs in change rules, or prefer unconditional required jobs with cache reuse.
- Give jobs consistent rules and use optional `needs` only where skipping is intentional.

## Reproducible jobs

- Pin every image by immutable digest or reviewed version.
- Use `cargo fmt --check` and `cargo clippy -- -D warnings`; never fix the worktree in CI.
- Build/test with `--locked` and cache registry/git data plus target outputs by toolchain, target, profile, and lockfile hash.
- Separate Linux and Windows artifacts into target-specific jobs on capable runners. Add macOS only when a runner and tier are explicit.

## Release contract

- Trigger release builds only for an existing tag matching the workspace version.
- Verify tag, package version, and changelog entry agree before building.
- Build each target from that tag, generate SHA-256 checksums and provenance/SBOM metadata, then upload immutable versioned artifacts.
- Create the GitLab release after uploads succeed and link to the exact artifacts. Never create the release tag from a branch pipeline.

# Validation

- Validate YAML with GitLab CI Lint and exercise merge-request, default-branch, tag, docs-only, lockfile-only, and CI-only rule scenarios.
- Run every Make target in a clean Linux environment and each platform build on its owning runner.
- Assert CI leaves `git diff --exit-code` clean after lint/test/docs jobs.
- Perform a non-publishing release rehearsal that verifies artifact names, checksums, target formats, version agreement, and release links.
- Cancel/retry jobs and verify cache/artifact behavior is deterministic.

# Acceptance criteria

- Merge requests cannot merge without required format, lint, test, dependency, and documentation checks.
- Changes to lockfiles, tests, build/CI files, and manifests cannot bypass relevant jobs.
- CI commands are read-only and use the locked graph.
- Each platform artifact is built on a declared capable runner with a pinned image/toolchain.
- Releases originate only from an existing version tag matching package metadata.
- Uploaded artifacts have verified checksums and point back to the exact commit and build inputs.
- Make targets and contributor documentation describe commands that actually exist.

# Related plans

- [Dependency surface and security gates](dependency-surface-and-security-gates.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
- [Documentation and examples](documentation-and-examples.md)
