# Feature Plans

## Planned

- [Kubernetes workflow execution backend](kubernetes-workflow-backend.md) - Submit one complete run as a reconnectable Kubernetes Job with shared-PVC state and a versioned runner contract.
- [MCP-enabled tenant tools](mcp-tenant-tools.md) - Invoke explicitly configured remote MCP tools from a workflow through ACORN's policy-checked client boundary.

## Waiting on validation

- [Homebrew release activation](homebrew-export.md) - Configure protected credentials, required runners, and merge checks, then verify publication from a new release tag.
- [A/P/O memory-aware scheduling](memory-aware-scheduling.md) - Run the credentialed expanded holdout gate before enabling concurrent admission for the pinned A/P/O workflow.
