# Refactor Plans

## Planned

- [Workflow environment and concurrency](workflow-environment-and-concurrency.md) - Add immutable run/input contexts, isolated workspaces, explicit failure policy, and deterministic bounded outcomes on the upstream ACORN process contract.
- [Secure module acquisition](secure-module-acquisition.md) - Pin, verify, bound, and atomically materialize remote workflow modules.
- [CI and release pipeline](ci-release-pipeline.md) - Test merge requests and tags, build explicit target artifacts, and publish releases from immutable tags.
- [Sandboxed workflow execution](sandboxed-workflow-execution.md) - Route workflow preparation and commands through a fail-closed policy, resource, and lifecycle boundary.
