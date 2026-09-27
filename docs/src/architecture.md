# Architecture

The CLI owns argument parsing, logging setup, top-level orchestration, and the
process exit boundary. The library owns configuration parsing, module
preparation, variable resolution, command construction, and workflow
execution.

Configuration follows a four-stage contract: parse the named JSON or YAML
document, validate all declared capabilities, build an immutable execution
plan, then execute that accepted plan. Planning resolves CLI precedence,
inputs, artifact roots, variables, acquisition intent, command arguments, and
completion declarations before Xylem creates its run workspace. Resume adds
explicitly labeled read-only completion observations; ordinary planning is
host-write-free.

```text
configuration (JSON or YAML)
            |
            v
        xylem-cli
            |
            v
        xylem-lib
       /    |       \
      v     v        v
 modules  inputs  variables
       \    |       /
        v   v      v
       child commands
             |
             v
  append-only journal
             |
             v
    console/JSON report
```

`ExecutionLimits` and `ExecutionContext` provide the current worker-budget
boundary. Modules execute in declaration order for an input, while multiple
inputs and module preparation may use the configured Rayon pool.

The library-owned execution journal is the durable source of truth for run,
input, and module state. It serializes concurrent transitions, assigns ordered
sequence numbers, flushes every complete JSON Lines record, and recovers a
deterministic summary while tolerating only a truncated final record. The CLI
owns reporter presentation and exit-code selection; reporters read the
recovered summary and never rewrite journal history.

Reusable failures are capability-owned. `ConfigReadError` covers selection and
deserialization, `PlanError` covers deterministic input and validation work,
and acquisition, requirement, template, module-loading, process, completion,
and journal operations each expose their own contextual error type. Paths,
module names, working directories, and underlying sources are retained where
they exist. Structured workflow outcomes convert those errors to durable text
only when crossing into the journal/report schema. The CLI alone maps the
classification to a process exit code and renders the terminal diagnostic.

Resume decisions are made before module preparation from explicit completion
declarations. The runtime requires both non-empty declared outputs and an exact
canonical marker identity, and records skips as `skipped_existing` in journal
schema version 2. Dry-run uses a separate in-memory execution plan and performs
no filesystem writes.

See
[ARCHITECTURE.md](https://code.ornl.gov/research-enablement/xylem/-/blob/main/ARCHITECTURE.md)
for repository-level notes and the planning wiki for proposed contract changes.
