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

Rechecked on 2026-10-04 against the current worktree, dependency network, the
live Homebrew tap merge request, and upstream ACORN MCP-client support. Quality measures whether a plan has
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
| [MCP-enabled tenant tools](features/mcp-tenant-tools.md) | 3/5 | High | High | High | Upstream ACORN now provides an explicitly configured, policy-checked MCP client over child-process and Streamable HTTP transports. Xylem is still pinned before that revision; define the workflow-facing contract before upgrading or exposing remote tools. |
| [Kubernetes workflow execution](features/kubernetes-workflow-backend.md) | 4/5 | Critical | Very high | Conditional high | The plan specifies the controller/worker, PVC, protocol, and failure contracts in detail. Delivery depends on shared-filesystem semantics, cluster runners, release images, and confirmed operator demand. Keep it separate from sandboxing and validate those deployment assumptions before implementation. |

## Consolidated and ranked portfolio

The seven source plans reduce to four portfolio-level plans. Existing pages
remain the detailed implementation chapters; physically merging their bodies
would recreate oversized plans and obscure the integration locks. Ranking uses
dependency readiness and value relative to remaining effort, while retaining
impact and difficulty explicitly.

| Rank | Consolidated plan | Source plans | Impact | Difficulty | Value and decision |
|---:|---|---|---:|---:|---|
| 1 | Release activation | Homebrew release activation | High | Low | Very high. Resume the nearly complete tap activation. |
| 2 | MCP-enabled tenant tools | MCP-enabled tenant tools | High | High | High. Upstream support removes the protocol implementation risk, but Xylem still needs an allowlisted workflow contract, plan/journal evidence, and policy tests. |
| 3 | Trusted local execution | Environment/concurrency + secure acquisition + sandboxing | Critical | Very high | High. These remain independent hardening plans. Sequence their integrations after preserving the landed runtime contracts. |
| 4 | Kubernetes execution target | Kubernetes workflow execution | Critical | Very high | Conditional high. It remains an orthogonal execution target with its own approval gate. Start from the landed execution boundary only when a supported cluster, shared PVC semantics, image pipeline, and user demand are confirmed. |

## Recommended execution order

1. Resume Homebrew activation and publish the verified installation path.
2. Define and prototype MCP-enabled tenant tools against the upstream ACORN client, with no remote-tool execution in Xylem until the policy and evidence contract is accepted.
3. Resume memory-aware scheduling when its external block clears; integrate it through the durable run state rather than making it a prerequisite.
4. Reassess environment/concurrency, secure acquisition, sandboxing, and Kubernetes against demonstrated operator demand and the landed execution boundary.

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
