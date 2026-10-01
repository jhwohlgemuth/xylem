# Refactor Plans

## Planned

- [Lossless process execution](process-execution-contract.md) - Capture stdout and stderr without deadlock, preserve exit status, and parse shell-format arguments correctly.
- [Workflow environment and concurrency](workflow-environment-and-concurrency.md) - Replace process-global environment mutation with immutable per-run contexts and deterministic parallel results.
- [Secure module acquisition](secure-module-acquisition.md) - Pin, verify, bound, and atomically materialize remote workflow modules.
- [Dependency surface and security gates](dependency-surface-and-security-gates.md) - Remove unused dependencies, narrow features, and make advisory policy actionable.
- [ACORN GPU capability](acorn-gpu-capability.md) - Move portable GPU identity and telemetry types into ACORN while keeping run monitoring in Xylem.
- [CI and release pipeline](ci-release-pipeline.md) - Test merge requests and tags, build explicit target artifacts, and publish releases from immutable tags.
- [Sandboxed workflow execution](sandboxed-workflow-execution.md) - Route workflow preparation and commands through a fail-closed policy, resource, and lifecycle boundary.
