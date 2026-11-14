# Feature Plans

## Planned

- [Configuration variable expansion](configuration-variable-expansion.md) - Inputs, output, and local sources now expand; EXP-05 remote/nested sources and EXP-06 template commands are ready.
- [Kubernetes workflow execution backend](kubernetes-workflow-backend.md) - Qualify operator gates, then add a reconnectable Job and versioned worker contract in bounded phases.

## Waiting on validation

- [Homebrew platform validation](homebrew-export.md) - v1.0.7 is published; qualify macOS, require both platform jobs, and verify clean consumer installs/upgrades.
- [A/P/O memory-aware scheduling](memory-aware-scheduling.md) - Run the credentialed expanded holdout gate before enabling concurrent admission for the pinned A/P/O workflow.
