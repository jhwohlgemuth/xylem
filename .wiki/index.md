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
- [Testing plans](testing/) - Hermetic unit, integration, and CLI coverage.

## Next execution phase

Stabilize the dependency and failure foundations before adding workflow behavior:

1. [Upgrade and narrow ACORN dependencies](migrations/acorn-0-2-upgrade.md)
2. [Replace the unmaintained YAML parser](migrations/maintained-yaml-parser.md)
3. [Introduce typed errors and workflow outcomes](refactors/typed-error-and-outcome-boundary.md)
4. [Make process execution lossless and deadlock-free](refactors/process-execution-contract.md)
5. [Add configurable thread support](features/configurable-thread-support.md)
6. [Isolate workflow environments and define concurrency](refactors/workflow-environment-and-concurrency.md)

The [hermetic workflow test suite](testing/hermetic-workflow-tests.md) may establish fixtures and host-independent helpers alongside the first three items, but assertions that depend on the new error or execution contracts land after those contracts.

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
