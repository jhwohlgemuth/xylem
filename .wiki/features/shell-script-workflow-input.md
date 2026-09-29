---
type: Plan
title: "Shell-script workflow input"
description: "Resolve workflow inputs from a configured shell script without weakening deterministic planning or execution safety."
difficulty: high
impact: medium
package: [xylem-lib, xylem-cli]
tags: [feature, input, shell, planning, security]
status: planned
priority: backlog
---

# Goal

Support `InputAttribute::ShellScript` as an explicit input-discovery mechanism.
The script must produce the same ordered, deduplicated input model used by list
and directory inputs while preserving the parse, validate, plan, execute
boundary.

# Contract decisions

- Replace the reserved `ShellScript(ScriptTemplate)` payload with a structured
  discovery request containing the script and declared input roots. Resolve
  relative roots from the configuration source and require operator policy to
  grant them before provisioning.
- Define stdout as the sole input channel, with one UTF-8 path per line.
- Reject non-zero status, invalid UTF-8, empty records, and output above a
  documented byte and record limit.
- Build a static plan first, then run discovery through the workflow sandbox
  before constructing the final immutable execution plan. Represent discovery
  failures as typed input errors with captured status and bounded stderr.
- Preserve first occurrence order and apply the same duplicate policy as CLI
  and configured list inputs.
- Make the side effect explicit: normal execution performs discovery between
  the static and final plans, while write-free dry-run never executes it.
- Apply a discovery-specific narrowing of the workflow sandbox policy: declared
  input roots are read-only, scratch is private, and network and credentials
  are denied by default. Do not inherit arbitrary process-global variables.
- Treat stdout paths as guest paths under declared input roots. Reject path
  traversal, special files, and symlink escapes while mapping them back to host
  inputs.
- Keep write-free dry-run process-free. With no CLI input override, report
  dynamic inputs as unresolved rather than running discovery implicitly.

# Implementation phases

## Phase 1: Typed discovery boundary

Add a capability-owned input discovery request and error enum. Move list and
directory resolution behind that boundary so shell discovery shares the same
normalization, duplicate handling, limits, and diagnostics. Validate declared
roots before resolving the script.

## Phase 2: Bounded shell execution

Execute the configured `ScriptTemplate` through the checked sandbox session.
Capture stdout and stderr without deadlock, enforce time, PID, byte, and record
limits, and parse stdout into input records only after successful completion.

## Phase 3: Planning and CLI integration

Replace the current semantic-validation rejection with a static discovery plan
followed by sandboxed input resolution. Record the discovery command identity,
effective policy digest, source, and bounded result metadata in the final
execution plan. Keep CLI inputs at highest precedence so an explicit CLI list
does not provision a discovery sandbox. Add a versioned journal transition for
registering discovered inputs before module execution.

## Phase 4: Documentation and fixtures

Document quoting, working directory, environment, line format, limits, and
failure behavior. Add JSON and YAML fixtures plus compiled-CLI integration
coverage.

# Validation

- Unit tests for empty output, duplicates, ordering, invalid UTF-8, non-zero
  status, oversized output, timeout, and bounded stderr.
- Integration proof that CLI inputs bypass the shell script.
- Integration proof that discovery cannot read undeclared host paths, inherit
  ambient credentials, reach the network, or escape declared input roots.
- Dry-run proof that no discovery session is provisioned.
- Determinism proof that identical stdout yields byte-identical plans.
- `cargo nextest run --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`

# Acceptance criteria

- Valid shell output resolves to deterministic `Vec<Option<String>>` inputs.
- Every process and parsing failure is typed and path-qualified.
- Input discovery cannot silently inherit unrestricted environment state or
  produce unbounded memory use.
- Input discovery has no direct host-process path and cannot broaden the
  workflow's effective sandbox policy.
- CLI input precedence remains unchanged.

# Dependencies

- Follows [lossless process execution](../refactors/process-execution-contract.md).
- Follows [sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md)
  and consumes its policy, session, path-mapping, cancellation, and journal
  contracts.
