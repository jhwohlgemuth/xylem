# Refactor Plans

## Planned

- [Lossless process execution](process-execution-contract.md) - Capture stdout and stderr without deadlock, preserve exit status, and parse shell-format arguments correctly.
- [Workflow environment and concurrency](workflow-environment-and-concurrency.md) - Replace process-global environment mutation with immutable per-run contexts and deterministic parallel results.
- [Secure module acquisition](secure-module-acquisition.md) - Pin, verify, bound, and atomically materialize remote workflow modules.
- [Dependency surface and security gates](dependency-surface-and-security-gates.md) - Remove unused dependencies, narrow features, and make advisory policy actionable.
- [CI and release pipeline](ci-release-pipeline.md) - Test merge requests and tags, build explicit target artifacts, and publish releases from immutable tags.
- [Documentation and examples](documentation-and-examples.md) - Reconcile the public docs, roadmap, examples, and repository hygiene with implemented behavior.
- [Configuration modules](configuration-modules.md) - Compose local and remote workflows as flattened, policy-constrained module groups.

## Completed records

- [Configuration validation and side-effect-free planning](configuration-validation-and-dry-run.md) - Landed deterministic parse, validation, planning, artifact designation, and write-free dry-run contracts.
- [Typed errors and workflow outcomes](typed-error-and-outcome-boundary.md) - Landed contextual library errors, structured outcomes, CLI-owned exit mapping, and production panic-shortcut lints.
