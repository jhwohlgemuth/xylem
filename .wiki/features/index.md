# Feature Plans

## Planned

- [Land typed execution foundations and add resumable execution](resumable-execution.md) - Prioritized. Replaces the YAML parser, introduces typed errors, validates configuration into a side-effect-free plan, then declares expected module outputs and skips completed work during resume runs.
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md) - Validate a peak-memory estimator and admit inputs within a configured memory budget and batch ceiling.

## Landed, pending document reconciliation

- [Structured run reporters](run-reporters.md) - Landed; console and JSON summaries are rendered from the recovered run summary. Only the dead reporter scaffolding remains.

## In progress

- [Homebrew release export](homebrew-export.md) - Repository and tap automation is complete; operational activation awaits protected tap credentials, required Homebrew runners, merge checks, and a new release tag.
