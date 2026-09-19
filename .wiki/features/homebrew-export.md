---
type: Plan
title: "Export Xylem releases to Homebrew"
description: "Generate, validate, and publish a Homebrew formula from immutable Xylem releases."
difficulty: medium
impact: high
package: [workspace, xylem-cli]
tags: [homebrew, release, distribution, macos, linux, packaging]
status: in-progress
---

# Goal

Let macOS and Linux users install and upgrade Xylem through an official Homebrew tap while keeping the formula version, source checksum, and release tag synchronized automatically.

# Current evidence

- The repository now contains a deterministic source-formula template and generator under `scripts/homebrew/`, plus local `make homebrew-formula` support.
- Version-tag pipelines test the workspace, render the formula from the immutable GitLab tag archive, and publish it to `research-enablement/homebrew-xylem`.
- The publisher handles both the currently empty tap and subsequent updates to its configured branch.
- The v1.0.0 archive was installed with the formula's Cargo command, its version test passed, and the rendered formula passed `brew style`.
- External activation still requires a protected `HOMEBREW_TAP_ACCESS_TOKEN` and a new version tag; tap-side `brew test-bot` validation remains follow-up work.

# Project code boundaries

- **Update:** `.gitlab-ci.yml`, `Makefile`, `README.md`, `CONTRIBUTING.md`, and release documentation.
- **Create:** a versioned formula template or deterministic generator, exporter tests, Homebrew audit/install smoke jobs, and a tag-only tap publication job.
- **Delete:** hand-maintained formula values or publication steps that can drift from the tagged release.

# Dependencies and ordering

The [CI and release pipeline](../refactors/ci-release-pipeline.md) must first produce immutable version tags, source archives, and SHA-256 checksums. This plan then exports that release contract to a dedicated tap. The [documentation and examples](../refactors/documentation-and-examples.md) plan documents the final tap name and verified installation commands after publication is stable.

# Recommendation

## Formula contract

- Start with a source formula for the `xylem` CLI and keep it in a dedicated, organization-owned tap. Treat prebuilt bottles as a later optimization once target runners and provenance are established.
- Generate `url`, `version`, and `sha256` from the immutable release tag and archive metadata; do not duplicate version literals in CI configuration.
- Follow the Rust installation pattern in the [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook): declare the Rust build dependency, stage Cargo dependencies, build with locked inputs, and install only the CLI binary.
- Add a formula `test do` block that runs `xylem --version` and a small local configuration smoke test without network access.

## Export and publication

- Add one deterministic command that renders the formula for an explicit tag and checksum, validates that the tag matches the workspace version, and fails when the generated file differs from its fixture.
- Have the successful tag pipeline create a reviewable pull or merge request in the tap repository instead of pushing directly to its default branch.
- Scope the tap credential to that repository and publication action. Never expose it to merge-request or branch pipelines.
- Publish only after all release artifacts and checksums exist, so a failed platform build cannot leave a formula pointing at a partial release.

## Homebrew validation

- Run `brew style`, `brew audit --strict`, installation from the generated formula, and `brew test` on a supported macOS runner.
- Add Linuxbrew coverage when the release support matrix declares Linux as a Homebrew tier.
- Use `brew test-bot` for tap CI once the tap exists; Homebrew documents its formula audit, build, install, test, and bottle workflow in [BrewTestBot](https://docs.brew.sh/BrewTestBot).
- Rehearse export against a temporary tap before enabling credentials or publishing a real release.

# Validation

- Generate the same formula twice from identical release metadata and assert byte-for-byte equality.
- Reject mismatched tags, package versions, missing checksums, mutable branch URLs, and unsupported targets.
- Install from the rendered formula in a clean Homebrew environment and run the formula test offline.
- Exercise publication in dry-run mode and assert that non-tag pipelines cannot access credentials or modify the tap.
- Upgrade from the previous formula version and verify that `brew upgrade xylem` selects the new immutable release.

# Acceptance criteria

- A documented `brew install <tap>/xylem` command installs the tagged Xylem release on every declared Homebrew platform.
- Formula version, URL, and checksum are generated from and agree with the immutable release metadata.
- Audit, style, install, and formula tests are required before the tap change can merge.
- Tap publication is tag-only, reviewable, retry-safe, and uses a least-privilege credential.
- A failed or partial release cannot update the tap.
- Maintainers can reproduce the generated formula locally with one documented command.

# Related plans

- [CI and release pipeline](../refactors/ci-release-pipeline.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
- [Documentation and examples](../refactors/documentation-and-examples.md)
