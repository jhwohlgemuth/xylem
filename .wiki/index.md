---
okf_version: "0.1"
execution: coordination
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

Rechecked on 2026-11-01 against baseline `fcf4df3`, current schemas, focused
tests, retained measurement, and platform evidence. The portfolio contains 5
executable plans, all using five-hour phases. External Windows-runner, tap,
A/P/O rollout,
cluster, storage, image, corpus, and credential claims remain explicit
prerequisites until their owners provide current evidence. Full xylem-lib and
xylem-cli tests, strict focused Clippy, and rustdoc generation passed for the
current plan, configuration, acquisition, and CLI changes. External tap state
was not independently rechecked during these phases.

Rootless BuildKit delivery evidence was added on 2026-11-04 against baseline
`7aa9ef8` and failed job 4503475. Four affected jobs are temporarily skipped
until a dedicated runner passes the bounded qualification plan.

| Plan | Quality | Impact | Remaining difficulty | Readiness | Assessment |
|---|---:|---:|---:|---|---|
| [Rootless BuildKit runner](refactors/rootless-buildkit-runner.md) | 5/5 | High | Medium | Planned | Four rootless BuildKit jobs are skipped after generic runner `kronk` blocked RootlessKit startup. Provision a dedicated non-privileged runner with a narrow seccomp profile before phased re-enablement. |
| [Secure module acquisition](refactors/secure-module-acquisition.md) | 5/5 | Critical | Low | Blocked on Windows runner support | SEC-01 through SEC-06 are accepted. Published ACORN CLI 0.5.1 still assumes a POSIX Docker socket for a local container runner, so SEC-07 awaits a native Windows-capable release and host. |
| [Homebrew release activation](features/homebrew-export.md) | 5/5 | High | Low | Ready | Tap merge request !2 remains open and mergeable at the intended commit. Its Linux job passes; three macOS retries remain canceled on the online assigned runner. |
| [A/P/O memory validation](features/memory-aware-scheduling.md) | 5/5 | High | Low | Blocked on rollout decision | The scheduler and A/P evidence are landed. Run the pinned credentialed A/P/O holdout only when selected for rollout. |
| [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) | 5/5 | Critical | Very high | Blocked on operator gates | No target, profile, protocol, client, or worker implementation has landed. Controller/worker work requires accepted demand, shared storage, authentication, image path, and the immutable environment boundary. |

The current baselines include execution-plan v15, run-summary v14,
verified acquisition, owned workspaces, sandbox providers, memory admission,
and durable run storage. New runtime work must preserve those contracts.

## Recommended execution order

1. Start RBK-01 when runner administration access and an amd64 host maintenance
   window are available; keep all four rootless BuildKit jobs skipped until
   their owning validation phases pass.
2. Start SEC-07 after a published ACORN release and Windows host support
   native Docker runner creation.
3. Start BRW-01 on the assigned online Mac runner. Run APO-01 only
   after the A/P/O rollout decision, corpus access, and credentials arrive.
4. Start K8S-01 when the operator gates are available; runtime phases consume
   the accepted immutable context and v15/v14 schema contracts.

## Planning gaps

Roadmap milestones and deferred ideas are not schedulable plans. Create a
focused page with current evidence, ownership, validation, acceptance
criteria, and bounded phases before starting process guards, another sandbox
provider, or other deferred scope.

## Adding a plan

1. Create one semantically named Markdown file for one atomic topic, or a
   folder whose `index.md` is the plan entrypoint. Keep plan assets under that
   folder's `_assets/` and wiki-wide assets under `.wiki/_assets/`.
2. Add `type: Plan`, `title`, `description`, `difficulty`, `impact`, a non-empty `package` list, `tags`, `status`, `execution: phased`, and `phase_budget_hours: 5` to executable-plan YAML frontmatter. Use `execution: coordination`, omit the phase budget, and link executable child plans for an Epic.
3. Use `low`, `medium`, `high`, `mixed`, or `not-assessed` for difficulty; use `low`, `medium`, `high`, `critical`, `mixed`, or `not-assessed` for impact.
4. Use `planned`, `in-progress`, or `waiting-on-validation` for status. Add
   `priority: next` only for an immediate unblocked candidate.
5. Document the goal, current evidence, project code boundaries, dependencies, recommendation, validation, and acceptance criteria. Separate accepted facts from remaining work.
6. In project code boundaries, identify files to update, create, and delete. Write `None` when a category is intentionally empty.
7. Add `# Execution phases`. Each phase has a stable ID, `planned`, `ready`, `blocked`, or `in-progress` status, at most five active hours, prerequisites, an exact write set or named external system, one primary integration lock, a mergeable outcome, focused validation, and a handoff.
8. Add the plan to its directory index, the [plan dependency network](plan-network.md), and a lane in the [parallel execution guide](parallel-execution.md).

## Maintaining plans

1. Recheck in-scope evidence at every planning session. While planning remains active, review the whole portfolio at least once per calendar month and record the date, evidence scope, and unresolved blockers in this section.
2. Update frontmatter and body together when scope, priority, package ownership, or status changes.
3. Remove completed phases and checked tasks. Preserve stable IDs for unfinished phases. Delete completed plans and remove their index, dependency-network, and lane entries after durable evidence is preserved outside `.wiki`.
4. Consolidate overlapping proposals instead of creating competing owners for the same contract.
5. Keep internal links valid and external claims cited.
6. Reconcile code boundaries and integration locks whenever files or modules move.
7. Treat the dependency network and parallel guide as coordination records, not implementation jobs.
8. Treat external waiting as a prerequisite outside the five-hour active budget. Give a validation defect its own bounded repair phase rather than expanding the active phase.
9. Run the planning skill's `validate_wiki.py` and inspect the diff after changes.
