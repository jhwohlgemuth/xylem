---
okf_version: "0.1"
---

# Plan dependency network

This map records implementation dependencies found while auditing Xylem 1.0.0 on 2026-09-17. An arrow means the left plan establishes a contract or baseline required by the right plan. Topical links alone do not imply an execution dependency.

## Correctness and execution path

```text
ACORN 0.2 upgrade ----> ACORN command environment sync
          |                         |
          +------------+------------+
                       |
maintained YAML parser-+
                       v
       typed errors and workflow outcomes
                 |                      |
                 v                      v
        lossless process       configuration validation
                 |                      |
                 +----------+-----------+
                            v
              environment/concurrency
                            |
                            v
             secure module acquisition
                            |
                            v
                   run reporters
```

The configuration plan may start parser-independent validation design while the YAML migration is underway. The secure acquisition plan waits for typed errors, validated locations, and the new ACORN download/checksum APIs so that it does not build another temporary boundary.

## Quality and delivery path

```text
ACORN upgrade ----+
YAML migration ---+----> dependency and security gates
                             |
hermetic tests ---------------+----> CI and release pipeline ----> Homebrew export

user-facing contracts + CI behavior + Homebrew export ----> documentation and examples
```

The hermetic test plan starts immediately with host-independent fixtures. Its final CLI failure assertions consume the typed outcome and reporter contracts.

## Direct dependency edges

- [ACORN 0.2 upgrade](migrations/acorn-0-2-upgrade.md) precedes acquisition changes because `download_binary` is asynchronous, checksum values are typed, portable location types moved to `acorn-core`, and SSH is no longer an ACORN `Scheme` variant.
- [ACORN command environment sync](migrations/acorn-command-environment-sync.md) follows the base upgrade, develops upstream and downstream validation in parallel, and precedes removal of Xylem's environment-bearing command fallback.
- [Maintained YAML parser](migrations/maintained-yaml-parser.md) precedes final configuration validation so JSON and YAML share one semantic error model.
- [Typed errors and workflow outcomes](refactors/typed-error-and-outcome-boundary.md) precedes process, configuration, acquisition, and reporter work; none of those plans may encode failure as logging plus `Vec::new()` or `process::exit`.
- [Process execution](refactors/process-execution-contract.md) precedes [environment and concurrency](refactors/workflow-environment-and-concurrency.md): per-invocation environment and cancellation attach to a structured child-process result, while scheduling builds on the existing typed thread budget.
- [Configuration validation and dry-run](refactors/configuration-validation-and-dry-run.md) precedes secure acquisition and reporters because both consume a validated, side-effect-free execution plan.
- [Secure module acquisition](refactors/secure-module-acquisition.md) consumes the error, process, configuration, and ACORN migration contracts.
- [Run reporters](features/run-reporters.md) consumes structured workflow outcomes and the dry-run plan; it does not parse logs to infer success.
- [Dependency and security gates](refactors/dependency-surface-and-security-gates.md) follows both dependency migrations so its accepted graph is not immediately invalidated.
- [Hermetic tests](testing/hermetic-workflow-tests.md) provides the regression gate required by the CI plan and validates every correctness plan as its contracts land.
- [CI and release pipeline](refactors/ci-release-pipeline.md) consumes the hermetic suite and dependency gates.
- [Homebrew release export](features/homebrew-export.md) consumes immutable tagged artifacts and checksums from the CI/release pipeline.
- [Documentation and examples](refactors/documentation-and-examples.md) follows user-facing contract decisions, the repaired build/release workflow, and the verified Homebrew installation contract.

## Shared integration boundaries

| Shared boundary | Plans that overlap | Coordination rule |
|---|---|---|
| `Cargo.toml`, crate manifests, `Cargo.lock` | ACORN, ACORN command sync, YAML, dependency gates, process parsing, testing tools | One manifest owner at a time. Land upstream ACORN work before pinning its commit, then regenerate and audit the final lockfile once. |
| `xylem-lib/src/config/mod.rs` | Errors, validation, environments, acquisition, reporters | Typed errors own signature conversion first; validation owns parse/plan separation; environment work preserves the existing Rayon execution limit; later plans modify only their extracted modules. |
| `xylem-lib/src/script/mod.rs` | Errors, process execution, environments, acquisition, ACORN command sync | Process execution owns `Script` and child-result contracts; the ACORN sync changes only the execution adapter after the upstream commit is fixed. |
| `xylem-cli/src/{main,cli}.rs` and `commands/run/mod.rs` | Errors, environment/concurrency, reporters, tests | One CLI integration owner. Unit/fixture work may proceed without changing command arguments or top-level dispatch. |
| `Makefile` and `.gitlab-ci.yml` | Dependency gates, tests, CI/release, Homebrew export | Dependency-gate commands land before CI wires them into required jobs; Homebrew publication follows the immutable release contract. |
| `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `examples/` | Every user-visible plan | Documentation may inventory drift early, but final edits wait for accepted names and behavior. |

## Target ownership

| Package | Boundary | Plans |
|---|---|---|
| `xylem-lib` | Configuration, planning, process execution, acquisition, workflow outcomes | ACORN, ACORN command sync, YAML, errors, process, environment, validation, acquisition |
| `xylem-cli` | Argument precedence, presentation, exit status, temporary workspace lifetime | Errors, environment, reporters, tests |
| `workspace` | Dependency graph, CI, releases, package exports, docs, examples | Dependency gates, CI/release, Homebrew export, documentation, hermetic tests |

## Handoff checklist

Before implementation, record the plan, exact files, shared locks, public contracts, baseline commit, focused validation command, and landing order. Before handoff, report changed files, new public contracts, generated artifacts, validation results, and any lock still required.
