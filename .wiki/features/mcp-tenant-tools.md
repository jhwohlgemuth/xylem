---
type: Plan
title: "Add MCP-enabled tenant tools"
description: "Run root-configured tenant MCP calls as policy-checked per-input postludes with deterministic plans and durable evidence."
difficulty: high
impact: high
package: [xylem-lib, xylem-cli, xylem-tui, workspace]
tags: [feature, mcp, interoperability, policy, tenants, run-store]
status: planned
priority: next
---

# Goal

Let a Xylem workflow call explicitly configured tenant MCP tools through ACORN's
outbound client. Calls run after the module sequence for each successful input
and remain separate from Xylem's module catalog. Planning, policy, cancellation,
failure handling, and run evidence must behave like the rest of the workflow
runner.

# Current evidence

Rechecked on 2026-10-11.

- Xylem still pins `acorn-cmd`, `acorn-core`, `acorn-host`,
  `acorn-macros`, and `acorn-schema` at version 0.3.4 and revision
  `36cd74041eaa4669636b7743071f3c334cdb3b7d`. It does not depend on
  `acorn-lib` and has no MCP configuration or execution path.
- ACORN 0.4.0 has a policy-checked client for child-process and Streamable HTTP
  transports. It enforces configured server and tool allowlists, declared
  effects, offline mode, timeouts, output limits, advertised-tool discovery,
  and JSON-object arguments. A valid `CallToolResult` with `isError: true`
  remains a successful transport result.
- ACORN commit `77be500e` narrowed `acorn-lib/mcp-client` to foundational
  `std`, HTTP/RMCP, Tokio, and executable discovery without `analysis`, Polars,
  or OpenSSL.
- ACORN commit `aa36e93f` added direct
  `BTreeMap<String, McpServerConfiguration>` construction, the public
  statically dispatched `McpClientBackend` trait, caller-supplied cancellation
  for list and call operations, and bounded HTTP/child cleanup. Focused
  upstream tests cover cancellation during startup, pagination, pending calls,
  cleanup, child reaping, and client reuse.
- ACORN's embedder-interface plan remains open only for separately owned book
  integration. Its source interface is ready for Xylem qualification; Xylem no
  longer waits for upstream client implementation.
- Xylem's runner is synchronous and uses Rayon for bounded input concurrency.
  `RunCancellation` provides a graceful first request and forceful repeated
  request. Execution plans currently use version 12 and journals/summaries use
  version 11; neither contract represents MCP calls.
- Xylem already expands uppercase `{{ VARIABLE }}` strings from workflow values
  and the derived `INPUT`, `PARENT_DIRECTORY`, `OUTPUT_DIRECTORY`, and
  `MODULE_LIST` values. MCP arguments can reuse that vocabulary without adding
  another expression language.

# Dependencies

Begin by qualifying ACORN `aa36e93f` or a newer accepted 0.4.0 revision. The
selected revision must:

1. compile `acorn-lib/mcp-client` with foundational `std` support but without
   `analysis`, Polars, or OpenSSL;
2. construct `McpClient` directly from the configured MCP server map; and
3. provide cancellation-aware call and list operations that close HTTP sessions
   and cancel and reap child transports within a bounded shutdown period.

All ACORN crates then move together to that one revision. The qualification
must rerun the narrow feature check and cancellation suite rather than relying
only on the upstream plan. The revision must
retain Xylem's Rust baseline, feature checks, native and Windows builds, and
dependency-security gates.

# Recommendation

Qualify the upstream revision as an isolated manifest change, then land pure
configuration and planning before live transports or schema integration. Keep
the existing per-input runner as the only execution path; MCP is a deterministic
postlude, not a second scheduler or module system.

# Configuration contract

MCP is an optional root-workflow block with named servers and an ordered call
list:

```yaml
mcp:
  servers:
    research:
      transport:
        type: streamable-http
        url: https://tenant.example/mcp
        bearer_token_env: RESEARCH_MCP_TOKEN
      allowed:
        resolve:
          effects: [network-read]
          timeout: 30
          max_output: 1 MB
  calls:
    - name: resolve-input
      server: research
      tool: resolve
      arguments:
        input: "{{ INPUT }}"
```

Add `McpConfiguration` and `McpCallConfiguration` to `Config` with
`deny_unknown_fields`. Preserve ACORN's server, transport, and tool policy
types rather than defining a second wire shape.

Validation applies these rules:

- A present `mcp` block has at least one server and one call.
- Call names are nonempty and unique in configured order.
- Every call names a configured server and one of that server's allowed tools.
- Arguments default to `{}` and must be a JSON object.
- Server aliases, commands, arguments to child executables, URLs, bearer-token
  environment names, and tool names are fixed configuration. Workflow input
  cannot replace them.
