# Architecture

The CLI owns argument parsing, logging setup, top-level orchestration, and the
process exit boundary. The library owns configuration parsing, module
preparation, variable resolution, command construction, and workflow
execution.

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

See
[ARCHITECTURE.md](https://code.ornl.gov/research-enablement/xylem/-/blob/main/ARCHITECTURE.md)
for repository-level notes and the planning wiki for proposed contract changes.
