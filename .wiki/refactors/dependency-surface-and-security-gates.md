---
type: Plan
title: "Minimize dependencies and enforce security gates"
description: "Remove unused direct dependencies, converge duplicate versions, and replace accumulating advisory ignores with an owned remediation policy."
difficulty: medium
impact: critical
package: [workspace, xylem-lib, xylem-cli]
tags: [dependencies, cargo, audit, deny, supply-chain, ci]
status: planned
---

# Goal

Make the resolved dependency graph proportional to Xylem's implemented capabilities and keep it continuously reviewable. Required CI must fail on new reachable vulnerabilities, unsound crates, yanked crates, and policy drift unless a time-bounded exception has an owner and rationale.

# Current evidence

Rechecked against `005aa73`.

- `acorn-lib` still enables its `analysis` feature, so the lockfile still resolves `bat`, `polars`, and the rest of the analyzer stack even though nothing in Xylem calls an analyzer.
- Confirmed unused direct dependencies: `uriparse` in `xylem-cli`, and
  `valuable` in `xylem-lib`, which imports only `valuable_derive` for the
  `Valuable` derive on `ContainerRuntime` and `ModuleType`. The completed
  resumable-execution work moved `glob` to its active library owner; the
  remaining entries still need an owner.
- `assert_cmd` is declared in both crates, but only `xylem-cli/tests/workflow.rs` uses it. The `xylem-lib` dev-dependency is unused.
- The plan's claim that `sha2` and `tokio` are declared in `xylem-lib` is no longer true; neither appears in that manifest. `bytes` and `reqwest` are declared and both are used by `remote.rs`.
- The lockfile still carries duplicate generations of `console` (0.15 and 0.16), `convert_case` (0.10 and 0.11), `indicatif` (0.17 and 0.18), and `reqwest` (0.12 and 0.13). Xylem declares the older generation of each.
- Phase 1 removed `serde_yml`, `libyml`, and their two unsoundness exceptions. The remaining permanent advisory ignores and extra `cargo audit` exceptions still need the policy defined here.
- `make audit` runs `cargo udeps`, `cargo deny check`, and `cargo audit`, but `.gitlab-ci.yml` still does not invoke it, so the gate is advisory only.
- Package-count, build-time, and advisory baselines are still unrecorded.

# Project code boundaries

- **Update:** root and crate manifests, `Cargo.lock`, `deny.toml`, `Makefile`, `.gitlab-ci.yml`, and dependency-policy documentation.
- **Create:** a small exception ledger or structured comments containing advisory ID, reachability, owner, expiry/review date, and removal condition; optional graph-size baseline script.
- **Delete:** unused direct dependencies, redundant version declarations, retired advisory ignores, and tools from required checks that have no configured purpose.

# Dependencies and ordering

Finalize after the [ACORN migration](../migrations/acorn-0-3-1-upgrade.md); the
YAML migration has landed. Its commands are prerequisites for the [CI/release
pipeline](ci-release-pipeline.md).

# Recommendation

1. Run source-aware unused-dependency tooling and manually verify macro/build/test-only cases before removal.
2. Move every shared direct dependency to `[workspace.dependencies]`; crate manifests should opt in with `workspace = true` and explicit features.
3. Remove dependencies that have no live owner. Reintroduce one only with a concrete call site and feature rationale.
4. Narrow default features and ACORN capabilities, then compare package count, duplicate versions, native build dependencies, binary size, and clean build time.
5. Update patch-compatible vulnerable packages in the lockfile. For transitive blockers, upgrade or remove the owning top-level capability rather than adding blanket ignores.
6. Make vulnerability and unsoundness failures required. Treat unmaintained and duplicate packages with a documented threshold and ratchet it downward.
7. Permit an exception only when it records reachability analysis, owner, review date, upstream issue, and exact removal condition. CI verifies exceptions remain present and unexpired.
8. Keep security tooling non-mutating. Formatting and Clippy fixes belong to developer commands, not audit jobs.

# Validation

- Run unused-dependency analysis, `cargo tree -d`, feature trees for both packages, `cargo audit`, and `cargo deny check` from a clean lockfile.
- Compare resolved package count, clean check/build time, and release binary size against the recorded baseline.
- Verify `cargo check --locked`, `cargo test --locked`, and the supported target matrix.
- Add a CI test proving a new unexcepted advisory or expired exception fails the job.
- Confirm every remaining direct dependency has at least one owning production, build, or test call site.

# Acceptance criteria

- No unused direct dependency remains.
- Shared versions and features have one workspace owner.
- Analyzer/Polars and other unused ACORN capabilities are absent from Xylem's graph.
- Required CI runs locked audit and policy checks.
- No indefinite advisory ignore remains; every exception is scoped, owned, justified, and time-bounded.
- Active reachable vulnerabilities are patched or removed, and any external blocker is explicit rather than hidden by `make audit` flags.
- Graph and build-size baselines are documented for future regression checks.

# Related plans

- [ACORN 0.3.1 upgrade](../migrations/acorn-0-3-1-upgrade.md)
- [CI and release pipeline](ci-release-pipeline.md)
