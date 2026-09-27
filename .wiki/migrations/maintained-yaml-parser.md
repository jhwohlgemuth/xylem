---
type: Plan
title: "Replace the unmaintained YAML parser"
description: "Move configuration parsing from serde_yml and libyml to a maintained YAML implementation with compatibility and safety tests."
difficulty: medium
impact: critical
package: [xylem-lib, workspace]
tags: [migration, yaml, serde, security, configuration]
status: complete
---

# Execution

This plan is executed as Phase 1 of [resumable execution](../features/resumable-execution.md), which owns the whole prerequisite chain. Do not schedule it separately. Its `status` flips when that phase lands.

# Goal

Preserve supported Xylem YAML configuration semantics while removing the unmaintained and unsound `serde_yml`/`libyml` stack and its permanent audit exceptions.

# Current evidence

- `xylem-lib` now uses `serde_norway = 0.9.42`, matching ACORN's resolved parser.
- `Config::read_json` and `Config::read_yaml` return `ConfigReadError` with format, path, and source-chain context.
- JSON and YAML share empty-file and strict unknown-field policy; YAML supports aliases and merge keys before typed deserialization.
- `serde_yml`, `libyml`, and their RUSTSEC-2025-0067/RUSTSEC-2025-0068 exceptions are absent.
- Unrelated transitive advisories remain owned by the dependency-security phase and do not change this migration's YAML-specific result.

# Project code boundaries

- **Update:** `Cargo.toml`, `Cargo.lock`, `xylem-lib/Cargo.toml`, `xylem-lib/src/config/mod.rs`, YAML fixtures and parser tests, `deny.toml`, and `Makefile`.
- **Create:** format-neutral configuration parse-error fixtures covering syntax, type, and semantic failures.
- **Delete:** the direct `serde_yml` dependency and the RUSTSEC-2025-0067/RUSTSEC-2025-0068 ignore entries once absent from the graph.

# Dependencies and ordering

Coordinate manifest integration with the [ACORN upgrade](acorn-0-2-upgrade.md). The parser migration precedes the final error mapping in [configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md) and the final dependency audit.

# Recommendation

1. Keep `serde_norway` aligned with ACORN and retain fixture parity and advisory coverage.
2. Keep third-party parser types behind `ConfigReadError`, including path, format, and source-chain context.
3. Preserve the documented alias, merge-key, scalar, duplicate-key, single-document, and resource-limit behavior.
4. Keep missing-file, empty-file, unknown-field, and semantic deserialization policy aligned across JSON and YAML.
5. Do not restore the retired advisory exceptions or replace them with a blanket ignore.

# Validation

- Parse every checked-in YAML fixture with the old and candidate parser in a temporary comparison test and review intentional differences.
- Add paired JSON/YAML tests that deserialize to equal `Config` values for the supported subset.
- Add malformed, duplicate-key, multi-document, alias, and resource-limit tests.
- Run `cargo audit --no-fetch --json` and `cargo deny check`; verify the two retired crates and their ignores are absent. Track unrelated audit findings in the dependency-security phase.
- Run the full workspace tests and documentation tests.

# Acceptance criteria

- No resolved dependency is `serde_yml` or `libyml`.
- YAML and JSON use one owned error taxonomy and one semantic validation path.
- Supported YAML behavior is documented and protected by fixtures.
- Duplicate or ambiguous configuration input fails with an actionable path-aware diagnostic.
- RUSTSEC-2025-0067 and RUSTSEC-2025-0068 are not ignored because the affected crates are no longer present.

# Related plans

- [Typed errors and outcomes](../refactors/typed-error-and-outcome-boundary.md)
- [Configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)
