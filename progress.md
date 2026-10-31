# Implementation progress

## Implementation — Workflow environment and concurrency

- Plan: `.wiki/refactors/workflow-environment-and-concurrency.md`; active phase: none; state: authorized execution complete.
- Authorized scope and decisions: execute ENV-01 through ENV-02 and their dependency phases ENV-01A and ENV-02A in order; preserve ACORN 0.5.1 structured process evidence and existing execution defaults.
- Baseline and local work: commit `292aa49`; pre-existing local changes are limited to the ten reviewed `.wiki` files from plan preparation.
- Verified state: ENV-01 through ENV-02A are implemented and pruned. Structured process errors retain ACORN evidence, shell-format templates preserve quoted and escaped arguments, failure policy and module timeout inputs validate across supported formats, and the CLI override has the documented precedence. Full xylem-lib and xylem-cli suites, strict package Clippy, and rustdoc generation pass.
- Remaining work: ENV-03 is ready but outside this execution scope.
- Budget: ENV-01, ENV-01A, ENV-02, and ENV-02A each completed in under 1 active hour.
- Blockers / failed approaches: the first compile found two additional provider callers, so the existing `sandbox_process_error` name was retained and no provider files required edits. The sink test can surface ACORN cleanup context in this sandbox, so it accepts the documented sink or cleanup operational context while requiring lossless access.
- Next action: hand off ENV-03 with the configuration lock free.
