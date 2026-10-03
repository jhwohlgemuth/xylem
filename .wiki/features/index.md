# Feature Plans

## Planned

- [Workflow dashboard and durable run history](workflow-dashboard.md) - Launch and inspect workflows through a live TUI backed by canonical system run bundles, scheduler projections, logs, and host telemetry.
- [Kubernetes workflow execution backend](kubernetes-workflow-backend.md) - Submit one complete run as a reconnectable Kubernetes Job with shared-PVC state and a versioned runner contract.
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md) - Validate a peak-memory estimator and admit inputs within a configured memory budget and batch ceiling.
- [Scripted workflow input discovery and filtering](cel-workflow-input.md) - Discover local candidates and select workflow inputs through a bounded engine, with CEL as the initial implementation.

## In progress

- [Homebrew release activation](homebrew-export.md) - Configure protected credentials, required runners, and merge checks, then verify publication from a new release tag.
