---
type: Plan
title: "Restore rootless BuildKit sandbox image delivery"
description: "Provision a narrowly confined BuildKit runner, validate sandbox image builds, and restore protected publication."
difficulty: medium
impact: high
package: [ci, sandbox-runner]
tags: [buildkit, rootless, seccomp, gitlab-runner, containers, release]
status: planned
execution: phased
phase_budget_hours: 5
---

# Goal

Restore deterministic sandbox image builds and dual-registry publication on a
dedicated non-privileged GitLab Runner whose seccomp policy permits only the
namespace operations required by rootless BuildKit.

# Current evidence

Repository CI and history rechecked on 2026-11-13 at clean baseline
`e3f0e09`; the original failure is preserved in GitLab job
[4503475](https://code.ornl.gov/research-enablement/xylem/-/jobs/4503475).

- `build-geospatial-sandbox-workload`, `build-sandbox-runner`,
  `publish-geospatial-sandbox-workload`, and `publish-sandbox-runner` use the
  pinned `moby/buildkit:rootless` image and the generic `amd64` runner tag.
- The jobs already set `BUILDKITD_FLAGS=--oci-worker-no-process-sandbox`, so
  the failure precedes evaluation of either sandbox `Containerfile`.
- Job 4503475 ran on self-managed Docker executor runner `RE Runner (kronk)`
  and failed while RootlessKit started its child with
  `fork/exec /proc/self/exe: operation not permitted`.
- [GitLab's BuildKit guidance](https://docs.gitlab.com/ci/docker/using_buildkit/#error-forkexec-procselfexe-operation-not-permitted)
  identifies this exact error as a runner seccomp restriction. It recommends a
  custom profile that permits only required calls and warns against
  `seccomp:unconfined`.
- All four rootless BuildKit jobs now begin their existing `rules` with
  unconditional `when: never`; their prior activation rules remain in place
  for phased re-enablement.

No dedicated-runner policy or successful re-enablement evidence was found in
the repository. Runner-host security configuration, administration access,
and a maintenance window were not qualified during this review. All three
phases remain; do not infer readiness from a generic runner being online.

# Project code boundaries

- **Update:** `.gitlab-ci.yml` runner tags and temporary skip rules; this plan
  and its portfolio coordination entries as phases complete.
- **Create:** a dedicated GitLab Runner registration and a custom BuildKit
  seccomp profile on one self-managed amd64 host. These are external runner
  administration artifacts, not repository files.
- **Delete:** the four temporary unconditional skip rules after their owning
  validation phases pass.

# Dependencies and ordering

Runner administration precedes repository re-enablement. Build-only jobs must
pass on the dedicated runner before either protected publisher is restored.
RBK-02 and RBK-03 share the delivery integration lock with other CI and release
work and must run serially with those owners.

# Decision

Use a dedicated `buildkit-rootless-amd64` runner tag instead of changing the
security policy for every generic `amd64` job. Keep the Docker executor
non-privileged. Derive a narrow seccomp profile from the runtime's current
default, adding only calls and argument forms demonstrated by RootlessKit and
BuildKit startup. Do not use `seccomp:unconfined` or privileged mode.

# Validation

Validate runner registration and the effective security policy before changing
job tags. Then validate local OCI exports separately from credentialed
publication. A failure outside a phase's stated outcome becomes a bounded
repair phase rather than broadening the active phase.

# Acceptance criteria

- A dedicated online runner accepts only `buildkit-rootless-amd64` work and
  runs the pinned rootless BuildKit image without privileged mode or an
  unconfined seccomp profile.
- Both build jobs produce non-empty metadata and OCI artifacts; the runner
  image's repeated build retains its reproducible digest check.
- Both publish jobs push canonical digest-addressed images to Savannah and
  GHCR from protected pipelines and emit their expected dotenv metadata.
- Generic `amd64` runners retain their current security boundary, and none of
  the four jobs remains unconditionally skipped after acceptance.

# Execution phases

## RBK-01 — Provision and qualify the dedicated BuildKit runner

- Status: blocked
- Budget: 5 active hours maximum
- Prerequisites: GitLab group-runner administration access and a maintenance window on one self-managed amd64 Docker runner host
- Exact write set: dedicated runner host `/etc/gitlab-runner/config.toml`, `/etc/gitlab-runner/seccomp-buildkit.json`, and GitLab group runner registration tagged `buildkit-rootless-amd64`
- Primary integration lock: runner security policy
- Outcome: A non-privileged dedicated runner starts the pinned rootless BuildKit daemon under a narrow custom seccomp profile while generic runners remain unchanged.
- Focused validation: `gitlab-runner verify`, inspection of the effective Docker executor configuration, and a minimal rootless BuildKit OCI export using the pinned image and custom profile.
- Handoff: Record the runner ID, host, runner and Docker versions, profile provenance, permitted policy delta, and smoke-test output; remove RBK-01 when the evidence is preserved outside `.wiki`.

## RBK-02 — Re-enable and validate sandbox image builds

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: RBK-01 and an available merge-request pipeline
- Exact write set: `.gitlab-ci.yml`, `.wiki/refactors/rootless-buildkit-runner.md`, `.wiki/index.md`, `.wiki/refactors/index.md`, `.wiki/plan-network.md`, `.wiki/parallel-execution.md`
- Primary integration lock: delivery
- Outcome: Both build-only sandbox jobs target the dedicated runner, no longer contain their temporary skip, and produce their expected OCI artifacts.
- Focused validation: GitLab CI lint; `build-geospatial-sandbox-workload` produces non-empty metadata and an OCI archive; `build-sandbox-runner` produces both archives and matching non-empty digests.
- Handoff: Preserve pipeline and job URLs plus artifact digests, remove RBK-02, and mark RBK-03 ready when the build evidence passes.

## RBK-03 — Re-enable and validate protected sandbox publication

- Status: planned
- Budget: 5 active hours maximum
- Prerequisites: RBK-02, protected Savannah and GHCR credentials, and a protected main or version-tag pipeline
- Exact write set: `.gitlab-ci.yml`, `.wiki/refactors/rootless-buildkit-runner.md`, `.wiki/index.md`, `.wiki/refactors/index.md`, `.wiki/plan-network.md`, `.wiki/parallel-execution.md`
- Primary integration lock: delivery
- Outcome: Both publishers target the dedicated runner, no longer contain their temporary skip, and publish canonical images to Savannah and GHCR.
- Focused validation: GitLab CI lint; both publish jobs report non-empty image digests; Savannah and GHCR resolve the pushed canonical digests; both dotenv reports contain the expected digest-qualified variables.
- Handoff: Preserve publication pipeline, registry digest, and runner evidence in release or operations documentation, then retire this plan and all index, network, and lane entries.
