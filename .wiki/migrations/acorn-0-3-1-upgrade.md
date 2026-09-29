---
type: Plan
title: "Upgrade ACORN dependencies to 0.3.1"
description: "Migrate Xylem from the pinned ACORN 0.2.0 revision to ACORN 0.3.1 while retaining only the capabilities Xylem uses."
difficulty: high
impact: critical
package: [workspace, xylem-lib, xylem-cli]
tags: [migration, acorn, dependencies, features, security, build]
status: planned
---

# Execution

Xylem currently pins `acorn-cmd`, `acorn-core`, `acorn-host`, `acorn-lib`, and
`acorn-schema` to Git revision `dde4d6bf851bf0b8d4ab3fd902d357e5d18d9645`.
The lockfile identifies those packages as 0.2.0. This migration moves all ACORN
dependencies together to 0.3.1 and removes capabilities that have no Xylem call
site.

The work stays in the dependency lane. Coordinate changes to `Cargo.toml` and
`Cargo.lock` through the manifest lock.

# Goal

Move Xylem from the pinned ACORN 0.2.0 source to the published 0.3.1 contract.
Keep the existing command, location, checksum, filesystem, and validation
behavior without retaining ACORN's analysis dependency graph.

# Current evidence

- The workspace uses one Git revision for all five ACORN packages; `Cargo.lock`
  records each package at 0.2.0.
- `acorn-core` enables only `alloc`, and `acorn-host` enables only `fs`.
- Xylem calls `CommandSpec`, `cmd!`, `Location`, `Scheme`, typed checksums,
  filesystem helpers, and schema validation directly from their owning crates.
- No source file imports `acorn-lib`, but both Xylem packages depend on it and
  the workspace enables its `analysis` feature.
- The `analysis` feature keeps analyzer-only dependencies in the lockfile even
  though Xylem has no analyzer call site.

# Project code boundaries

- **Update:** workspace and package manifests, `Cargo.lock`, ACORN imports or
  call sites changed by 0.3.1, and focused compatibility tests.
- **Create:** a narrow Xylem-owned adapter only when more than one call site
  needs the same 0.3.1 conversion.
- **Delete:** the unused `acorn-lib` dependency and any compatibility code made
  obsolete by 0.3.1.

# Dependencies and ordering

This is the first migration in the dependency lane. It precedes
[secure module acquisition](../refactors/secure-module-acquisition.md) and the
final [dependency security gates](../refactors/dependency-surface-and-security-gates.md).
Preserve the landed YAML parser dependency changes while editing the manifests.

# Implementation

1. Record the current feature tree, duplicate packages, package count, clean
   build time, and advisory results.
2. Change all ACORN workspace dependencies to version 0.3.1 in one manifest
   edit. Keep `default-features = false` and retain only capabilities required
   by an identified Xylem call site.
3. Remove `acorn-lib` from the workspace and package manifests unless the 0.3.1
   migration identifies a concrete, tested use.
4. Compile before adapting call sites. For each error, use the owning 0.3.1
   crate and public API instead of recreating a removed re-export.
5. Preserve explicit checksum algorithms, non-interactive Git behavior,
   environment-aware command execution, and SSH classification as a Git
   transport.
6. Regenerate `Cargo.lock` and confirm every ACORN package resolves to 0.3.1
   from the selected registry source. Do not leave a Git or path override.
7. Run the workspace tests, Clippy, documentation build, feature-tree audit,
   duplicate-version audit, and security checks.
8. Record the before-and-after dependency measurements in the implementation
   change or release notes.

# Compatibility checkpoints

| Current use | ACORN 0.3.1 checkpoint |
|---|---|
| `acorn-cmd::{CommandSpec, args, cmd}` | Preserve argument boundaries, working directories, environment injection, and captured status/output. |
| `acorn-core::{Location, Scheme}` | Import from the owning portable crate; keep SSH handling in the Git acquisition path. |
| `acorn-core` checksum and validation types | Preserve explicit algorithms, normalized comparisons, and path-qualified diagnostics. |
| `acorn-host::fs` helpers | Retain only the filesystem feature needed for reads, checksums, and executable permissions. |
| `acorn-schema::validation` | Preserve aggregated semantic validation and stable error paths. |
| unused `acorn-lib` analysis feature | Remove the dependency and confirm analyzer-only crates leave the graph. |

# Validation

- Compile every workspace package with the selected ACORN 0.3.1 feature set
  and no default features.
- Run `cargo tree -e features` and `cargo tree -d`; confirm analyzer-only crates
  are absent unless a documented Xylem call requires them.
- Run the full test suite and focused command, location, checksum, filesystem,
  and validation regression tests on supported platforms.
- Run `cargo audit` and `cargo deny check` against the regenerated lockfile and
  assign an owner to any remaining advisory.
- Verify an offline locked build and confirm the workspace Rust version is
  compatible with ACORN 0.3.1.

# Acceptance criteria

- Every ACORN package resolves to 0.3.1 from the committed dependency source.
- Xylem compiles without legacy ACORN shims or temporary source overrides.
- `acorn-lib` and analyzer-only dependencies are absent unless a concrete,
  tested call requires them.
- Command execution, locations, checksums, filesystem operations, and
  validation retain focused regression coverage.
- The dependency graph, package-count delta, duplicate versions, build-time
  comparison, and advisory results are recorded.

# Related plans

- [Secure module acquisition](../refactors/secure-module-acquisition.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
