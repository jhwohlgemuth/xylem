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

Reviewed **2026-11-13** against initially clean commit `e3f0e09` (workspace
version 1.0.7). All **7 executable plans** at that review were compared with current code,
adjacent tests, user/architecture documentation, generated CLI help, schemas,
CI/release configuration, retained measurement/platform evidence, and relevant
Git history. CodeGraph was used before code searches. The network and parallel
guide are coordination records; there are no executable Epics.

Read-only external checks covered the Homebrew tap's current formula, CI,
merged release MR/pipeline/jobs and runner inventory, plus published ACORN
v0.6.3 and its runner/socket implementation. No external settings, branches,
runners, environments, or product files were changed. Cluster/PVC availability,
Windows hosts, BuildKit host policy/maintenance access, credentials, and the
A/P/O rollout decision remain unqualified external prerequisites.

Fresh checks: `cargo test -p xylem-lib --lib --quiet` passed **336 tests, two
ignored**; `cargo test -p xylem-cli --test workflow --quiet` passed **44 tests,
three ignored**. These qualify local baseline behavior, not missing Windows,
cluster, macOS Homebrew, or credentialed holdout acceptance.
`cargo test -p xtask --quiet` passed all **7 tests**. Planning wiki validation
and `git diff --check` passed. Ignored tests were not promoted to acceptance.

Pruned the obsolete v1.0.3/!2 Homebrew activation/merge tasks and already
published install-command work; condensed completed acquisition phase logs
into links to durable evidence; removed retired environment-plan dependencies
from Kubernetes; replaced stale uncommitted-baseline and v13 schema claims.
The CLI variable argument plan was subsequently implemented and retired after
the xylem-lib and xylem-cli suites, generated help, Clippy, rustdoc, and the
executable example passed. **6 executable plans remain.** Root interpolation
and the basename cutover remain baselines for the broader expansion plan.

| Recommended lane | Plan | Impact / remaining difficulty | Status and next action | Blockers / evidence |
|---|---|---|---|---|
| 1 | [Configuration expansion](features/configuration-variable-expansion.md) | High / high | Ready: EXP-01 bounded core extraction; invocation bindings are available to later field phases. | Current core remains in `selection/roots.rs`; five structural-field gaps remain. Serialize later shared files. |
| 2, parallel validation | [Homebrew platform validation](features/homebrew-export.md) | High / low | Waiting on validation: BRW-01 macOS, BRW-02 required checks, BRW-03 clean consumers. | v1.0.7 already merged in [MR !5](https://code.ornl.gov/research-enablement/homebrew-xylem/-/merge_requests/5); Linux passed, macOS manual/optional. Mac runner online at review; credential-setting and clean-consumer evidence outstanding. |
| Conditional delivery lane | [Rootless BuildKit runner](refactors/rootless-buildkit-runner.md) | High / medium | Blocked: qualify dedicated runner in RBK-01, then re-enable builds/publishers. | All four jobs remain unconditionally skipped. Host policy, administration access, and maintenance window unqualified. |
| Conditional platform lane | [Windows acquisition](refactors/secure-module-acquisition.md) | Critical / low | Blocked: SEC-07 native Windows qualification and audit. | ACORN libraries remain pinned at 0.5.1; published CLI 0.6.3 still assumes the POSIX runner socket. Native host/support evidence missing. |
| Conditional rollout lane | [A/P/O memory validation](features/memory-aware-scheduling.md) | High / low | Blocked: APO-01 individual then APO-02 concurrent holdout. | No expanded A/P/O rollout evidence/decision; needs corpus and credentials. Earlier one-input GO does not qualify concurrency. |
| Conditional feature lane | [Kubernetes execution](features/kubernetes-workflow-backend.md) | Critical / high | Blocked: K8S-01 deployment assumptions before implementation. | No target/client/worker/Kind code or accepted operator cluster/PVC/auth/image evidence. Local context/cancellation/v15-v14 prerequisites are already landed. |

## Recommended execution order

1. Start EXP-01; named and positional bindings are available to later
   structural-field phases.
2. Prioritize EXP-02/03 input/output consistency, then sources, templates, and
   environment fields. Shared configuration, CLI, test, and documentation
   files serialize. Finish
   combined acceptance in EXP-11.
3. Run BRW-01 in parallel with disjoint repository work, then require both
   platform checks through a new tap CI change and verify clean consumers.
   The already-merged release update is not an outstanding task.
4. Start RBK-01 only when host administration and the maintenance window are
   confirmed. Keep the four image jobs skipped until their owning gates pass.
5. Execute SEC-07, APO-01, or K8S-01 only when their explicit platform,
   rollout, or deployment prerequisites arrive. These external gates do not
   block the ready variable work or shipped local runtime.

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
7. Add `# Execution phases`. Each phase has a stable ID, `planned`, `ready`, `blocked`, `in-progress`, or `waiting-on-validation` status, at most five active hours, prerequisites, an exact write set or named external system, one primary integration lock, a mergeable outcome, focused validation, and a handoff.
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
