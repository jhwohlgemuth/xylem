---
type: Plan
title: "Align documentation, examples, and repository hygiene"
description: "Make the public documentation and checked-in examples accurately describe Xylem 1.x behavior, support policy, commands, and configuration."
difficulty: medium
impact: medium
package: [workspace, xylem-lib, xylem-cli]
tags: [documentation, examples, roadmap, architecture, hygiene]
status: planned
---

# Goal

Give users and contributors one accurate path from installation through a local, offline example, and remove generated or stale artifacts that contradict the maintained product contract.

# Current evidence

- No example has automated configuration validation, and several configurations still use absolute placeholders such as `file:///path/to/module-a` and `/data/test-cases`.
- There is no self-contained offline smoke example a new user can run from a clean checkout without editing paths.
- The generated `lcov.info` coverage artifact remains tracked and absent from `.gitignore`.
- `xylem-lib/Cargo.toml` still comments out the workspace `missing-docs = "deny"` policy, so public API documentation is unenforced.
- `CONTRIBUTING.md` still needs checking against the current CLI and `Makefile`, and README install verification still needs to match the generated `xylem --help` interface.

# Project code boundaries

- **Update:** `README.md`, `CONTRIBUTING.md`, `.gitignore`, `CITATION.cff` if metadata is stale, crate docs, and every maintained example.
- **Create:** one self-contained offline smoke example, automated example validation, a configuration-reference entry point, and generated-help/doc validation where appropriate.
- **Delete:** stale generated coverage output if the project chooses CI artifacts instead, dead documentation claims, and obsolete empty examples rather than leaving placeholders.

# Dependencies and ordering

Inventory can begin immediately. Final behavior descriptions follow the accepted error, configuration, environment, reporter, dependency, and [CI/release](ci-release-pipeline.md) contracts.

# Recommendation

1. Declare current scope and support tiers separately from aspirational roadmap items.
2. Generate or snapshot CLI help and configuration examples so argument names and aliases cannot silently drift.
3. Make at least one example run offline from a clean checkout using only ubiquitous test helpers or a checked-in tiny script.
4. Validate every example configuration through the same parser and semantic validator used in production.
5. Enable workspace lint inheritance consistently and either document public APIs or intentionally scope the lint, rather than keeping a commented-out package override.
6. Ignore and remove generated caches and coverage artifacts while retaining intentional fixtures.

# Validation

- Run Markdown link checking and spell/style checks against local and public links.
- Parse and validate every example JSON/YAML file in tests.
- Execute the smoke example in offline and dry-run modes from a clean temporary checkout.
- Compare documented CLI help with `xylem --help` and `xylem run --help` snapshots.
- Render the architecture diagram and build Rust documentation with warnings denied.
- Verify ignored generated files do not reappear after tests, examples, or coverage runs.

# Acceptance criteria

- Architecture, roadmap, support tiers, CLI commands, Make targets, and release instructions match the repository.
- Every retained example has a purpose, README, valid configuration, and automated validation.
- A new user can run one offline example without editing absolute placeholder paths.
- Generated Python caches and local coverage artifacts are not tracked.
- Documentation links resolve and generated help is protected against drift.
- Public API documentation policy is explicit and enforced consistently.

# Related plans

- [CI and release pipeline](ci-release-pipeline.md)