- Any `mcp` block in a nested local or remote configuration is rejected, even
  when the block is empty. Imported workflows cannot add transports or tenant
  actions.
- Only string leaves inside `arguments` use templates. Expansion is textual;
  number, boolean, object, array, and null values remain unchanged. An unknown
  or unavailable variable fails planning before a transport can start.
- MCP calls use root workflow variables and the derived input values. Module
  variables do not affect root-level calls.

# Caller policy

Add `--allow-mcp-mutation` and `--allow-mcp-inference` to `xylem run` and
`xylem tui`. These values are runtime-only `ConfigOptions` and cannot be enabled
by a workflow file.

The pure planner mirrors ACORN's current policy vocabulary:

- `mutation`, `destructive`, and `filesystem-write` require mutation approval.
- `inference` requires inference approval.
- `network-read` is denied when offline, and Streamable HTTP is unavailable
  when offline regardless of declared effects.
- `credentials` and `process` remain unavailable effects.
- `filesystem-read` remains available without an extra approval, matching
  ACORN 0.4.0.

This mirror prevents known policy failures from occurring after modules have
run. ACORN remains the enforcement boundary and checks every live invocation.

# Planning contract

Add `mcp_calls: Vec<PlannedMcpCall>` to `ExecutionPlan`. Entries are ordered by
input index and then call index. Each entry records:

- input and call indices, input value, and configured call name;
- server alias and tool name;
- the resolved JSON arguments and a sorted, deduplicated list of referenced
  template variables;
- declared effects, timeout seconds, and maximum output bytes;
- effective offline, mutation, and inference settings; and
- `would_call` or `denied`, with a bounded policy reason for a denied call.

Dry-run renders denied calls instead of requiring approval merely to inspect a
plan. A real run refuses a plan containing a denied call before acquisition,
module execution, credential lookup, executable lookup, or transport startup.

Dry-run does not construct `McpClient`, inspect bearer-token values, resolve a
child executable, open a network connection, start a child process, request a
tool catalog, create a journal, or write a result artifact. Resolved arguments
are visible in dry-run output, consistent with the existing plan's effective
module variables. They are not copied into the journal.

Allocate the next execution-plan version at implementation time and add its
schema document. Retain prior schemas as historical documentation.

# Execution design

Create a small Xylem MCP adapter in `xylem-lib`. It owns one immutable ACORN
client and a one-worker Tokio runtime. Rayon continues to bound active inputs;
the runtime handles asynchronous I/O for calls already admitted by that input
limit. The adapter is created only for a real run with configured calls.

For each input:

1. Run modules through the existing recorded runner.
2. If the module outcome failed or was cancelled, leave its MCP calls
   `not_started` and finalize the input normally.
3. If the module outcome succeeded or was skipped, run MCP calls sequentially
   in configured order.
4. Stop the remaining calls on the first reported error, invocation failure,
   evidence-write failure, or cancellation. Return the resulting input outcome
   to the existing fail-fast dispatcher.

Calls still run when `--resume` skips every module as already complete. If all
calls succeed, the final input state is `succeeded` because the postlude did
work. There are no retries and no completion-marker semantics for MCP calls.

Build one `InvocationContext` per call with the effective approvals and offline
state, the configured tool output ceiling, direct origin, principal `xylem`, and
a correlation identifier derived from run, input, and call identity. Calls on
different inputs may overlap; calls for one input never overlap.

# Cancellation

Extend `RunCancellation` with a dedicated MCP cancellation signal. The first
request cancels active MCP work and prevents new calls from starting while
retaining the existing graceful behavior for managed module commands. A
repeated request also cancels the forceful command token.

The adapter passes the cancellation future to ACORN rather than aborting the
Tokio runtime. ACORN must finish protocol cancellation and bounded session
shutdown before the call future resolves. Xylem records the call and input as
`cancelled`; a cleanup detail may be appended without changing cancellation as
the primary outcome.

# Durable evidence and reporting

Add immutable `McpCallOutcome`, `McpCallSummary`, `McpResultEvidence`, and
`McpFailureKind` types. The failure kinds are:

- `reported_error` for a valid result whose `isError` field is true;
- `invocation` for policy, discovery, transport, timeout, output-limit, or
  malformed-response failure;
- `evidence` when a valid result cannot be stored; and
- `cancelled` for caller cancellation.

Serialize every valid bounded result atomically to
`mcp/input-{input_index}/call-{call_index}.json` inside the run bundle. The
version 1 artifact contains stable run, input, and call identity plus the MCP
`content`, `structuredContent`, `isError`, and `_meta` fields. Compute the
SHA-256 digest and byte count from the bytes actually written.

Add initialized MCP call identities and `McpCallState` transitions to the
journal. Terminal events and summaries store the relative artifact path,
digest, byte count, and reported-error flag instead of duplicating the result.
A result with `isError: true` is written first, then recorded as
`reported_error`, and fails the input. An invocation failure has no result
artifact. Failure to persist evidence fails the input because the durable
result contract was not met.

