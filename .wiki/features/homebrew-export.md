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

- The repository uses one deterministic formula generator and one formula template under `scripts/homebrew/`, plus local `make homebrew-formula` support. Formula checks run directly in GitLab CI instead of through additional helper programs.
- Release binaries are uploaded before the release tag is created. The tag pipeline verifies those immutable artifacts, renders the source formula, and submits a `release/xylem-vMAJOR.MINOR.PATCH` merge request to `research-enablement/homebrew-xylem`.
- The tag job handles both the currently empty tap and subsequent updates without pushing formula changes directly to the protected default branch.
- The formula stages Cargo dependencies before its offline install and tests both `xylem --version` and a local, offline workflow configuration.
- The tap configuration runs `brew style`, `brew audit --strict`, source installation, `brew test`, and upgrade coverage directly on macOS and Linux before merge. Because Homebrew limits `brew test-bot` to GitHub Actions, the GitLab tap invokes the same lifecycle commands directly.
- The rendered formula passes current `brew style` and `brew audit --strict`. The immutable v1.0.0 archive no longer compiles with Homebrew Rust 1.98 because it locked `ethnum` 1.5.2; the current repository locks the compatible 1.5.3 release and passes a full workspace check with Rust 1.98, so activation must use a new release tag rather than republish v1.0.0.
- External activation requires a protected `HOMEBREW_TAP_ACCESS_TOKEN`, protected release tags, required tap merge-request pipelines, compatible `cpu` and `macos` runners, and a new version tag.

# Project code boundaries

- **Update:** `.gitlab-ci.yml`, `Makefile`, `README.md`, `CONTRIBUTING.md`, and release documentation.
- **Create:** a versioned formula template or deterministic generator, exporter tests, Homebrew audit/install smoke jobs, and a tag-only tap publication job.
- **Delete:** hand-maintained formula values or publication steps that can drift from the tagged release.

# Dependencies and ordering

The [CI and release pipeline](../refactors/ci-release-pipeline.md) must first produce immutable version tags, source archives, and SHA-256 checksums. This plan then exports that release contract to a dedicated tap. The [documentation and examples](../refactors/documentation-and-examples.md) plan documents the final tap name and verified installation commands after publication is stable.

# Recommendation

## Formula contract

- Start with a source formula for the `xylem` CLI and keep it in a dedicated, organization-owned tap. Treat prebuilt bottles as a later optimization once target runners and provenance are established.
- Generate the immutable release `url` and `sha256` from tag metadata and let Homebrew infer `version` from that URL; do not duplicate a redundant version field or literals in CI configuration.
- Follow the Rust installation pattern in the [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook): declare the Rust build dependency, stage Cargo dependencies, build with locked inputs, and install only the CLI binary.
- Add a formula `test do` block that runs `xylem --version` and a small local configuration smoke test without network access.

## Export and publication

- Add one deterministic command that renders the formula for an explicit tag and checksum and validates that the tag matches the workspace version. Keep determinism and drift comparisons in CI.
- Have the successful tag pipeline create a reviewable pull or merge request in the tap repository instead of pushing directly to its default branch.
- Scope the tap credential to that repository and publication action. Never expose it to merge-request or branch pipelines.
- Publish only after all release artifacts and checksums exist, so a failed platform build cannot leave a formula pointing at a partial release.

## Homebrew validation

- Run `brew style`, `brew audit --strict`, installation from the generated formula, and `brew test` on a supported macOS runner.
- Add Linuxbrew coverage when the release support matrix declares Linux as a Homebrew tier.
- Run the `brew test-bot` lifecycle directly in GitLab tap CI; the `brew test-bot` command itself supports GitHub Actions only.
- Keep publication commands in the tag-only CI job so the release gate and credential boundary are visible in one place.

# Validation

- Generate the same formula twice from identical release metadata and assert byte-for-byte equality.
- Reject mismatched tags, package versions, missing checksums, mutable branch URLs, and unsupported targets.
- Install from the rendered formula in a clean Homebrew environment and run the formula test offline.
- Assert deterministic output and rejection of invalid versions, checksums, and formula drift in the formula test job.
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
