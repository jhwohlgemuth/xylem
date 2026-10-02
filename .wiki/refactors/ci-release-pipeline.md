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

- The `test` job still runs only on the default branch, so merge requests are not gated by it. `changes` lists still omit `Cargo.lock`, `tests/`, `Makefile`, and `.gitlab-ci.yml` on several jobs, so those can bypass the relevant job.
- Images are still floating. Four jobs use the ACORN `rust:latest` tag, the release job uses `release-cli:latest`, and the publish job uses `curlimages/curl:latest`. `ubuntu` is unqualified in the default image.
- `make lint` still runs `cargo fmt` and `cargo clippy --no-deps --all-targets --fix --allow-dirty`, so a CI lint check built on it would mutate the worktree and hide findings.
- `make test` and `make doc` still `cargo clean` first, so CI pays a cold build and cannot observe incremental-cache behavior.
- Uploaded artifacts still have no recorded checksum or build-input provenance pointing back to the exact commit.
- `TARGETS` in the `Makefile` and the contributor docs still list nine cross-compilation targets with no Make target or CI matrix that builds them, and `make build` still assumes PowerShell plus the Windows GNU toolchain.

# Project code boundaries

- **Update:** `.gitlab-ci.yml`, `Makefile`, workspace package metadata if release extraction needs it, and contributing/release documentation.
- **Create:** target-specific build recipes or scripts, immutable artifact checksums, CI lint/smoke fixtures, and an optional release manifest/SBOM.
- **Delete:** branch-created release behavior, floating image references, mutating lint commands in CI, unconditional clean prerequisites, and undocumented dead Make targets.

# Dependencies and ordering

Consumes the implemented dependency and security commands and the [hermetic test suite](../testing/hermetic-workflow-tests.md). Release changes must update the public installation and contributor documentation directly.

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

- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
