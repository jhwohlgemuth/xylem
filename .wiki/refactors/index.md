# Refactor Plans

## Planned

- [Lossless process execution](process-execution-contract.md) - Capture stdout and stderr without deadlock, preserve exit status, and parse shell-format arguments correctly.
- [Workflow environment and concurrency](workflow-environment-and-concurrency.md) - Replace process-global environment mutation with immutable per-run contexts and deterministic parallel results.
- [Secure module acquisition](secure-module-acquisition.md) - Pin, verify, bound, and atomically materialize remote workflow modules.
- [Dependency surface and security gates](dependency-surface-and-security-gates.md) - Remove unused dependencies, narrow features, and make advisory policy actionable.
- [CI and release pipeline](ci-release-pipeline.md) - Test merge requests and tags, build explicit target artifacts, and publish releases from immutable tags.
- [Documentation and examples](documentation-and-examples.md) - Reconcile the public docs, roadmap, examples, and repository hygiene with implemented behavior.
- [Sandboxed workflow execution](sandboxed-workflow-execution.md) - Route workflow preparation and commands through a fail-closed policy, resource, and lifecycle boundary.
- [Managed container lifecycle](managed-container-lifecycle.md) - Create, verify, and clean up workflow-owned Docker and Podman containers.
