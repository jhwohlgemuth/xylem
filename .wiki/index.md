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

Rechecked on 2026-10-19 against committed source `de0bd89` and current schemas.
All six active plans are executable and use five-hour phases. External tap,
runner, cluster, and credential claims
remain prerequisites until their owners provide current evidence. Quality
reflects evidence, bounded phases, dependencies, and acceptance criteria;
difficulty describes remaining work.

| Plan | Quality | Impact | Remaining difficulty | Readiness | Assessment |
|---|---:|---:|---:|---|---|
| [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md) | 4/5 | Critical | High | Ready | The workspace and split run path are landed. Process results, immutable contexts, policy, cancellation, and schema integration remain. |
| [Secure module acquisition](refactors/secure-module-acquisition.md) | 4/5 | Critical | Medium | In progress | SEC-04/05 Linux amd64 and arm64 qualification is accepted at `b1e9297`. SEC-06 macOS is next; Windows qualification and the dependency audit remain. |
| [Local OCI archive qualification](features/local-oci-bundle-uris.md) | 4/5 | High | Medium | Blocked on acquisition lock | Native directory URIs and the example are landed; archive and sandbox qualification remain. |
| [Homebrew release activation](features/homebrew-export.md) | 4/5 | High | Low | Ready | The assigned Mac runner is online, but its tap jobs were canceled. A passing macOS job, required two-platform pipeline, merge, and install/upgrade evidence remain. |
| [A/P/O memory validation](features/memory-aware-scheduling.md) | 5/5 | High | Low | Blocked on rollout decision | The scheduler and A/P evidence are landed. Run the pinned credentialed A/P/O holdout only when selected for rollout. |
| [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) | 4/5 | Critical | Very high | Blocked on operator gates | Controller/worker work requires accepted demand, shared storage, authentication, image path, and the immutable environment boundary. |

The current landed baselines include execution-plan v14, run-summary v13,
verified acquisition, owned workspaces, sandbox providers, memory admission,
and durable run storage. New runtime work must preserve those contracts.

## Recommended execution order

1. Run ENV-01 and SEC-06 on disjoint process and acquisition platform locks;
   record the macOS acquisition result.
2. Start OCI-01 when SEC releases the configuration/acquisition lock; continue
   the environment phases in order as their locks become available.
3. Start BRW-01 on the assigned online Mac runner. Run APO-01 only
   after the A/P/O rollout decision, corpus access, and credentials arrive.
4. Start K8S-01 when the operator gates are available; runtime phases follow
   the environment context and schema work.

## Planning gaps

Roadmap milestones and deferred ideas are not schedulable plans. Create a
focused page with current evidence, ownership, validation, acceptance
criteria, and bounded phases before starting process guards, another sandbox
provider, or other deferred scope.

## Adding a plan

1. Create one semantically named Markdown file for one atomic topic.
2. Add `type: Plan`, `title`, `description`, `difficulty`, `impact`, a non-empty `package` list, `tags`, `status`, `execution: phased`, and `phase_budget_hours: 5` to YAML frontmatter.
3. Use `low`, `medium`, `high`, `mixed`, or `not-assessed` for difficulty; use `low`, `medium`, `high`, `critical`, `mixed`, or `not-assessed` for impact.
4. Use `planned`, `in-progress`, or `waiting-on-validation` for status. Add
   `priority: next` only for an immediate unblocked candidate.
5. Document the goal, current evidence, project code boundaries, dependencies, recommendation, validation, and acceptance criteria. Separate accepted facts from remaining work.
6. In project code boundaries, identify files to update, create, and delete. Write `None` when a category is intentionally empty.
7. Add `# Execution phases`. Each phase has a stable ID, `planned`, `ready`, `blocked`, or `in-progress` status, at most five active hours, prerequisites, an exact write set or named external system, one primary integration lock, a mergeable outcome, focused validation, and a handoff.
8. Add the plan to its directory index, the [plan dependency network](plan-network.md), and a lane in the [parallel execution guide](parallel-execution.md).

## Maintaining plans

1. Recheck evidence against the current source, lockfile, tests, and CI configuration before implementation.
2. Update frontmatter and body together when scope, priority, package ownership, or status changes.
3. Remove completed phases and checked tasks. Preserve stable IDs for unfinished phases. Delete completed plans and remove their index, dependency-network, and lane entries after durable evidence is preserved outside `.wiki`.
4. Consolidate overlapping proposals instead of creating competing owners for the same contract.
5. Keep internal links valid and external claims cited.
6. Reconcile code boundaries and integration locks whenever files or modules move.
7. Treat the dependency network and parallel guide as coordination records, not implementation jobs.
8. Treat external waiting as a prerequisite outside the five-hour active budget. Give a validation defect its own bounded repair phase rather than expanding the active phase.
9. Run the planning skill's `validate_wiki.py` and inspect the diff after changes.
