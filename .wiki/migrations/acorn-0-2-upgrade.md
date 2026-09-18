---
type: Plan
title: "Upgrade and narrow ACORN dependencies"
description: "Migrate Xylem from acorn-lib 0.1.50 to the ACORN 0.2 contracts while enabling only required filesystem, network, and utility capabilities."
difficulty: high
impact: critical
package: [workspace, xylem-lib, xylem-cli]
tags: [migration, acorn, dependencies, features, security, build]
status: planned
priority: next
---

# Goal

Move Xylem off `acorn-lib` 0.1.50 and onto the current ACORN 0.2 contract represented by `../acorn`, without retaining the full `std` plus `analyzer` dependency graph for a handful of host utilities.

# Current evidence

- The workspace pins `acorn-lib = 0.1.50` with `default-features = false` but enables `std` and `analyzer`.
- Xylem uses ACORN for `Label`, file/path helpers, download/checksum/executable helpers, and `Location`/`Scheme`; it does not call ACORN analyzers.
- `cargo tree -e features -p xylem-lib` resolves ACORN's analysis stack, including Polars, Bat, Lychee, two Reqwest generations, and their transitive native/TLS dependencies.
- The adjacent ACORN workspace is version 0.2.0 with Rust 1.96.0. Its portable `Location` and `Scheme` types are owned by `acorn-core`; `download_binary` is async; `file_checksum` accepts an algorithm and returns a typed checksum; and `Scheme` no longer models SSH as a general content location.
- The current lockfile's oversized ACORN graph contributes multiple active advisories. The migration must measure the resulting graph rather than assuming a version bump alone clears them.

# Project code boundaries

- **Update:** `Cargo.toml`, `Cargo.lock`, `xylem-lib/Cargo.toml`, `xylem-cli/Cargo.toml`, ACORN imports and call sites in `xylem-lib/src/{lib,config/mod,script/mod}.rs` and `xylem-cli/src/commands/run/mod.rs`, plus focused tests.
- **Create:** a narrow Xylem-owned adapter module only if multiple call sites require the same ACORN compatibility conversion.
- **Delete:** obsolete `analyzer` feature activation, legacy imports/re-exports, and direct dependencies made unnecessary by the new ACORN crates or adapters.

# Dependencies and ordering

This is the first migration in the dependency lane. It precedes [secure module acquisition](../refactors/secure-module-acquisition.md) and final [dependency security gates](../refactors/dependency-surface-and-security-gates.md). Coordinate its manifest edits with the [YAML migration](maintained-yaml-parser.md) through the manifest lock.

# Recommendation

1. Establish whether ACORN 0.2.0 is published for every required crate. Use a temporary path override to `../acorn` for compatibility work, but commit registry dependencies unless the project explicitly chooses a workspace/source dependency policy.
2. Depend directly on `acorn-core` for portable locations if required by the public 0.2 API. Do not ask `acorn-lib` to recreate removed root re-exports.
3. Replace `std` plus `analyzer` with the smallest tested feature set. Expected host capabilities are `std-core`, `cli-util`, and `net`; confirm every feature with `cargo tree -e features` rather than enabling `std` as a shortcut.
4. Adapt downloads to async at the orchestration boundary or select a Xylem-owned client. Do not create an ad-hoc Tokio runtime inside a library helper.
5. Compare typed checksum values explicitly and preserve the algorithm in configuration and diagnostics.
6. Classify SSH only as a Git transport in the acquisition layer. Do not reintroduce it into the general file/HTTP `Scheme` model.
7. Declare the workspace Rust version explicitly and test it. If ACORN 0.2 requires Rust 1.96.0, make that requirement visible in package metadata and CI.
8. Regenerate `Cargo.lock`, inspect duplicate versions and native build dependencies, and record before/after package count and clean build time.

# Compatibility checkpoints

| Existing use | ACORN 0.2 decision |
|---|---|
| `acorn::{Location, Scheme}` | Import from the owning portable crate or a documented public re-export. |
| synchronous `download_binary` | Await from orchestration or replace with a Xylem acquisition port. |
| string checksum comparison | Compare normalized typed SHA-256 values and report the algorithm. |
| `Scheme::SSH` | Treat only as a Git transport after repository validation. |
| `std` + `analyzer` features | Replace with a measured minimal capability set. |

# Validation

- Compile each package with the selected feature set and no default features.
- Run `cargo tree -e features`, `cargo tree -d`, and a clean release build; confirm Polars and analyzer-only crates are absent unless a documented Xylem call requires them.
- Run the full hermetic test suite and ACORN adapter tests on Linux, macOS, and the supported Windows target.
- Run `cargo audit` and `cargo deny check` against the regenerated lockfile; classify any remaining advisory by reachable capability and remediation owner.
- Verify offline builds from the lockfile and the declared MSRV.

# Acceptance criteria

- All ACORN packages use a compatible 0.2 release and Xylem compiles without legacy API shims in ACORN.
- Xylem enables no analyzer capability unless a concrete analyzer call is present and tested.
- Location, download, checksum, and executable behavior has focused regression coverage.
- The workspace declares and tests its Rust-version requirement.
- The final dependency graph, package-count delta, duplicate versions, and advisory results are recorded.
- No temporary path override remains in committed manifests unless source dependencies are an explicit project policy.

# Related plans

- [Secure module acquisition](../refactors/secure-module-acquisition.md)
- [Dependency surface and security gates](../refactors/dependency-surface-and-security-gates.md)
- [Hermetic workflow tests](../testing/hermetic-workflow-tests.md)
