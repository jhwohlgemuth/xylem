---
okf_version: "0.1"
---

# Xylem Planning Wiki

This directory is the authoritative home for Xylem implementation plans. It follows the Open Knowledge Format layout used by the adjacent ACORN project.

## Contents

- [Plan dependency network](plan-network.md) - Execution order, prerequisites, and shared integration boundaries.
- [Parallel execution guide](parallel-execution.md) - Work lanes, exact ownership rules, and integration locks.
- [Feature plans](features/) - User-facing behavior that completes an existing public interface.
- [Refactor plans](refactors/) - Correctness, security, maintainability, delivery, and documentation work.
- [Migration plans](migrations/) - Dependency and format migrations.
- [Testing plans](testing/) - Unit, integration, and CLI coverage.

## Portfolio review

Rechecked on 2026-10-03 against the current worktree, dependency network, and
the live Homebrew tap merge request. Quality measures whether a plan has
current evidence, a bounded contract, explicit dependencies, phased delivery,
and verifiable acceptance criteria. Difficulty is remaining delivery effort;
`very high` identifies multi-release or infrastructure-heavy work. Value
combines user benefit, risk reduction, prerequisite leverage, and remaining
effort.

| Plan | Quality | Impact | Remaining difficulty | Value | Assessment |
|---|---:|---:|---:|---:|---|
| [Homebrew release activation](features/homebrew-export.md) | 4/5 | High | Low | Very high | The implementation and release artifacts exist, and Linux passes. Merge request [!2](https://code.ornl.gov/research-enablement/homebrew-xylem/-/merge_requests/2) is still open; the optional macOS job still fails for lack of a matching runner. Finish this operational work before adding packaging scope. Move the alternative runner setup detail to an operator runbook after activation. |
| [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md) | 4/5 | Critical | Very high | Very high | This is the runtime foundation for every advanced target. Its contracts and tests are strong, but process adaptation, immutable contexts, workspaces, scheduling, cancellation, and a breaking schema revision make it too large for one undivided landing. Keep its six phases as separately gated changes. |
| [Secure module acquisition](refactors/secure-module-acquisition.md) | 5/5 | Critical | High | Very high | The scope is focused, with a clear threat boundary, typed failure behavior, offline semantics, and adversarial validation. Implement planner and cache primitives independently, then serialize integration across the shared configuration and process locks. |
| [Sandboxed workflow execution](refactors/sandboxed-workflow-execution.md) | 4/5 | Critical | Very high | High | The threat model and capability negotiation are well specified. The plan spans fake, OCI, separate-kernel, and default-on rollout horizons; deliver through OCI and lifecycle hardening first, then require a new decision before the stronger-provider rollout. |
| [Memory-aware adaptive scheduling](features/memory-aware-scheduling.md) | 5/5 | High | Medium | High | The measurement evidence is strong. The scoped estimator passed its gate, but only for the pinned A+P workflow and measured size range. Production concurrency must remain one until a real concurrent holdout mix validates additive reservations. |
| [Workflow dashboard and durable run history](features/workflow-dashboard.md) | 3/5 | High | Very high | Medium | The plan combines runner extraction, configuration changes, storage, logs, telemetry, forecasting, history, and a TUI. Land the headless runner and run store first; telemetry and the TUI need independent exit gates before the CLI-breaking configuration change is accepted. |
| [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) | 4/5 | Critical | Very high | Conditional high | The plan specifies the controller/worker, PVC, protocol, and failure contracts in detail. Delivery depends on shared-filesystem semantics, cluster runners, release images, and confirmed operator demand. Keep it separate from sandboxing and validate those deployment assumptions before implementation. |
| [Scripted workflow input discovery and filtering](features/cel-workflow-input.md) | 4/5 | Medium | Medium | Medium | Bounded Xylem-owned discovery keeps the selection evaluator pure while supporting file selection. CEL is the initial engine behind a replaceable boundary. Engine, traversal, and schema costs are justified only by demonstrated discovery demand. |

## Consolidated and ranked portfolio

The eight source plans reduce to five portfolio-level plans. Existing pages
remain the detailed implementation chapters; physically merging their bodies
would recreate oversized plans and obscure the integration locks. Ranking uses
dependency readiness and value relative to remaining effort, while retaining
impact and difficulty explicitly.

| Rank | Consolidated plan | Source plans | Impact | Difficulty | Value and decision |
|---:|---|---|---:|---:|---|
| 1 | Release activation | Homebrew release activation | High | Low | Very high. Finish the macOS runner, make both tap jobs required, merge, and publish the verified install command. This is the smallest path to immediate user value. |
| 2 | Trusted local execution | Environment/concurrency + secure acquisition + sandboxing | Critical | Very high | Very high. These plans share the process, environment, workspace, configuration, CLI, and schema boundaries. Sequence the immutable runtime first, secure materialization second, then fake and OCI sandbox enforcement; defer the stronger provider to a later approval gate. |
| 3 | Adaptive runtime operations | Memory-aware scheduling + dashboard/run history | High | Very high | High. Treat scheduler events, simulation, and the canonical run store as one observability contract. Deliver the measured scheduler and headless run evidence before forecasts or TUI work. |
| 4 | Kubernetes execution target | Kubernetes workflow execution | Critical | Very high | Conditional high. It remains an orthogonal execution target with its own approval gate. Start after the shared runner lands and only when a supported cluster, shared PVC semantics, image pipeline, and user demand are confirmed. |
| 5 | Scripted input discovery and filtering | Scripted workflow input discovery and filtering | Medium | Medium | Medium. Keep it outside the critical runtime path; proceed after selection-engine qualification and the immutable input and path boundaries, or earlier only when user demand exceeds the dependency cost. |

## Recommended execution order

1. Complete Homebrew activation in the delivery lane.
2. Publish the upstream [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md), then land the environment/concurrency phases through the reusable runner and immutable input boundary.
3. Complete secure acquisition. Pure planner, policy, and cache work may overlap with the runtime foundation, but shared configuration and process integration remains serial.
4. Implement the already-authorized memory estimator, telemetry, and admission seam for its measured boundary. Keep concurrency at one until the concurrent holdout gate passes.
5. Add the fake and rootless OCI sandbox backends plus artifact and lifecycle hardening. Reassess stronger-provider work separately.
6. Decide between Kubernetes and the headless dashboard foundation using confirmed deployment demand. Both consume the shared runner and durable evidence contracts; neither should redefine them.
7. Add scripted discovery and filtering when selection-engine qualification and user demand justify its medium-impact dependency and traversal surface.

## Planning gaps

The roadmap milestones are aspirations rather than executable plans and are not
ranked here. The dependency network also names dependency/security gates, a
CI/release pipeline, hermetic tests, and documentation/examples without
corresponding current plan pages. Treat those names as coordination concerns,
not schedulable plans, until a page is restored or the stale edge is removed.

## Adding a plan

1. Create one semantically named Markdown file for one atomic topic.
2. Add `type: Plan`, `title`, `description`, `difficulty`, `impact`, a non-empty `package` list, `tags`, and `status` to YAML frontmatter.
3. Use `low`, `medium`, `high`, `mixed`, or `not-assessed` for difficulty; use `low`, `medium`, `high`, `critical`, `mixed`, or `not-assessed` for impact.
4. Use `planned` or `in-progress` for status. Add `priority: next` only for an immediate candidate.
5. Document the goal, current evidence, project code boundaries, dependencies, recommendation, validation, and acceptance criteria.
6. In project code boundaries, identify files to update, create, and delete. Write `None` when a category is intentionally empty.
7. Add the plan to its directory index, the [plan dependency network](plan-network.md), and a lane in the [parallel execution guide](parallel-execution.md).

## Maintaining plans

1. Recheck evidence against the current source, lockfile, tests, and CI configuration before implementation.
2. Update frontmatter and body together when scope, priority, package ownership, or status changes.
3. Delete completed plans and remove their index, dependency-network, and lane entries.
4. Consolidate overlapping proposals instead of creating competing owners for the same contract.
5. Keep internal links valid and external claims cited.
6. Reconcile code boundaries and integration locks whenever files or modules move.
7. Treat the dependency network and parallel guide as coordination records, not implementation jobs.
