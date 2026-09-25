# Feature Plans

## Planned

- [Fixed input concurrency configuration](fixed-input-concurrency.md) - Add `options.batch` as a rolling cap on active top-level inputs while preserving the global worker budget.
- [Durable execution journal](durable-execution-journal.md) - Persist per-input and per-module transitions so failed, interrupted, and unstarted work remains inspectable.
- [Structured run reporters](run-reporters.md) - Make the existing reporter and output CLI options produce deterministic console, stdout, and JSON workflow summaries.
- [Resumable execution and output checks](resumable-execution.md) - Declare expected outputs and explicitly skip completed work during resume runs.
- [Memory-aware adaptive scheduling](memory-aware-scheduling.md) - Validate a peak-memory estimator and admit inputs within a configured memory budget and batch ceiling.

## In progress

- [Homebrew release export](homebrew-export.md) - Repository and tap automation is complete; operational activation awaits protected tap credentials, required Homebrew runners, merge checks, and a new release tag.
