---
type: Plan
title: "Add MCP-enabled tenant tools"
description: "Expose explicitly configured remote MCP tools to Xylem workflows through ACORN's policy-checked client boundary."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli, workspace]
tags: [feature, mcp, interoperability, policy, tenants]
status: planned
priority: next
---

# Goal

Let a Xylem workflow call an explicitly configured tenant MCP tool through the
upstream ACORN client, while preserving Xylem's deterministic planning,
execution, cancellation, journaling, and reporting contracts.

# Current evidence

ACORN implements the client in revision `3d2b504c` (`acorn-lib`'s non-default
`mcp-client` feature). It supports configured child-process and Streamable HTTP
transports, tool discovery, allowlists, effect policy, offline behavior,
deadlines, output limits, and typed MCP results. Xylem is currently pinned to
ACORN revision `36cd7404` and does not contain MCP configuration or invocation
code.

# Scope

- Define the Xylem configuration shape for MCP server aliases and allowed tools.
- Add a pure planning representation for an MCP invocation, including server,
  tool, argument provenance, policy, timeout, and output limits.
- Adapt ACORN's client result into durable run evidence without converting a
  tool-level `isError` into a transport failure.
- Support fake child-process and HTTP fixtures for allowlist, offline, effect,
  timeout, output-limit, cancellation, and malformed-result behavior.
- Document that Xylem is a configured caller, not a general MCP proxy.

Do not add arbitrary server URLs, headers, tool names, or shell commands to
workflow input. Do not expose remote tools through Xylem's existing module
catalog until a separate capability and trust decision is recorded.

# Dependencies and acceptance criteria

Upgrade all ACORN crates together only after the MCP feature is compatible with
Xylem's MSRV, feature matrix, and portable builds. Preserve the dashboard's
run-store and schema contracts. The plan is accepted when dry-run is
side-effect-free, every invocation is allowlisted and policy-checked, remote
results are journaled deterministically, cancellation closes the session, and
headless and TUI reporting agree on the outcome.

# Project code boundaries

- Update: `Cargo.toml`, `Cargo.lock`, configuration, planning, journal/outcome,
  CLI orchestration, docs, and examples as the contract is finalized.
- Create: MCP adapter, fixtures, focused unit/integration tests, and schema
  changes only if existing durable evidence cannot represent the result.
- Delete: None.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md)
