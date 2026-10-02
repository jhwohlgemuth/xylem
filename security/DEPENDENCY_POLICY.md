# Dependency policy

Xylem keeps its production dependency graph small and treats changes to that
graph as security-sensitive. New dependencies need a concrete runtime or
development use, disabled default features where practical, and review of the
resulting `Cargo.lock` diff.

The ACORN integration is intentionally limited to `acorn-cmd`, `acorn-core`,
`acorn-host`, `acorn-macros`, and `acorn-schema`, all pinned to one revision.
`acorn-lib`, Polars, and OpenSSL bindings are prohibited because Xylem does not
use their functionality. The checked-in baseline also prevents unreviewed
growth in the total package count or duplicate package names.

Run the complete gate with:

```shell
make audit
```

Use `make audit OFFLINE=1` when the advisory databases have already been
fetched and network access is unavailable. Required CI uses the default mode
and refreshes advisory data.

The gate checks the dependency baseline without rewriting manifests or the
lockfile, reports unused direct dependencies, enforces license and source
policy, and scans RustSec advisories. CI uses `cargo-shear` 1.14.0,
`cargo-deny` 0.20.2, and `cargo-audit` 0.22.2.

## Advisory exceptions

Exceptions live in both `deny.toml` and
`security/advisory-exceptions.tsv`. Each exception records reachability, an
owner, a review date, upstream tracking, an exact removal condition, and a
rationale. The policy check fails when the two files drift or a review date
expires. These are Xylem-owned exceptions; the ledger links the RustSec
advisories but, by project decision, does not create ACORN issues.

Current exceptions are limited to dependencies required by the retained ACORN
integration and runtime stack. They are owned by `@o9w` and must be reviewed by
2026-10-28.

## Updating the baseline

Review `Cargo.lock`, run `make audit`, and update
`security/dependency-baseline.env` only when the increase is justified. A
decrease does not require lowering the limits immediately, though doing so in
the same change preserves the strongest gate.

## Recorded baseline

The implementation worktree based on commit
`eebf30428dc6db55454fe72b5460845a9397de3c` was measured on 2026-10-01 with
Rust 1.98.1 on arm64 macOS 26.7. Both commands used an empty target directory,
`--locked`, and `--offline`:

- `cargo check --workspace --all-targets`: 296.63 seconds.
- `cargo build --release --package xylem-cli`: 371.78 seconds and a 5,054,272-byte arm64 Mach-O binary.

These platform-specific timings are comparison evidence, not cross-platform
limits. The portable enforced baselines are 376 locked packages and 23 names
with duplicate versions. The previous graph contained 844 packages.
