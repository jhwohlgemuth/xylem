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

Rechecked on 2026-10-11 against the current worktree, schema set, Homebrew tap
merge request, and adjacent ACORN 0.4.0 source. Quality measures current
evidence, bounded scope, explicit dependencies, delivery phases, and verifiable
acceptance criteria. Difficulty is remaining work rather than original scope.

| Plan | Quality | Impact | Remaining difficulty | Value | Assessment |
|---|---:|---:|---:|---:|---|
| [MCP-enabled tenant tools](features/mcp-tenant-tools.md) | 5/5 | High | High | Very high | ACORN's narrow feature graph, direct server-map constructor, public backend trait, and cancellation-aware operations landed in `aa36e93f`. Qualify an accepted 0.4.0 revision, then implement the already bounded Xylem contract. |
| [Homebrew release activation](features/homebrew-export.md) | 4/5 | High | Low | Very high | Merge request [!2](https://code.ornl.gov/research-enablement/homebrew-xylem/-/merge_requests/2) remains open and mergeable. Linux passes; the allowed-to-fail macOS job is canceled and a compatible runner is still unavailable. Runner assignment and required validation remain the only activation blockers. |
| [Secure module acquisition](refactors/secure-module-acquisition.md) | 5/5 | Critical | High | Very high | The fail-closed planner, bounded executors, and verified offline cache are implemented but dormant. Active configuration, binary, and Git paths retain their legacy behavior until the owned-workspace prerequisite permits one coordinated integration. |
| [Workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md) | 4/5 | Critical | Very high | High | Run orchestration is split and cancellation primitives exist, but host execution still mutates process-global environment, creates nested worker pools, shares mutable preparation, and lacks immutable contexts, continue policy, sibling cancellation, and owned workspaces. |
| [A/P/O memory validation](features/memory-aware-scheduling.md) | 5/5 | High | Low | Conditional high | The module-agnostic scheduler, v12 plan, v11 durable evidence, Linux OCI peak collection, A/P expanded gate, and operational reruns are complete. Only the credentialed A/P/O rollout gate remains; do not schedule it until that workflow is selected for rollout. |
| [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) | 4/5 | Critical | Very high | Conditional high | The controller/worker, PVC, protocol, and failure contracts remain coherent, but implementation still depends on immutable host-run contexts, shared-filesystem qualification, a worker image pipeline, cluster runners, and demonstrated operator demand. |

The completed sandbox execution plan was removed on 2026-10-11. OCI,
OpenShell, and Docker Sandbox providers, policy v1, execution-plan v12,
run-summary v11, lifecycle reconciliation, artifact publication, conformance,
examples, and documentation are now landed baselines. Process guards and any
new provider require a focused plan before work starts.

## Recommended execution order

1. Qualify ACORN `aa36e93f` or a newer accepted 0.4.0 revision and implement
   MCP-enabled tenant tools.
2. Assign a compatible macOS runner, make both tap jobs required, merge the
   v1.0.3 formula, and publish the verified Homebrew command.
3. Implement secure acquisition and the environment/concurrency phases as
   separately gated changes while preserving the landed sandbox and memory
   contracts.
4. Run the A/P/O memory gate only after a rollout decision and credentials are
   available.
5. Start Kubernetes only after its operator-demand, storage, image, and cluster
   gates are approved.

## Planning gaps

Roadmap milestones and deferred ideas are not schedulable plans. Create a
focused page with current evidence, ownership, validation, and acceptance
criteria before starting process guards, another sandbox provider, or any
other deferred scope.

## Adding a plan

1. Create one semantically named Markdown file for one atomic topic.
2. Add `type: Plan`, `title`, `description`, `difficulty`, `impact`, a non-empty `package` list, `tags`, and `status` to YAML frontmatter.
3. Use `low`, `medium`, `high`, `mixed`, or `not-assessed` for difficulty; use `low`, `medium`, `high`, `critical`, `mixed`, or `not-assessed` for impact.
4. Use `planned`, `in-progress`, or `waiting-on-validation` for status. Add
   `priority: next` only for an immediate unblocked candidate.
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
