---
type: Plan
title: "Replace the unmaintained YAML parser"
description: "Move configuration parsing from serde_yml and libyml to a maintained YAML implementation with compatibility and safety tests."
difficulty: medium
impact: critical
package: [xylem-lib, workspace]
tags: [migration, yaml, serde, security, configuration]
status: planned
priority: next
---

# Goal

Preserve supported Xylem YAML configuration semantics while removing the unmaintained and unsound `serde_yml`/`libyml` stack and its permanent audit exceptions.

# Current evidence

- `xylem-lib` directly depends on `serde_yml = 0.0.12`, and `Config::read_yaml` exposes its error type in a public signature.
- `deny.toml` ignores RUSTSEC-2025-0067 and RUSTSEC-2025-0068 for `libyml` and `serde_yml`; `Makefile` repeats those exceptions for `cargo audit`.
- The current audit classifies both crates as unsound with no patched version.
- The adjacent ACORN workspace already uses `serde_norway`, providing a nearby compatibility reference and an opportunity to converge parsers.
- JSON and YAML paths currently differ in empty-file handling and labels, and both discard file-read context before the top-level process exits.

# Project code boundaries

- **Update:** `Cargo.toml`, `Cargo.lock`, `xylem-lib/Cargo.toml`, `xylem-lib/src/config/mod.rs`, YAML fixtures and parser tests, `deny.toml`, and `Makefile`.
- **Create:** format-neutral configuration parse-error fixtures covering syntax, type, and semantic failures.
- **Delete:** the direct `serde_yml` dependency and the RUSTSEC-2025-0067/RUSTSEC-2025-0068 ignore entries once absent from the graph.

# Dependencies and ordering

Coordinate manifest integration with the [ACORN upgrade](acorn-0-2-upgrade.md). The parser migration precedes the final error mapping in [configuration validation and dry-run](../refactors/configuration-validation-and-dry-run.md) and the final dependency audit.

# Recommendation

1. Evaluate `serde_norway` first because ACORN already uses it, but accept it only after Xylem fixture parity and advisory checks pass.
2. Stop exposing a third-party parser error as Xylem's public API. Return the owned configuration error introduced by [typed errors and outcomes](../refactors/typed-error-and-outcome-boundary.md), including path, format, and source-chain context.
3. Define and test supported YAML behavior: aliases, merge keys, quoted scalars, nulls, booleans, numeric variables, duplicate keys, and multi-document input. Reject ambiguous or unsupported constructs explicitly.
4. Apply identical missing-file, empty-file, unknown-field, and semantic-validation policy to JSON and YAML after deserialization.
5. Remove advisory exceptions rather than renaming them or replacing them with an indefinite blanket ignore.

# Validation

- Parse every checked-in YAML fixture with the old and candidate parser in a temporary comparison test and review intentional differences.
- Add paired JSON/YAML tests that deserialize to equal `Config` values for the supported subset.
- Add malformed, duplicate-key, multi-document, alias, and resource-limit tests.
- Run `cargo audit --no-fetch --json` and `cargo deny check`; verify the two retired crates and their ignores are absent.
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
