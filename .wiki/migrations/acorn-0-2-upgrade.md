---
type: Plan
title: "Upgrade and narrow ACORN dependencies"
description: "Migrate Xylem from acorn-lib 0.1.50 to the ACORN 0.2 contracts while enabling only required filesystem, network, and utility capabilities."
difficulty: high
impact: critical
package: [workspace, xylem-lib, xylem-cli]
tags: [migration, acorn, dependencies, features, security, build]
status: planned
---

# Execution

Partially landed at baseline `005aa73`: the workspace is on `acorn-core`, `acorn-lib`, and `acorn-schema` 0.2.0 with `Location` and `Scheme` imported from the owning portable crate, so the version migration and its compatibility checkpoints are done. The feature narrowing is not: `acorn-lib` still enables `analysis` and the lockfile still resolves the analyzer stack, including Polars.

The remaining work stays in the dependency lane. The completed
[resumable execution](../features/resumable-execution.md) work no longer holds
the manifest lock.

# Goal

Move Xylem off `acorn-lib` 0.1.50 and onto the current ACORN 0.2 contract represented by `../acorn`, without retaining the full `std` plus `analyzer` dependency graph for a handful of host utilities.

# Current evidence

Rechecked against `005aa73`. The version migration is done and the feature narrowing is not.

- Landed: the workspace resolves `acorn-core`, `acorn-lib`, and `acorn-schema` 0.2.0 from the registry, so the pinned-revision and temporary-override concerns are closed.
- Landed: `Location` and `Scheme` are imported from the owning portable crate, with no legacy root re-export or compatibility shim in Xylem.
- Landed: `file_checksum` is called with an explicit algorithm and compared as a typed `Checksum`, and downloads are awaited at the orchestration boundary with no ad-hoc Tokio runtime.
- Landed: the command environment sync closed the `std::process::Command` fallback, so `run_output_with_env` in ACORN and `cmd!` in Xylem are the only execution path.
- Open: `Cargo.toml` still declares `acorn-lib = { version = "0.2.0", features = ["analysis"] }`, and `Cargo.lock` still resolves the analyzer stack, including `bat` and `polars`. Nothing in Xylem calls an analyzer.
- Open: `acorn-core` enables `alloc` only, which is already minimal, but the `analysis` feature on `acorn-lib` is the single largest remaining source of unused graph.
- Open: the advisory and package-count deltas this plan asked to record were never written down. `make audit` and `cargo deny check` still run, and `deny.toml` still carries six permanent advisory ignores.

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
