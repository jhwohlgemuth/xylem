---
type: Plan
title: "Add an explicit Kubernetes workflow execution backend"
description: "Submit one complete Xylem run as a Kubernetes Job with a shared PVC, reconnectable lifecycle commands, durable run evidence, and a versioned runner contract."
difficulty: high
impact: critical
package: [xylem-lib, xylem-cli, workspace]
tags: [feature, kubernetes, execution, job, pvc, orchestration, runner, release]
status: planned
---

# Goal

Add Kubernetes as an explicit workflow execution target. A local Xylem CLI
acts as the trusted controller, submits one `batch/v1` Job for one complete
workflow run, observes its lifecycle, and renders the durable result written by
the in-cluster Xylem worker.

The first release preserves Xylem's current execution semantics:

- one run owns one journal and one ordered configuration;
- top-level inputs use the existing bounded rolling admission policy;
- modules remain sequential within one input;
- completion, resume, GPU monitoring, reporters, and exit-code selection keep
  their existing meanings;
- local execution remains the default and requires no kubeconfig, cluster, or
  Kubernetes executable at runtime;
- a Kubernetes run is explicit and never selected as a fallback.

Kubernetes is an execution target, not a new `ContainerRuntime` and not a
generic security boundary. Cluster scheduling determines where the Xylem
process runs. A later sandbox policy may constrain workflow-controlled
processes inside that target, but Kubernetes support must not claim controls
that an arbitrary cluster does not enforce.

# Non-goals

The first release does not:

- translate Docker, Podman, or Apptainer module declarations into Pods;
- run one Job per input or one Job per module;
- upload a local workspace into cluster-only storage;
- download artifacts through the Kubernetes API;
- add object-store staging;
- accept arbitrary Pod or Job manifest patches;
- add a CRD, operator, controller Deployment, Helm chart, or admission webhook;
- inject Kubernetes Secrets or ambient controller credentials into the worker;
- provide automatic workflow retries after a Pod or node failure;
- claim that a Pod, namespace, service account, or security context is a
  qualified sandbox;
- make Xylem's remote-source allowlist an egress `NetworkPolicy`;
- mount a host Docker or Podman socket into the worker;
- support a submitter that cannot access the same underlying shared filesystem
  as the configured PVC.

# Current evidence

Rechecked against the 2026-09-30 worktree, including the in-progress GPU
monitoring changes.

- `xylem-cli/src/commands/run/mod.rs` currently owns configuration precedence,
  run-ID creation, the temporary workspace, planning, journal creation, module
  preparation, container lifecycle, input dispatch, monitoring, reporting, and
  final exit mapping in one function. There is no reusable controller/worker
  boundary.
- `Config::run_workflow_recorded` and `Script::run_checked` execute on the local
  host. Working directories, local module URIs, input paths, completion paths,
  and report paths are therefore host paths.
- `ContainerEnvironment` wraps module commands in `docker exec`, `podman exec`,
  or `apptainer exec`. Managed Docker and Podman containers are created and
  inspected through local runtime CLIs. Adding `Kubernetes` to
  `ContainerRuntime` would leave path, storage, lifecycle, reporting, and
  concurrency behavior undefined.
- Managed containers are deduplicated by `(ContainerRuntime, name)`, prepared
  once per run, journaled as run-scoped resources, and cleaned after input
  dispatch. A Kubernetes Job cannot safely inherit this behavior through a
  new match arm.
- `ExecutionPlan` is deterministic and write-free, but it currently describes
  only local acquisition, module commands, local container operations, resume,
  and the in-progress GPU monitor. It has no execution-target evidence.
- The current worktree uses execution-plan, journal, and report version 7 for
  CEL selection provenance and durable dashboard state. The dashboard owns the
  coordinated version-7 runtime contract; Kubernetes must advance that
  contract to version 8 rather than modifying version-7 artifacts in place.
- `RunJournal` is append-only, flushed at every transition, and recovered into
  `RunSummary`. It is the correct authoritative worker result once its path is
  placed on durable shared storage.
- A random run root is currently created under the controller's operating
  system temporary directory. That directory cannot be assumed to exist in a
  Pod and has no controller/worker mapping contract.
- Dry-run intentionally performs no writes, network requests, process spawns,
  or runtime actions. Kubernetes dry-run must preserve that contract.
- The CLI exposes generated help as a checked-in contract. New target and
  lifecycle commands require regenerated snapshots and explicit tests.
- The workspace publishes a static Linux binary and Windows binary, but no OCI
  runner image. The rigorously supported binary target is Linux amd64; the
  Kubernetes release will deliberately expand its reference image to Linux
  amd64 and arm64.
- The planned sandbox work already distinguishes application containers from
  an outer execution or isolation session. Kubernetes target types must remain
  separate from `ContainerEnvironment` and `SandboxPolicy`.

# Core decisions

## One Job represents one complete run

Use one Kubernetes Job with one worker container for the complete Xylem run.
The worker executes the existing input scheduler and module sequence inside
the Pod.

This keeps one journal owner, preserves run-wide monitoring and fail-fast
semantics, and avoids distributed journal aggregation. One-Job-per-input and
one-Job-per-module designs would need idempotent distributed retries, artifact
handoffs, cross-Job completion state, and a new scheduling contract; they are
deferred.

The Job contract is fixed:

