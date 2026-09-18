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

- `ARCHITECTURE.md` references CLI commands, reporters, directories, and a Python package that do not exist in the current tree; its Mermaid graph includes an unconnected Python node.
- `ROADMAP.md` marks URL allowlisting and multi-platform releases complete even though acquisition contains allowlist TODOs and current CI does not build macOS.
- The roadmap says workflow environment variables are unfinished while production code mutates them globally, and it mixes old `pipe` names with Xylem.
- `CONTRIBUTING.md` documents `check`, `make test-cross`, coverage behavior, and line references that do not match the current CLI or Makefile.
- README links use inconsistent documentation hosts and verify installation with `xylem help` rather than the conventional generated `xylem --help` interface.
- Several example README files are empty, most configuration paths are placeholders, and there is no validated self-contained smoke example.
- Python `__pycache__` bytecode is checked into `examples/`, while `.gitignore` does not ignore Python caches or the generated root `lcov.info` coverage artifact.
- Public library items have sparse documentation even though the workspace declares `missing-docs = "deny"`; `xylem-lib` locally comments that lint out.

# Project code boundaries

- **Update:** `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `.gitignore`, `CITATION.cff` if metadata is stale, crate docs, and every maintained example.
- **Create:** one self-contained offline smoke example, a configuration-reference entry point, and generated-help/doc validation where appropriate.
- **Delete:** tracked Python cache files, stale generated coverage output if the project chooses CI artifacts instead, dead documentation claims, and obsolete empty examples rather than leaving placeholders.

# Dependencies and ordering

Inventory can begin immediately. Final behavior descriptions follow the accepted error, configuration, environment, reporter, dependency, and [CI/release](ci-release-pipeline.md) contracts.

# Recommendation

1. Declare current scope and support tiers separately from aspirational roadmap items.
2. Rewrite architecture from the actual two-crate workspace and its parse/validate/plan/execute boundaries.
3. Generate or snapshot CLI help and configuration examples so argument names and aliases cannot silently drift.
4. Make at least one example run offline from a clean checkout using only ubiquitous test helpers or a checked-in tiny script.
5. Validate every example configuration through the same parser and semantic validator used in production.
6. Reconcile roadmap completion boxes against executable evidence; move speculative items out of release sections that already shipped.
7. Enable workspace lint inheritance consistently and either document public APIs or intentionally scope the lint, rather than keeping a commented-out package override.
8. Ignore and remove generated caches/artifacts while retaining intentional fixtures.

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

- [Configuration validation and dry-run](configuration-validation-and-dry-run.md)
- [Structured run reporters](../features/run-reporters.md)
- [CI and release pipeline](ci-release-pipeline.md)
