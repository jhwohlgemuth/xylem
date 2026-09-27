# Feature Plans

## Planned

- [Finish typed execution foundations and resumable execution](resumable-execution.md) - Prioritized. Complete typed-error hardening, remaining resume edge cases, and release reconciliation; validated planning and completion behavior have landed.
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md) - Validate a peak-memory estimator and admit inputs within a configured memory budget and batch ceiling.
- [Shell-script workflow input](shell-script-workflow-input.md) - Add bounded, typed input discovery while preserving deterministic planning and CLI precedence.

## Landed, pending document reconciliation

- [Structured run reporters](run-reporters.md) - Landed; console and JSON summaries are rendered from the recovered run summary. Only the dead reporter scaffolding remains.

## In progress

- [Homebrew release export](homebrew-export.md) - Repository and tap automation is complete; operational activation awaits protected tap credentials, required Homebrew runners, merge checks, and a new release tag.
