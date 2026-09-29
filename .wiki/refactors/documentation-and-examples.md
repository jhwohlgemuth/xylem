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

Rechecked against `005aa73`.

- Landed: `ARCHITECTURE.md` now describes the actual two-crate workspace, the journal, and `RunSummary`, and `docs/src/architecture.md` carries the library/CLI split and the append-only journal contract.
- Landed: `MODULE_SPEC.md` is a real normative document with a field table, a reserved-variable table, environment-manager rules, and a conformance checklist, and `docs/src/` documents `options.batch`, `options.allowed`, the reporters, the journal, and exit status.
- Landed: `ROADMAP.md` no longer claims URL allowlisting is incomplete; the `[x]` on parameterized allowlists and reachability checks now matches `RemotePolicy`.
- Open: no example has automated configuration validation, and several example configurations still point at absolute placeholders such as `file:///path/to/module-a` and `/data/test-cases`.
- Open: there is no self-contained offline smoke example a new user can run from a clean checkout without editing paths.
- Open: two Python bytecode files are still tracked at `examples/python-with-conda/lib/__pycache__/`. `.gitignore` already ignores `__pycache__/`, so the fix is `git rm --cached`, not a new ignore rule.
- Open: the generated `lcov.info` coverage artifact is still tracked and still absent from `.gitignore`.
- Open: `xylem-lib/Cargo.toml` still comments out `missing-docs = "deny"` that the workspace declares, so public API documentation is unenforced.
- Open: `CONTRIBUTING.md` still needs checking against the current CLI and `Makefile`, and README install verification still needs to match the generated `xylem --help` interface.

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

- [CI and release pipeline](ci-release-pipeline.md)
