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

## Next execution phase

The next dependency-ready runtime item is the upstream
[ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md),
followed by [workflow environment and concurrency](refactors/workflow-environment-and-concurrency.md).
Secure acquisition may proceed in its separate configuration/acquisition lane
after the same published ACORN contract is available.

Memory-aware scheduling is authorized for its measured workflow boundary and
remains in progress. The [workflow dashboard](features/workflow-dashboard.md)
follows the immutable runtime and memory scheduler. Host-independent fixtures
may continue alongside them.

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