Add MCP calls to `WorkflowOutcome` and each `InputSummary`. Allocate the next
journal/run-summary version at implementation time and add its schema plus
`docs/schema/mcp-result-v1.schema.json`. Update writers, recovery, projection,
store discovery, reporters, and the TUI together. Follow the current exact
schema-version policy: retain prior schema documents but do not add run
migrations.

Console, JSON, and TUI views use the same projected summary. They show call
name, server, tool, state, failure kind, and artifact reference. The TUI does
not render arbitrary result payloads.

# Implementation sequence

1. Qualify the upstream ACORN revision and update all Acorn dependencies,
   imports, the lockfile, and the dependency baseline in one manifest-owned
   change.
2. Add configuration types, root-only resolution, validation, pure recursive
   argument expansion, caller policy, and planning. Land current-version plan
   tests before live execution.
3. Add the ACORN adapter, Tokio bridge, runtime-only approvals, and real-run
   policy preflight.
4. Add result artifacts, journal events, outcomes, recovery, projections, and
   newly versioned summaries.
5. Integrate per-input postludes with the runner and cancellation path.
6. Update headless reporting, TUI rendering, examples, generated help, and user
   documentation after names and behavior are stable.

# Project code boundaries

- Update: workspace manifests and lockfile; dependency policy; configuration,
  planning, cancellation, workflow outcomes, run orchestration, journal,
  projection, run store, reporters, TUI, schemas, documentation, and examples.
- Create: the Xylem MCP adapter, typed result artifact support, fake
  child-process and HTTP fixtures, focused tests, and versioned/v1 schemas.
- Delete: none. Do not remove prior schema documents.

The implementation claims the manifest, configuration, CLI, MCP integration,
run-state, run-store, TUI, and documentation locks. It must not overlap another
active owner of those files.

# Validation

Unit and schema tests cover:

- JSON, JSONC, and YAML parsing; unknown fields; empty blocks; duplicate names;
  server and allowlist references; and nested-configuration rejection;
- recursive templates, literals of every JSON type, missing input variables,
  sorted provenance, input-major planning order, and zero-input workflows;
- every effect and approval combination, offline child and HTTP behavior, and
  dry-run policy decisions;
- current-version plans, journals, recovery, projections, summaries,
  result-artifact schema, digest verification, invalid transitions, and
  interrupted runs.

Integration fixtures exercise the real ACORN adapter for:

- child-process and Streamable HTTP success, bearer authentication, environment
  isolation, allowlisted but unadvertised tools, and unallowlisted calls that do
  not start a transport;
- mutation and inference approval, offline rejection, timeout, output limit,
  malformed protocol/result data, and cleanup failure;
- `isError: true` with retained evidence versus invocation failure without an
  artifact;
- first-request cancellation, forced cancellation, HTTP closure, child reaping,
  sequential call order, cross-input concurrency, fail-fast dispatch, module
  failure, resume, and evidence-write failure;
- matching console, JSON, and TUI outcomes over the same stored run.

Run `cargo check --workspace --all-targets --locked`, `make test-ci`,
`make audit`, documentation builds, and the existing native and Windows release
builds. Inspect the final feature tree and lockfile to confirm that Polars and
OpenSSL remain absent.

# Acceptance criteria

- The selected ACORN 0.4.0 revision satisfies the narrow-feature, direct
  construction, and cancellation contracts without a local fork.
- Workflow input cannot choose a server, transport, executable, URL, header,
  bearer-token environment name, or tool.
- Dry-run is write-free and transport-free while showing exact arguments,
  provenance, policy, timeout, and output limits.
- Every live invocation is configured, allowlisted, advertised, and checked by
  ACORN under the caller's explicit approvals.
- Calls are deterministic per input, honor existing input concurrency and
  fail-fast behavior, and run after fully resumed module chains.
- Tool-reported errors remain distinct from invocation failures and retain the
  bounded typed result.
- Cancellation closes the MCP session and reaps child transports before the
  workflow reports completion.
- Result artifacts, journal recovery, headless reporters, and the TUI agree on
  every terminal outcome.
- `make audit` accepts the reviewed graph while continuing to forbid Polars and
  OpenSSL.

# Non-goals

- No ad hoc `xylem mcp call` or tool-list command.
- No arbitrary headers, URLs, commands, tool names, or server aliases supplied
  by workflow input.
- No remote tools in the module catalog.
- No result-to-module data flow, retries, call-level continue-on-error option,
  nested MCP configuration, or MCP proxy/server mode.
- No Xylem-owned MCP transport implementation or local ACORN patch.

# Related plans

- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- ACORN: [Refine the outbound MCP client interface for embedders](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/api/outbound-mcp-client-interface.md)
