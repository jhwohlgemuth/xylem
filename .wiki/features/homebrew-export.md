---
type: Plan
title: "Activate the Homebrew release export"
description: "Activate and verify the implemented Homebrew publication path with protected credentials, required runners, and a new release."
difficulty: medium
impact: high
package: [workspace, xylem-cli]
tags: [homebrew, release, distribution, macos, linux, packaging]
status: in-progress
---

# Goal

Activate the implemented Homebrew publication path and verify that macOS and
Linux users can install and upgrade Xylem through the official tap.

# Current evidence

- Formula generation, tag-only tap merge-request creation, tap CI, offline formula testing, and upgrade coverage are implemented.
- The rendered formula passes `brew style` and `brew audit --strict`.
- The immutable v1.0.0 archive cannot compile with Homebrew Rust 1.98 because it locks `ethnum` 1.5.2; activation must use a new release containing the compatible lockfile.
- Activation still requires a protected `HOMEBREW_TAP_ACCESS_TOKEN`, protected release tags, required tap merge-request pipelines, compatible `cpu` and `macos` runners, and a new version tag.

# Project code boundaries

- **Update:** `README.md`, `CONTRIBUTING.md`, and release documentation after the first successful publication.
- **Create:** protected project variables, runner assignments, and merge checks in GitLab project and tap settings.
- **Delete:** None.

# Dependencies and ordering

The [CI and release pipeline](../refactors/ci-release-pipeline.md) must produce the new immutable version tag and artifacts. The [documentation and examples](../refactors/documentation-and-examples.md) plan records the verified installation command after activation.

# Recommendation

1. Configure a protected, least-privilege `HOMEBREW_TAP_ACCESS_TOKEN` and protect release tags.
2. Assign compatible Linux and macOS runners to the tap project and require both validation jobs before merge.
3. Create a new version tag from the compatible lockfile and verify that the release pipeline opens the expected tap merge request.
4. Require the tap pipeline, review and merge the generated formula, then verify fresh installation and upgrade on both declared platforms.
5. Publish the tested tap command and operational recovery steps.

# Validation

- Confirm the protected credential is unavailable to branch and merge-request pipelines.
- Confirm an incomplete or failed release cannot open or update the tap merge request.
- Install from the rendered formula in clean macOS and Linux Homebrew environments and run the formula test offline.
- Upgrade from the previous formula version and verify that `brew upgrade xylem` selects the new immutable release.

# Acceptance criteria

- A documented `brew install <tap>/xylem` command installs the tagged Xylem release on every declared Homebrew platform.
- Audit, style, install, and formula tests are required before the tap change can merge.
- The first tap update is created from a new immutable tag, passes both platform jobs, and merges through review.
- The publication credential is protected and scoped only to the tap update operation.

# Related plans

- [CI and release pipeline](../refactors/ci-release-pipeline.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
- [Documentation and examples](../refactors/documentation-and-examples.md)
