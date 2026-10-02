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

The gate checks the dependency baseline without rewriting manifests or the
lockfile, reports unused direct dependencies, enforces license and source
policy, and scans RustSec advisories. CI uses `cargo-shear` 1.14.0,
`cargo-deny` 0.20.2, and `cargo-audit` 0.22.2.

## Advisory exceptions

Exceptions live in both `deny.toml` and
`security/advisory-exceptions.tsv`. Each exception must have an owner, a short
rationale, and a review date. The policy check fails when the two files drift
or a review date expires. Exceptions are Xylem-owned and do not imply an
upstream ACORN work item.

Current exceptions are limited to dependencies required by the retained ACORN
integration and runtime stack. They are owned by `@o9w` and must be reviewed by
2026-10-28.

## Updating the baseline

Review `Cargo.lock`, run `make audit`, and update
`security/dependency-baseline.env` only when the increase is justified. A
decrease does not require lowering the limits immediately, though doing so in
the same change preserves the strongest gate.
