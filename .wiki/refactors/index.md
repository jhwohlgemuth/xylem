# Refactor Plans

## Planned

- [Typed errors and workflow outcomes](typed-error-and-outcome-boundary.md) - Remove process termination and panic placeholders from the library and propagate classified failures to the CLI. Executed as Phase 2 of [resumable execution](../features/resumable-execution.md).
- [Lossless process execution](process-execution-contract.md) - Capture stdout and stderr without deadlock, preserve exit status, and parse shell-format arguments correctly.
- [Workflow environment and concurrency](workflow-environment-and-concurrency.md) - Replace process-global environment mutation with immutable per-run contexts and deterministic parallel results.
- [Configuration validation and side-effect-free dry-run](configuration-validation-and-dry-run.md) - Separate parsing, semantic validation, planning, and execution while rejecting unsupported public variants. Executed as Phase 3 of [resumable execution](../features/resumable-execution.md).
- [Secure module acquisition](secure-module-acquisition.md) - Pin, verify, bound, and atomically materialize remote workflow modules.
- [Dependency surface and security gates](dependency-surface-and-security-gates.md) - Remove unused dependencies, narrow features, and make advisory policy actionable.
- [CI and release pipeline](ci-release-pipeline.md) - Test merge requests and tags, build explicit target artifacts, and publish releases from immutable tags.
- [Documentation and examples](documentation-and-examples.md) - Reconcile the public docs, roadmap, examples, and repository hygiene with implemented behavior.