- `apiVersion: batch/v1`;
- `completions: 1`;
- `parallelism: 1`;
- Pod `restartPolicy: Never`;
- Job `backoffLimit: 0`;
- no automatic workflow retry;
- one Xylem worker container;
- one configured PVC mounted read-write;
- read-only ConfigMap, Secret, host-path, runtime-socket, and device mounts are
  not generated by Xylem v1.

Kubernetes documents that a Job can sometimes start the same program more than
once even with one completion and one parallel Pod. The shared run directory
therefore carries an execution lock; scheduler settings alone are not treated
as exactly-once execution. See the upstream
[Job execution model](https://kubernetes.io/docs/concepts/workloads/controllers/job/).

## The controller and worker use the same PVC-backed filesystem

Require an operator-provisioned PVC and a shared filesystem visible to both
the submitting host and the Job. An operator profile defines one canonical host
root and its Pod mount root.

The controller reads, validates, and writes run files through the host root.
The worker reads and writes the same files through the Pod root. No artifact is
encoded into a ConfigMap, log stream, annotation, or termination message.

This choice is intended for large research data. It avoids API-streamed tar
archives, ConfigMap size limits, result truncation, and a second data-transfer
protocol. Kubernetes Volumes provide storage whose lifetime is independent of
one container instance; PVC provisioning and durability remain operator
responsibilities. See the upstream
[volume documentation](https://kubernetes.io/docs/tasks/configure-pod-container/configure-volume-storage/).

## Cluster authority lives in an operator profile

Do not place kubeconfig context, namespace, service account, PVC, node
placement, or runner-image authority in workflow configuration. A workflow is
portable input; the target profile is trusted operator input.

The CLI selects the profile explicitly. Workflow configuration cannot broaden
the profile, substitute another image, request a service-account token, add
mounts, or weaken the generated security context.

## Kubernetes is not a sandbox claim

Generate conservative Pod defaults, record them in the plan, and document what
they do. Do not claim that those defaults enforce tenant isolation, egress
rules, disk quotas, artifact safety, or protection from a compromised node.

The execution target and sandbox policy remain orthogonal:

```text
workflow semantics
  -> execution target: local | kubernetes
  -> optional sandbox policy inside that target
  -> process runner
```

A future sandbox implementation may qualify a particular cluster profile. The
initial Kubernetes target advertises scheduling and process-placement
capabilities only.

## Support Kubernetes 1.31 and newer

Use only stable `core/v1`, `batch/v1`, and authorization APIs available in
Kubernetes 1.31. Test the minimum version and the newest supported upstream
release. Do not use alpha or beta fields, restartable sidecars, Indexed Jobs,
custom scheduling APIs, or a CRD.

The live preflight rejects an API server below 1.31. A newer server is accepted
when discovery confirms the required stable resources. The compatibility claim
is wire/API compatibility, not a promise that every CSI driver, scheduler,
network plugin, or admission policy behaves identically.

# User-facing contract

## Run target selection

Add a closed execution-target enum:

```text
local
kubernetes
```

`local` is the default. The explicit Kubernetes invocation is:

```shell
xylem run \
  --target kubernetes \
  --kubernetes-profile ./kubernetes-target.yaml \
  --config ./config.json
```

Add `--detach` only to `run`:

```shell
xylem run \
  --target kubernetes \
  --kubernetes-profile ./kubernetes-target.yaml \
  --config ./config.json \
  --detach
```

CLI validation rules:

- `--kubernetes-profile` is required exactly when `--target kubernetes` is
  selected;
- `--detach` is valid only for Kubernetes;
- `--env` is rejected for Kubernetes in the first release;
- existing workflow options retain their precedence and are serialized into
  the worker request after resolution;
- `--artifact-output`, `--report-output`, the configuration path, and every
  other known host path must satisfy the workspace mapping rules;
- `--dry-run` parses and validates the profile but performs no API call and
  creates no run directory;
- `--no-progress` affects the controller presentation and is always applied to
  the non-interactive worker;
- the selected reporters are rendered by the controller after it validates the
  shared journal; the worker does not compete for controller stdout.

## Lifecycle commands

Add a top-level `kubernetes` command with these subcommands:

```shell
xylem kubernetes check --profile PROFILE
xylem kubernetes status RUN_ID --profile PROFILE
xylem kubernetes logs RUN_ID --profile PROFILE
xylem kubernetes logs RUN_ID --profile PROFILE --follow
xylem kubernetes cancel RUN_ID --profile PROFILE
```

Behavior:

- `check` validates the local profile and mapping, connects to the selected
  cluster, verifies its version, resolves the namespace, verifies the PVC, and
  checks required caller permissions. It does not create a Job.
- `status` reads the shared request/result when present, queries the labeled
  Job and Pods when they still exist, and returns one stable state:
  `submitted`, `pending`, `running`, `succeeded`, `failed`, `cancelled`, or
  `unknown`.
- `logs` selects every Pod carrying the exact run-ID label. A normal run has
  one Pod; multiple Pods are shown with Pod prefixes in stable creation order.
- `logs --follow` reconnects from the last observed timestamp/resource without
  silently dropping the transition to a replacement or duplicate Pod.
- `cancel` is idempotent. It records controller cancellation intent in the
  shared run record, deletes the Job with foreground propagation, observes the
  configured termination grace period, and reports the final shared result if
  the worker produced one.
- Cancelling a terminal run does not rewrite its result.
- If the Job TTL has removed Kubernetes objects, `status` still reports the
  validated durable result from the shared run directory.

Attached `run` submits, follows logs, waits for a terminal result, renders the
summary, and returns the worker exit code. A controller network failure or
process crash does not delete the Job. An explicit interrupt of an attached run
requests cancellation; users who want disconnect-tolerant operation select
`--detach`.

## Operator profile schema

Use strict YAML with this initial shape:

```yaml
api_version: xylem.ornl.gov/v1alpha1
kind: KubernetesTarget
context: research-cluster
namespace: xylem
runner:
  image: registry.example/xylem-runner@sha256:0123456789abcdef...
  image_pull_secrets:
    - research-registry
  service_account_name: xylem-runner
workspace:
  claim: xylem-workspace
  host_root: /shared/xylem
  mount_root: /workspace
resources:
  requests:
    cpu: "1"
    memory: 1Gi
  limits:
    cpu: "4"
    memory: 8Gi
    nvidia.com/gpu: "1"
scheduling:
  node_selector:
    accelerator: nvidia
  tolerations:
    - key: accelerator
      operator: Equal
      value: nvidia
      effect: NoSchedule
  priority_class_name: research-batch
  runtime_class_name: null
job:
  active_deadline_seconds: 86400
  termination_grace_period_seconds: 30
  ttl_seconds_after_finished: 86400
```

Required fields:

- `api_version`, exactly `xylem.ornl.gov/v1alpha1`;
- `kind`, exactly `KubernetesTarget`;
- `runner.image`, including an OCI digest;
- `workspace.claim`;
- `workspace.host_root`;
- `workspace.mount_root`.

Defaults:

- `context`: the active kubeconfig context;
- `namespace`: the context namespace, then `default`;
- `runner.image_pull_secrets`: empty;
- `runner.service_account_name`: namespace default service account;
- `resources.requests` and `resources.limits`: empty;
- `scheduling.node_selector` and `scheduling.tolerations`: empty;
- priority and runtime class: absent;
- `job.active_deadline_seconds`: absent;
- `job.termination_grace_period_seconds`: 30;
- `job.ttl_seconds_after_finished`: 86400.

Validation:

- deny unknown fields at every level;
- reject empty names and values;
- validate namespace, claim, service account, Secret, priority-class,
  runtime-class, label, and toleration fields with Kubernetes syntax;
- validate resources as Kubernetes quantities without changing their string
  representation;
- require non-zero positive time values;
- require `mount_root` to be an absolute, normalized, non-root POSIX path;
- canonicalize `host_root`, require it to be a directory, and reject a symlink
  or special-file root;
- require the runner image to contain exactly one digest and reject tag-only
  images;
- compute a deterministic digest of the canonical profile, excluding no
  fields because credentials are referenced by name rather than embedded;
- never accept inline kubeconfig credentials or registry tokens.

The profile intentionally omits affinity, topology-spread constraints,
host networking, host namespaces, devices, arbitrary volumes, env vars,
container commands, probes, security-context overrides, and PodSpec patches.
Add a field only after a concrete scheduling requirement and merge policy are
defined.

## Runner compatibility contract

The controller writes a versioned request and launches the same Xylem binary in
worker mode:

```text
xylem __kubernetes-worker --request /workspace/.xylem/kubernetes/<run-id>/request.json
```

The hidden command is an implementation protocol, not a user-facing workflow
API. The request contains:

- protocol version;
- exact controller Xylem version;
- run ID;
- canonical profile digest;
- mapped configuration path;
- mapped run root, artifact root, report root, journal path, and result path;
- the resolved ordered input set;
- resolved CLI-over-configuration booleans and lists;
- thread and batch limits;
- reporter-independent execution options;
- expected static-plan digest, excluding filesystem observations that may
  legitimately change before the worker starts;
- Kubernetes namespace, Job name, runner image digest, and PVC claim.

The worker rejects a request unless its Xylem version and protocol version
match exactly. It never reads kubeconfig or calls the Kubernetes API.

# Path and storage contract

## Run-directory layout

Every real submission owns this durable directory:

```text
<host_root>/.xylem/kubernetes/<run-id>/
├── config.json
├── request.json
├── submission.json
├── worker.lock/
├── journal.jsonl
├── result.json
└── work/
```

The same directory appears to the worker beneath `mount_root`.

- `config.json` is the flattened, canonical, Pod-path configuration.
- `request.json` is immutable once the Job is created.
- `submission.json` is an atomic controller-owned record of API identity,
  submission time, cancellation intent, and the last observed Job/Pod state.
- `worker.lock/` is acquired with atomic directory creation before any workflow
  side effect.
- `journal.jsonl` is the existing authoritative workflow journal.
- `result.json` is written atomically after journal finalization and contains
  protocol version, run ID, terminal status, stable exit code, journal path,
  bounded error text, and completion timestamp.
- `work/` replaces the worker's operating-system temporary run root and holds
  acquired modules and preparation state.

Do not automatically delete this directory when the Job TTL expires. Artifact
and run-data retention are shared-storage policy, distinct from Kubernetes
object cleanup.

## Mapping algorithm

Implement host-to-Pod and Pod-to-host mapping as pure component-aware
functions.

For an existing host source:

1. canonicalize `host_root` and the source;
2. reject special files and symlink escapes;
3. strip the exact root by path components;
4. append the relative components to normalized `mount_root`;
5. preserve the source's file/directory role in the plan.

For a future output:

1. resolve it according to the existing configuration and CLI precedence;
2. lexically normalize it without requiring the final file to exist;
3. canonicalize the nearest existing parent;
4. reject traversal or a parent outside `host_root`;
5. map the remaining components beneath `mount_root`.

Apply mapping to:

- the root configuration and every resolved local configuration source;
- local script and binary module URIs;
- path-based input directories and their discovered entries;
- absolute values supplied through input lists or `--input`;
- workflow output and CLI artifact/report output;
- completion markers and output patterns after expansion;
- template paths and module working directories;
- the generated run root and all protocol files.

Preserve relative and non-path input values literally. Treat an absolute input
value as a filesystem path: it must map beneath `host_root` or validation
fails. Preserve remote URIs unchanged.

Dry-run builds the host plan and projected Pod plan in memory. It must expose
both sides of each mapping without canonicalizing a non-existent output leaf,
writing the canonical configuration, or contacting the cluster.

# Plan, journal, and result schemas

## Execution-plan version 8

Add a required `target` object to every plan.

Local:

```json
{
  "backend": "local"
}
```

Kubernetes:

```json
{
  "backend": "kubernetes",
  "namespace": "xylem",
  "job_name_prefix": "xylem-workflow-",
  "profile_digest": "sha256:...",
  "runner_image": "registry.example/xylem-runner@sha256:...",
  "workspace_claim": "xylem-workspace",
  "host_root": "/shared/xylem",
  "mount_root": "/workspace",
  "resources": {
    "requests": {},
    "limits": {}
  },
  "scheduling": {},
  "job": {
    "active_deadline_seconds": null,
    "termination_grace_period_seconds": 30,
    "ttl_seconds_after_finished": 86400
  }
}
```

The dry-run plan omits a random run ID, concrete Job name, Pod identity, and
live server observations so repeated planning remains deterministic.

Retain the existing containers array. Kubernetes validation requires it to be
empty for every selected module and emits a module-qualified error otherwise.

Create `execution-plan-v8.schema.json` and make it the only supported execution
plan schema. Remove the superseded version-7 schema and reader when version 8
becomes current.

## Journal and run-summary version 8

Extend run metadata and recovered summary with target evidence:

- backend;
- profile digest;
- namespace;
- concrete Job name;
- runner image digest;
- workspace claim;
- Kubernetes server version observed by the controller;
- Pod name and UID supplied by the downward API;
- worker protocol and Xylem version.

Local runs record `backend: local` and no Kubernetes fields. Kubernetes worker
events remain ordinary module, input, container, completion, and GPU events;
do not duplicate Kubernetes watch events into the append-only workflow
journal. Controller observations belong in `submission.json`.

Create `run-summary-v8.schema.json` and advance the journal version only after
the environment/concurrency v7 contract lands. Preserve bounded error behavior
and never record kubeconfig content, access tokens, registry credentials, or
credential-bearing URLs.

# Controller architecture

## Native Kubernetes client

Use the Rust `kube` client rather than a `kubectl` subprocess. Kubernetes lists
`kube-rs` as a community-maintained client, so its version, feature set, and
transitive dependencies require explicit ownership and audit. See
[Kubernetes client libraries](https://kubernetes.io/docs/reference/using-api/client-libraries/).

Keep API-client dependencies in `xylem-cli`. `xylem-lib` owns only
API-independent target/profile/plan/protocol types.

Enable only the features required for:

- kubeconfig loading and context selection;
- rustls TLS;
- stable typed `core/v1`, `batch/v1`, version, and authorization resources;
- list/get/create/watch/delete and streaming Pod logs.

Do not enable controller-runtime, CRD derive, admission, WebSocket exec,
port-forward, OAuth flows, or OpenSSL unless a supported authentication path
proves they are required. Use a current-thread Tokio runtime at the synchronous
CLI boundary; do not introduce a second long-lived application runtime.

Support kubeconfig client certificates, bearer tokens, and exec credential
plugins. Respect standard kubeconfig path selection. Do not invent a separate
credential store.

## Internal client boundary

Define an internal client trait around the exact operations Xylem uses:

- discover server version;
- perform self-access reviews;
- get namespace and PVC;
- create/get/watch/delete Job;
- list/get/watch Pods by exact labels;
- stream one Pod's logs;
- read terminal container state and scheduling events needed for diagnostics.

Production uses the native adapter. Unit and CLI tests use a deterministic fake
that can inject watch closure, resource-version expiry, duplicate Pods,
permission denial, scheduling failure, image-pull failure, Job deletion, and
network interruption.

## Preflight

Real submission performs these checks before writing `request.json` or creating
a Job:

1. load and validate the profile;
2. validate and canonicalize the local shared root;
3. connect through the selected kubeconfig context;
4. require server minor version 31 or newer;
5. resolve and read the namespace;
6. read the PVC and require `Bound` phase;
7. verify the controller identity can create/get/watch/delete Jobs and
   get/list/watch Pods and Pod logs in that namespace;
8. validate that no selected module declares a module container;
9. validate and plan all host-to-Pod paths;
10. create the durable run directory and protocol files;
11. create the Job with server-side collision protection.

Do not treat self-access review as permanent authorization. Every API failure
still returns a typed operation and resource context.

If filesystem creation succeeds and Job creation fails, write a terminal
submission failure to `submission.json`, do not create a workflow journal, and
retain the run directory for diagnosis. A repeated submission never adopts or
overwrites a foreign Job with the same name.

# Job specification

## Identity and ownership

Derive a DNS-safe name from the configured workflow ID or configuration stem
plus the random run ID. Enforce Kubernetes length limits after normalization.

Apply stable labels:

```text
app.kubernetes.io/name=xylem
app.kubernetes.io/component=workflow-runner
app.kubernetes.io/managed-by=xylem
xylem.ornl.gov/run-id=<run-id>
xylem.ornl.gov/version=<sanitized-version>
xylem.ornl.gov/profile=<short-profile-digest>
```

Use exact run-ID and Job UID selectors when following Pods. Never delete a
resource that lacks the expected managed-by, run-ID, and profile labels.

Set `ttlSecondsAfterFinished` from the profile. The upstream TTL controller
removes the Job and dependent Pods after the configured terminal retention
period; shared run data remains. See
[TTL-after-finished](https://kubernetes.io/docs/concepts/workloads/controllers/ttlafterfinished/).

## Container and Pod security defaults

Generate these non-configurable defaults:

- Pod and container `runAsNonRoot: true`;
- `allowPrivilegeEscalation: false`;
- drop all Linux capabilities;
- `seccompProfile.type: RuntimeDefault`;
- read-only container root filesystem;
- no privileged mode;
- no host PID, IPC, network, users, devices, or runtime sockets;
- `automountServiceAccountToken: false`;
- explicit `terminationGracePeriodSeconds`;
- writable memory- or disk-backed `emptyDir` mounts for `/tmp` and the runner
  home/cache only;
- PVC mounted once at `mount_root` read-write;
- worker command and arguments generated by Xylem, never supplied by the
  workflow or profile.

The optional service account still affects admission and image-pull policy,
but its API token is not mounted. Image pull Secret names are references only;
their contents never pass through Xylem.

These are defense-in-depth defaults, not a sandbox claim. Admission may make
them stricter. If admission rejects the Job, surface the exact API status and
do not weaken the spec automatically.

## Resources and scheduling

Copy validated resource requests and limits without interpreting business
meaning. This supports standard CPU/memory/ephemeral-storage quantities and
extended resources such as `nvidia.com/gpu`.

Copy only the typed scheduling subset:

- node selector;
- tolerations;
- priority class name;
- runtime class name.

The execution plan records this effective subset. Admission-time mutation is
outside Xylem's control; the controller records returned Job/Pod identity but
does not claim that the submitted and admitted PodSpecs are identical.

`--threads` and `options.batch` retain their Xylem meanings. Do not derive them
from CPU requests or silently clamp them. Documentation must tell operators to
configure consistent values.

## Offline behavior

When Xylem's resolved offline option is true:

- set runner `imagePullPolicy: Never`;
- preserve existing rejection of remote acquisition;
- do not claim the cluster itself is network-isolated;
- fail clearly when the digest-pinned runner image is not already present on a
  scheduled node.

When offline is false, use `IfNotPresent`. Never use `Always` with a digest.

# Worker lifecycle

## Exactly-one run claim

Before creating the journal, the worker atomically creates `worker.lock/` and
writes its Pod name, UID, and start time inside it.

- The winner validates the request, creates the journal, and executes.
- A second Pod never runs workflow code. It waits for a valid `result.json`
  until the Job deadline or a bounded duplicate-worker timeout, then exits with
  the recorded status.
- If the winner disappears without a result, the duplicate fails closed. It
  does not take over an ambiguous partial run.
- The controller reports a missing-result infrastructure failure and leaves the
  journal/work directory for `--resume` or manual diagnosis.

This contract assumes atomic directory creation and rename on the shared
filesystem. Document that storage requirement and exercise it in integration
tests.

## Execution

The winning worker:

1. reads immutable `request.json` and canonical `config.json`;
2. verifies protocol, exact Xylem version, run ID, profile digest, and expected
   static-plan digest;
3. verifies its downward-API Job/Pod identity matches the request labels;
4. sets its explicit run/work/journal paths from the request;
5. creates the v8 journal with Kubernetes target evidence;
6. verifies requirements inside the runner image;
7. acquires and prepares selected modules inside the shared work directory;
8. starts GPU monitoring when requested;
9. dispatches the resolved inputs through the existing batch/thread scheduler;
10. finalizes container-free workflow state, monitoring, and journal;
11. writes `result.json` with a temporary file, flush, sync, and atomic rename;
12. exits with the stable Xylem exit code recorded in the result.

The worker never renders a JSON reporter to Kubernetes logs as the durable
result. Logs are diagnostics; the shared journal and result are authoritative.

## Termination and cancellation

Install a worker cancellation token and handle Pod termination:

- on `SIGTERM`, stop admitting inputs;
- terminate and reap owned child processes through the process-execution
  contract;
- stop GPU monitoring;
- transition active input/module state to interrupted or failed according to
  the journal vocabulary;
- finalize an interrupted run when the grace period permits;
- atomically write the interrupted result;
- exit before Kubernetes sends `SIGKILL`.

If the process cannot finalize before the grace period, journal recovery and
the controller's Pod terminal state expose the interruption. Do not fabricate
a succeeded or cleanly cancelled result.

# Existing feature interactions

## Module containers

Reject every selected inline or file-backed template that resolves to a
`ContainerEnvironment`, whether managed or external. Report the qualified
module path and runtime before submission.

Do not:

- invoke Docker, Podman, or Apptainer inside the runner;
- mount a runtime socket;
- create sidecars from module declarations;
- reinterpret `container.name` as a Pod or container name;
- silently ignore the declaration.

A future module-image design needs its own plan covering Pod composition,
per-module images, environment managers, shared volumes, command dispatch,
readiness, and reporting.

## Requirements and environments

Run `verify_requirements` in the worker, because the runner image—not the
controller host—owns executable availability.

The reference image supplies only its documented baseline. Conda, Mamba, Pixi,
Poetry, NVIDIA tools, and domain applications require a custom compatible
image. Virtual-environment preparation continues in the worker's shared work
directory and writable home/cache.

## Remote acquisition and allowed hosts

The controller resolves reusable configuration modules before it writes the
flattened canonical configuration. Executable module acquisition remains a
worker operation.

`options.allowed` continues to constrain Xylem's own HTTP/Git source handling.
It does not create or verify Pod DNS, IP, port, protocol, redirect, private
network, or metadata-endpoint policy. Namespace/network policy is an operator
responsibility until a qualified sandbox backend can enforce and attest it.

Private remote credentials are unsupported in the first Kubernetes release.
Do not forward ambient controller environment variables, SSH agents, home
directories, kubeconfig credentials, or dotenv values.

## Resume and completion

Resolve the ordered input set on the controller and persist it in the request.
The worker uses that exact list rather than rediscovering a changing directory.

The controller's dry-run may inspect completion through host paths. Before
execution, the worker rebuilds the mapped plan and validates its digest. A
changed configuration, input list, module template, variables, or completion
declaration fails closed. Output existence may legitimately change between
planning and execution; the worker performs the authoritative resume
inspection immediately before module execution and records the final decision.

## GPU monitoring

The existing explicit GPU-monitor option is forwarded to the worker. The
worker owns `nvidia-smi` discovery and samples because only the scheduled Pod
has the effective device assignment.

Xylem does not automatically enable GPU monitoring from an extended-resource
request and does not automatically request a GPU from `--gpu-monitor`. The
profile and workflow option must both be configured intentionally. Missing
GPU tooling retains the existing typed monitoring failure behavior.

# Failure and exit semantics

Classify failures by stage:

- profile/configuration/path errors: usage or data errors before API access;
- kubeconfig/authentication/RBAC/API availability: unavailable errors;
- namespace/PVC/preflight failures: unavailable with exact resource context;
- durable run-directory creation: cannot-create or I/O errors;
- Job admission or scheduling: unavailable;
- image pull and startup: unavailable;
- worker protocol/version mismatch: configuration/software error;
- worker requirement/preparation failure: existing unavailable classification;
- module failure: existing software classification;
- deadline, eviction, OOM, signal, cancellation, or missing result: typed
  infrastructure/interrupted outcomes with Pod termination context;
- controller reporting failure after a valid worker result: preserve the
  worker failure as primary and attach reporting failure as secondary.

Attached mode returns the worker's recorded exit code when `result.json` and
the journal validate. If no trustworthy result exists, derive a controller exit
code from the Job/Pod failure category and state that workflow completion is
unknown.

Never infer success solely from Job `Complete`; require a valid matching result
and recoverable finalized journal. Never infer workflow failure solely from a
lost log stream.

# Reference runner image

Publish an OCI manifest list for Linux amd64 and arm64 with every Xylem release.

The reference image contains:

- the exact release's static Xylem binary;
- a POSIX shell;
- Git;
- CA certificates;
- a numeric non-root user and group;
- an empty non-root home directory;
- OCI labels for source revision, Xylem version, worker protocol, licenses, and
  documentation.

It does not contain:

- Docker, Podman, Apptainer, containerd, or a runtime socket;
- Conda, Mamba, Pixi, Poetry, CUDA, or `nvidia-smi`;
- kubeconfig, service-account tokens, registry credentials, or SSH keys;
- example data or a writable application root.

Build each architecture from the corresponding static binary in a pinned
minimal base. Run as the non-root user in the image metadata. Publish:

- per-architecture digests;
- the manifest-list digest;
- an SBOM;
- provenance tying the image to the release commit and binary checksums;
- a sample profile pinned to the manifest-list digest.

Profiles may select a custom image. Compatibility requires the exact Xylem
version, worker protocol, non-root/read-only-root behavior, and documented
writable paths. The controller does not probe an image by launching an
untracked Pod; incompatibility fails in the versioned worker handshake.

# Project code boundaries

## Update

- CLI command definitions, main dispatch, generated help, and run
  orchestration.
- Configuration/planning types and semantic validation needed for target
  selection, mapped plans, and module-container rejection.
- Journal metadata, recovery, reporters, and versioned v8 schemas.
- Workspace and CLI manifests, lockfile, dependency policy, Makefile, and
  GitLab CI.
- Architecture, CLI, configuration, module, examples, release, and operator
  documentation.
- Existing CLI, plan, journal, and workflow tests where the target becomes a
  required field or orchestration is extracted.

## Create

- API-independent execution-target, Kubernetes profile, path-mapping,
  request/result, and plan-evidence modules in `xylem-lib`.
- Native Kubernetes adapter and controller lifecycle modules in `xylem-cli`.
- Fake Kubernetes client and API fixtures.
- Version-7 execution-plan and run-summary schemas.
- Reference runner Containerfile/build scripts and multi-architecture release
  metadata.
- Kind fixtures for shared storage, namespace, Role/RoleBinding, PVC, success,
  failure, cancellation, and duplicate-worker cases.
- A Kubernetes operator guide and runnable offline-smoke example profile.

## Delete

- Superseded version-7 execution-plan, journal, and summary schemas, readers,
  and successful compatibility fixtures when version 8 becomes current.

Local execution and existing container runtimes remain supported on their
current path.

# Dependencies and coordination

## Prerequisites

- Preserve the landed execution-plan version-6 CEL provenance and land the
  environment/concurrency version-7 contract first; Kubernetes owns version 8.
- Complete the lossless process result, timeout, cancellation, and child-reaping
  contract before claiming graceful Pod termination.
- Complete immutable workflow environment/run-context work before serializing
  a controller request; the Kubernetes worker must not depend on controller
  process-global environment mutation.
- Coordinate kube-rs/Tokio additions with the dependency-surface and security
  gate plan. That plan owns manifest policy and audit baselines.

Profile/path/protocol types, pure Job construction, fake-client tests, and
runner-image prototypes may proceed before all runtime prerequisites. The real
worker and cancellation integration wait for them.

## Relationship to sandboxing

Kubernetes target work and sandbox work both touch run orchestration, plans,
journals, CLI commands, and documentation, so their integration phases cannot
hold those locks concurrently.

Neither plan semantically replaces the other:

- Kubernetes target chooses where the Xylem engine runs.
- Sandbox policy chooses how workflow-controlled work is constrained.

Land common `RunContext`, structured process, cancellation, and target-evidence
types once. Do not make Kubernetes implement the sandbox provider trait unless
a later cluster-profile qualification plan defines enforceable capabilities.

## Delivery dependency

Default fake-client tests can land with ordinary unit/CLI CI. Required Kind and
multi-architecture image jobs consume the repaired CI/release pipeline and its
immutable release identity. Do not publish a reference image from an untagged,
unverified binary.

# Implementation phases

## Phase 0: dependency and API spike

- Pin a kube-rs/k8s-openapi/Tokio combination compatible with the workspace
  MSRV, musl binary, Windows controller build, Kubernetes 1.31 wire APIs, and
  supported kubeconfig authentication.
- Measure resolved package count, duplicate versions, clean build time, binary
  size, and audit results.
- Prototype create/watch/log/delete against Kind 1.31 and the newest release.
- Verify shared filesystem atomic directory creation and rename in both the
  controller and Pod views.
- Prototype amd64 and arm64 reference images and run the hidden worker version
  handshake under emulation or native runners.

Exit gate: the dependency/security review is accepted, required authentication
works without `kubectl`, stable APIs cover the lifecycle, and the PVC contract
passes on the intended storage class.

## Phase 1: reusable run orchestration

- Extract pure CLI/config precedence and plan preparation from the current run
  function.
- Add an explicit immutable run request, injected run ID, run workspace, and
  cancellation token.
- Separate local execution from reporter rendering and exit mapping.
- Preserve existing local command behavior and workflow tests byte-for-byte
  where output is contractual.
- Complete structured process cancellation and immutable child environment
  prerequisites.

Exit gate: local runs and examples pass through the new boundary without a
behavior or schema regression, and no production path requires global mutable
workflow state.

## Phase 2: target/profile/path/protocol model

- Add `ExecutionTarget`, strict profile types, pure validation, canonical
  hashing, and component-safe path mapping.
- Add the v1 request, submission, and result protocols with atomic persistence.
- Add version-8 plan/journal/summary target evidence.
- Add deterministic Job construction as a pure conversion from validated
  profile plus request metadata.
- Reject module containers and unsupported controller inputs during planning.

Exit gate: dry-run renders a deterministic v8 Kubernetes plan without writes
or API access, and all profile/path/protocol/Job tests pass without a cluster.

## Phase 3: native controller and worker

- Implement kubeconfig loading, live preflight, least-authority checks, Job
  submission, watches, logs, result validation, and typed failures.
- Add the hidden worker, durable lock, downward identity, explicit run root,
  signal handling, journal finalization, and atomic result.
- Add attached/detached run plus check/status/logs/cancel commands.
- Preserve primary/secondary errors and stable CLI exit codes.

Exit gate: fake-client fault tests cover every lifecycle operation, and Kind
success/failure/detach/reconnect/cancel/duplicate cases produce valid durable
summaries.

## Phase 4: image, CI, and documentation

- Build, scan, and publish amd64/arm64 reference images from immutable release
  binaries.
- Add Kind 1.31 and newest-version required jobs on capable Linux runners.
- Add architecture-specific image smoke tests and manifest/SBOM/provenance
  validation.
- Publish sample namespace RBAC, PVC/profile fixtures, operator guide, module
  compatibility table, and troubleshooting guidance.
- Update supported-target claims only after the required cluster and image
  jobs pass.

Exit gate: a tagged release publishes matching binaries, crates, docs, and a
multi-architecture runner digest, and the documented offline-smoke Job passes
on both supported image architectures.

# Validation

## Pure unit tests

- JSON/YAML profile equivalence, defaults, unknown fields, empty values, and
  deterministic profile hashing.
- Kubernetes names, quantities, durations, tolerations, resource maps, digest
  references, and namespace resolution.
- Host/Pod path mapping for exact roots, nested paths, sibling prefix attacks,
  `..`, symlinks, non-UTF-8 paths, missing output leaves, special files, and
  reverse mapping.
- Absolute path inputs versus literal relative input values.
- Run-ID and Job-name normalization, truncation, collision resistance, and
  label values.
- Job spec snapshots for default, resource/GPU, scheduling, offline, deadline,
  image-pull Secret, and service-account profiles.
- Security-context invariants that cannot be weakened by profile data.
- Request/result/submission round trips, version mismatch, digest mismatch,
  wrong run ID, partial files, truncation, and atomic replacement.
- Duplicate-worker winner, waiter, matching result, winner disappearance, and
  timeout cases.
- Version-7 schemas for both local and Kubernetes runs.

## Fake-client tests

- Kubeconfig/context/namespace resolution and authentication errors.
- Server below 1.31 and discovery missing a required stable resource.
- Missing/unbound PVC and every required RBAC denial.
- Job name conflict with Xylem-owned and foreign resources.
- Watch disconnect, expired resource version, reconnect, and terminal-state
  races.
- Pending scheduling diagnostics, admission rejection, image-pull failure,
  OOMKilled, active deadline, eviction, signal, non-zero worker exit, and Job
  completion without a result.
- Multiple Pods ordered and labeled correctly.
- Attached log interruption without workflow cancellation.
- Controller crash followed by status/log reconnection.
- Idempotent cancellation before scheduling, while running, while terminating,
  after success, and after TTL cleanup.
- Valid worker failure plus controller reporting failure preserves the worker
  error as primary.

## Kind integration matrix

Run required integration against:

- Kubernetes 1.31;
- the newest supported upstream minor;
- the reference amd64 image on the normal CI host;
- the reference arm64 image on a native arm64 runner or under required QEMU
  smoke when native capacity is unavailable.

Configure Kind with an explicit host directory mounted into the node and a
matching static PV/PVC. Do not use an implicit repository bind mount.

Scenarios:

- live `check` with allowed and denied identities;
- write-free dry-run proves no Kubernetes object or run directory is created;
- attached offline-smoke success and local summary rendering;
- detached submission, controller exit, status/log reconnection, and success;
- deterministic module failure and stable exit code;
- requirement failure inside the custom runner rather than on the controller;
- cancellation during preparation and execution;
- active deadline and forced termination recovery;
- missing runner image in offline mode;
- module-container rejection before Job creation;
- duplicate worker lock behavior;
- Job TTL removes resources while durable status remains readable;
- GPU request/profile propagation, with monitoring exercised only on a capable
  runner;
- custom compatible image success and version-mismatch failure.

## Regression and release checks

- All existing local unit, CLI, workflow, example, schema, and help tests pass.
- Local execution requires no kubeconfig and performs no Kubernetes discovery.
- Linux musl, Windows controller, amd64 image, and arm64 image builds pass.
- Dependency count, binary size, build time, duplicate versions, audit, and
  license policy are recorded and accepted.
- Reference images run as non-root with a read-only root filesystem.
- Manifest-list members match the published per-architecture binary versions
  and checksums.
- CI leaves the worktree clean after help, schemas, docs, image metadata, and
  generated artifacts are checked.

# Documentation and operations

Document:

- controller versus worker responsibilities;
- why Kubernetes is a target rather than a module runtime or sandbox;
- shared host/PVC filesystem requirements, supported semantics, and retention;
- exact controller RBAC verbs and a namespace-scoped Role/RoleBinding example;
- runner service-account token behavior;
- profile fields, defaults, and immutable image requirement;
- local-to-Pod path mapping with valid and rejected examples;
- reference versus custom runner contents;
- unsupported module containers and private credentials;
- interaction among resources, `--threads`, `options.batch`, GPU requests, and
  monitoring;
- attached, detached, reconnect, cancellation, TTL, and recovery workflows;
- Kubernetes 1.31+ compatibility and tested-version policy;
- debugging admission, scheduling, PVC, image-pull, worker, journal, and result
  failures;
- remaining security risks and the separate sandbox roadmap.

Provide a self-contained Kubernetes offline-smoke example that uses placeholder
namespace/PVC values, a digest placeholder updated at release time, no secret,
and a workflow rooted entirely beneath the shared workspace.

# Acceptance criteria

- `xylem run` remains local by default and existing local workflows retain
  their behavior.
- An explicit Kubernetes run submits exactly one stable-API Job for the whole
  workflow and never invokes `kubectl`.
- Dry-run performs no write or cluster call and includes deterministic v8
  target/path evidence.
- A real run validates Kubernetes 1.31+, namespace, PVC, RBAC, profile, paths,
  image digest, and module compatibility before Job creation.
- The controller and worker exchange only strict, versioned, durable protocol
  files on the configured shared filesystem.
- The worker owns requirement checks, acquisition, preparation, scheduling,
  monitoring, journal transitions, and terminal result inside the Pod.
- Duplicate Pods cannot execute workflow modules concurrently for the same run.
- Attached, detached, status, logs, and cancel flows are reconnectable and use
  the exact Xylem run ID.
- Job success is accepted only with a matching valid result and recoverable
  finalized journal.
- Cancellation and Pod termination attempt bounded child cleanup and journal
  finalization without inventing success.
- Module-level Docker, Podman, and Apptainer declarations fail before any
  Kubernetes resource is created.
- No kubeconfig credential, registry token, ambient environment, dotenv value,
  or service-account token is copied into the worker by Xylem.
- Plans and summaries identify the target, runner digest, profile digest,
  namespace, Job, PVC, and Pod without recording credentials.
- Required Kind tests pass on Kubernetes 1.31 and the newest supported release.
- The tagged release publishes verified amd64 and arm64 runner images under one
  immutable manifest-list digest with SBOM and provenance.
- Documentation states that Kubernetes execution is not a generic sandbox and
  accurately lists unsupported staging, secret, module-container, and retry
  behavior.

# Related plans

- [ACORN Xylem runtime prerequisites](https://code.ornl.gov/research-enablement/acorn/-/blob/main/.wiki/refactors/xylem-runtime-prerequisites.md)
- [Workflow environment and concurrency](../refactors/workflow-environment-and-concurrency.md)
- [Sandboxed workflow execution](../refactors/sandboxed-workflow-execution.md)
