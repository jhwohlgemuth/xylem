# Workflow configuration

Xylem reads JSON, strict JSONC, YAML, and YML configurations. JSONC accepts
line and block comments plus trailing commas while retaining strict JSON syntax.
A workflow describes requirements,
zero or more inputs, shared variables, ordered modules, and execution options.

```json
{
  "version": "1.0.0",
  "description": "Run a local greeting module",
  "requirements": ["sh"],
  "input": ["first", "second"],
  "output": "/data/artifacts",
  "variables": {
    "GREETING": "Hello"
  },
  "modules": [
    {
      "name": "Greeting",
      "type": "script",
      "programmingLanguage": "generic",
      "uri": "file:./modules/greeting",
      "template": {
        "command": "sh",
        "arguments": ["run.sh", "{{ GREETING }}", "{{ INPUT }}"]
      }
    }
  ]
}
```

Relative `file:` module paths resolve from the directory where Xylem is
launched. Modules execute in array order for each active input.

## Opt-in sandboxing

Sandboxing is disabled by default. Omitting `sandbox`, or setting it to
`false`, preserves the existing host execution path and requires no sandbox
policy or container runtime.

Set the single top-level field below to require sandboxed execution:

```yaml
sandbox: true
```

An opted-in run also requires the operator-controlled global option
`--sandbox-policy <PATH>`. Supplying a policy does not activate sandboxing, and
supplying one for a workflow without `sandbox: true` is an error. A requested
sandbox never falls back to host execution.

Policy documents use strict JSON, JSONC, YAML, or YML. Version 1 separates the
lifecycle provider from its immutable workload and minimum isolation class:

```yaml
version: 1
provider:
  type: oci
  runtime: docker
workload:
  default: example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  allowed:
    - example.invalid/xylem@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
minimum_isolation: shared_kernel
process_guards: []
```

The supported providers are Docker Sandbox through the standalone local `sbx`
CLI, rootless OCI through Docker or Podman, and OpenShell through a Docker or
Podman compute driver. A Docker Sandbox policy uses
`provider: { type: docker_sandbox }`; an OpenShell policy uses
`provider: { type: openshell, driver: docker }`. The policy also owns allowed
host roots and child environment names, resource ceilings, preparation
networking, and cleanup. Process guards are reserved for later integrations
and must currently be empty. The reserved vocabulary is Bubblewrap, nono, and
nsjail.

Relative policy roots resolve from the policy file. Dry-run validates the
workflow and policy and records their effective values without contacting the
runtime. Cleanup defaults to `delete`; trusted operator policy may select
`retain_on_failure`, in which case failed resources remain explicitly identified
in the durable run summary until stale-resource reconciliation removes them.
The checked-in [`sandbox-policy-v1` schema](../schema/sandbox-policy-v1.schema.json)
defines the complete document shape. Real execution is currently qualified on
native x86_64 Linux with a local rootless runtime. Before opening a run session,
Xylem inspects the exact digest-pinned workload. An online run pulls that exact
reference once when it is absent, then re-inspects it; `--offline` requires the
workload to exist locally and never contacts a registry. The realized image must
be Linux/amd64, declare no entrypoint or environment outside the policy
allowlist, and pass a fixed network-disabled non-root bootstrap probe. Module
execution remains `--pull=never`.

The reproducible baseline runner is published to
`savannah.ornl.gov/research-enablement/xylem/runner` and
`ghcr.io/jhwohlgemuth/xylem`. Its CI artifact reports both canonical digest
references for operator policy. Custom workloads remain supported when they
satisfy the same metadata and bootstrap contract.

Before running native conformance, pull the exact digest and run the checked-in
preflight with the runtime, workload, and required available bytes:

```bash
docker pull "$XYLEM_SANDBOX_WORKLOAD"
sh scripts/check-rootless-sandbox.sh \
  docker \
  "$XYLEM_SANDBOX_WORKLOAD" \
  1073741824
```

The preflight rejects non-Linux and non-x86_64 hosts, rootful or remote
runtimes, non-v2 cgroups, an unavailable memory controller, insufficient
available memory, noncanonical workload references, non-amd64 images, and a
runtime that cannot enforce the hardened private-IPC, CPU, memory, PID, and
read-only-root contract or provide non-root-writable private preparation
storage. Use `podman` as the first argument for the Podman provider.

Set `XYLEM_SANDBOX_EVIDENCE_DIR` when running the ignored native conformance
test to retain deterministic `<runtime>-summary.json` and
`<runtime>-journal.jsonl` files outside its temporary fixture. The GitLab
Docker and Podman conformance jobs set this automatically and upload those
files even when a job fails after creating them.

### Docker Sandbox provider

`docker_sandbox` is a selectable experimental provider in sandbox policy v1.
It uses the standalone local `sbx` CLI versions `>=0.45.0,<0.48.0`; prerelease
versions and the removed `docker sandbox` plugin are not supported. Discovery
checks both `sbx version --json` and the local daemon through `sbx ls --json`.

Every input uses separate mountless preparation and execution microVMs created
from an exact digest-pinned Docker Sandbox template. `--offline` maps to
`--pull never`; online acquisition maps to `--pull missing`. Shared skills are
disabled. Xylem passes whole-core CPU limits and whole-MiB memory limits, with
a 512 MiB minimum, and runs workflow commands as UID/GID 1000 with a cleared,
explicit environment. Execution always adds `--deny-network "**"`.
Preparation adds the same deny rule when policy selects `deny`, while
`unrestricted` adds no sandbox-local network rule and remains subject to any
stricter host or organization policy.

The provider advertises separate-kernel isolation and network denial. It does
not claim read-only-root, aggregate resource-limit, or rootless capabilities
because the current interface does not provide portable enforcement or
evidence for read-only roots, PID ceilings, scratch ceilings, and rootless
operation. Xylem prints an experimental warning when the provider is selected.
The checked-in
[`docker-sandbox-provider` example](../../examples/docker-sandbox-provider/)
pins Docker's published multi-platform `shell` template and has both a public
dry-run test and an ignored native end-to-end test.

### OpenShell provider

OpenShell 0.1.x is a selectable experimental provider in sandbox policy v1.
Xylem prints a warning whenever an OpenShell policy is selected because the
current CLI does not guarantee offline workload acquisition or portable
per-sandbox PID and scratch-size ceilings. CPU and memory requests are passed
to OpenShell, while PID enforcement remains gateway-wide and scratch sizing is
provider-managed.

The checked-in [`openshell-provider` example](../../examples/openshell-provider/)
includes a public operator policy and exercises the adapter against an
authenticated local Docker or Podman gateway. The provider creates distinct
preparation and execution sandboxes, applies generated phase policies, stages
local content through validated archives, executes exact argument arrays,
downloads artifacts, and deletes or retains resources according to policy.
The ignored conformance test remains available for collecting detailed native
gateway evidence.

The OCI lifecycle uses a separate preparation container and execution
container for every admitted input. Xylem first acquires and verifies remote
sources through its content-addressed cache, then copies them into the owned
input workspace. Sandbox providers receive only those materialized local
sources; they do not contain a second URL-download or floating-Git path.
Preparation verifies workflow requirements inside the pinned workload and may
prepare `environment.yml` with conda or mamba, or `pixi.toml` with pixi, when
the selected workload provides the corresponding tool.

Preparation receives only read-only module sources and bounded private
storage. Its network is either denied or unrestricted according to operator
policy. The resulting archive is bounded by the scratch-byte and artifact-file
ceilings, rejects unsafe paths and entry types, and materializes only safe
links. Execution mounts the staged modules and current input read-only, uses a
bounded writable tmpfs, and always disables networking. Preparation and module
execution stay within the same scheduler admission for an input; sandbox
memory limits do not alter the scheduler's independent memory estimate.

When `output` or `--artifact-output` is configured, each input receives a
private bounded tmpfs at `/xylem/output`; the host exposes only one exact empty
archive file at `/xylem/export.tar`. Xylem bounds exported bytes and
entries with `resources.artifact_bytes` and `resources.artifact_files`,
validates archives after the execution container is quiescent, merges inputs
deterministically, verifies that every declared output glob matches a non-empty
regular file, synthesizes completion markers, and atomically renames the
complete tree to the absent final destination. The destination is never mounted
into a workload. Missing or empty declared outputs fail publication, and a
failed run leaves the destination absent. Sandbox artifact publication rejects
resume and requires enough free staging space for twice the per-input
byte-plus-entry allowance.

Nested-configuration, application-container, and GPU features fail closed in
sandbox mode while their existing host behavior remains available when
sandboxing is disabled.

The reusable geospatial workload under `sandbox/runner/geospatial/` contains
the dependency set used by the measured imagery workflow. Release pipelines
publish it as `savannah.ornl.gov/research-enablement/xylem/geospatial` and
`ghcr.io/jhwohlgemuth/xylem-geospatial`, with canonical digest references in
the `XYLEM_SANDBOX_GEOSPATIAL_WORKLOAD*` dotenv variables.

Explicit input lists preserve their declared order and duplicate values keep
their first occurrence. Directory-derived inputs are sorted by path. CLI
`--input` replaces configured input values rather than appending to them.

## Memory-aware admission

Any workflow can declare a positive memory budget and a complete estimator.
The estimator is independent of module names, order, count, and revisions:

```yaml
options:
  memory:
    budget: 64GB
    estimator:
      identity: my-workflow-v1
      intercept: 16GB
      maximum_input: 1TB
      minimum_input: 1KB
      safety_millionths: 1100000
      slope_millionths: 0
```

`identity` is a stable caller-chosen label for plans and future telemetry.
`intercept` is the fixed memory contribution. `slope_millionths` is the number
of memory bytes per input byte, scaled by one million. `safety_millionths` is a
safety multiplier scaled by one million and must be at least `1000000` (1.0).
The minimum and maximum input sizes define the model's calibrated range. A
zero slope is valid when the intercept is positive, allowing a conservative
fixed reservation for any module combination. Xylem validates the model's
values and arithmetic, but the workflow author remains responsible for
calibrating it for the configured modules and inputs.

For example, the measured `module-a-p-v2` input-byte model supports two-way
admission for the pinned Module A then Module P workflow over its calibrated
529,854,516–2,588,922,236-byte input range:

```yaml
options:
  batch: 2
  memory:
    budget: 72GB
    estimator:
      identity: module-a-p-v2
      intercept: 8870450.595703125KB
      maximum_input: 2528244.37109375KB
      minimum_input: 517436.05078125KB
      safety_millionths: 1150000
      slope_millionths: 23320131
```

The two measured holdouts reserve 28,634,438,485 and 41,120,796,343 bytes,
so both fit concurrently within this budget. Reuse this model only for the
measured module revisions and order; a different workflow needs its own
identity and evidence.

Execution-plan schema v13 records artifact publication, acquisition, and lifecycle intent, the budget,
complete estimator, and one
read-only sizing decision per resolved input. A regular file is sized by its
logical length; a directory is sized as the checked sum of its recursively
contained regular files. Symbolic links, special entries, missing metadata,
zero-input workflows, and sizes outside the calibrated range cause the entire
run to use `mode: fixed_fallback`. Real runs emit one warning for that fallback.

When every input can be sized, `mode: memory_aware` reserves the planned bytes
before admitting it. The sum of active reservations cannot exceed `budget`,
and `options.batch` and `--threads` remain independent upper bounds. A
reservation larger than the full budget is rejected before module work begins.
Omitting `options.memory` preserves fixed batching without a warning. There is
no CLI override or automatic host-memory detection.

When a configured run also uses an OCI sandbox policy on Linux, Xylem attempts
to record each input container's exact cgroup v2 `memory.peak` after execution
and before cleanup. The observation is telemetry only: it does not change the
reservation or admission decision. Unsupported platforms, host execution, and
unavailable cgroup files leave the observed peak null and do not fail the
workflow. Once collected, however, the peak must be written to the durable
journal or the input is failed rather than silently losing evidence.

## CEL input selection

A workflow can discover local files and directories beneath explicit roots and
select inputs with a CEL expression:

```yaml
input:
  select:
    engine: cel
    options:
      roots:
        - ./scenes
      recursive: true
    script: >-
      items
        .filter(item,
          item.kind == "file" &&
          item.size > 0 &&
          (item.extension == ".tif" || item.extension == ".tiff"))
        .map(item, item.path)
```

For readable multiline YAML expressions, `>-` folds ordinary line breaks into
spaces and removes the final newline. Use `|-` when the expression's newline
characters should be preserved; CEL accepts them as whitespace.

Relative roots resolve from the declaring configuration's directory. Roots
must be existing local directories and cannot be symlinks. Discovery never
follows symlinks, does not read file contents, and skips symlinks and special
files. Candidates retain configured root order and are sorted by `/`-separated
relative path within each root. Overlapping roots keep the first occurrence.

CEL receives only three immutable bindings:

- `items`, an ordered list of candidate records;
- `options`, containing canonical `roots` and the effective `recursive` value;
- `variables`, the configured workflow variables without reading the process
  environment.

Each item provides `index`, `path`, `relative_path`, `name`, `extension`,
`kind`, `size`, and `depth`. JSON/YAML null, boolean, number, string, list, and
map variable values map recursively to the corresponding CEL values. Nested
values passed to child processes are encoded as compact JSON strings.

The supported CEL subset includes literals, field selection, indexing,
conditionals, core comparison/arithmetic/logical operators, the list macros
`has`, `all`, `exists`, `exists_one`, `filter`, and `map`, and the pure
functions `size`, `contains`, `startsWith`, `endsWith`, `int`, `uint`,
`double`, and `string`. Macros may iterate over lists only. Bytes, regular
expressions, time, optional values, custom functions, and filesystem, process,
network, environment, clock, random, or locale access are unavailable.

The result must be `list<string>`, and every string must exactly equal a
discovered item's `path`. Result order is preserved and later duplicates are
removed. An empty result runs the workflow once without an input, matching the
existing zero-input behavior.

Discovery and evaluation use fixed limits: 16 roots, depth 32, 100,000 visited
entries, 50,000 candidates, 16 MiB of candidate paths, 250,000 metadata
operations, a 16 KiB script, 64 AST levels, 1,024 AST nodes, 10,000,000 units
of statically estimated evaluation cost, and 50,000/16 MiB intermediate and
result records/bytes. Variables are limited to depth 32, 10,000 nodes, and 1
MiB per scalar. Exceeding any limit fails before workflow execution.

A non-empty CLI `--input` list has precedence and bypasses selection request
validation, root access, discovery, compilation, and evaluation. Dry-run uses
the same read-only selection snapshot as execution. Execution-plan schema v13
records canonical options, limits, engine/interpreter identity, counts, and
SHA-256 script and inventory digests without recording the script, configured
variable values, or unselected inventory.

## Reusable configuration modules

Use a `configuration` module to expand another workflow in place:

```json
{
  "name": "Preprocess",
  "type": "configuration",
  "uri": "file:./workflows/preprocess.json",
  "variables": {
    "METHOD": "normalized"
  }
}
```

Nested workflows expand depth-first to executable leaves. Local paths are
relative to the declaring configuration. HTTP(S) `.json`, `.jsonc`, `.yaml`,
and `.yml` locations require an algorithm-qualified `checksum` and are read
only after SHA-256 verification. Git locations load repository-root
`config.json` and require a full lowercase 40- or 64-hex commit ID, for example
`{"scheme":"https","uri":"https://example.org/workflow.git","revision":"0123456789abcdef0123456789abcdef01234567"}`.

The root input and run options remain authoritative. Variables merge in
parent, wrapper, child, then leaf order; child outputs replace inherited output
defaults unless `OUTPUT_DIRECTORY` was explicitly set. Child requirements are
de-duplicated into the root workflow, and reports identify leaves with a
qualified `module_path`. Wrappers cannot declare `template` or `completion`.
Cycles and nesting beyond 16 wrappers are rejected before execution.

## YAML behavior

YAML configuration files may use anchors, aliases, and merge keys. Explicit
mapping fields take precedence over merged fields; for a merge sequence, the
first mapping takes precedence over later mappings. Quoting a scalar prevents
YAML from interpreting values such as `null`, `true`, or numbers as other
types.

Xylem accepts exactly one YAML document per file. It rejects duplicate keys,
unknown configuration fields, malformed merges, unresolved aliases, and input
that exceeds the parser's recursion or alias-expansion limits. JSON uses the
same required-field, unknown-field, and semantic validation policy.

## Variables

Workflow variables are available to every module. Module variables override
workflow values. The top-level `output` field supplies `OUTPUT_DIRECTORY` when
the workflow has not defined it; a module may override that default. Xylem then
supplies reserved values such as `MODULE_LIST`, `INPUT`, and
`PARENT_DIRECTORY`. Placeholders use `{{ VARIABLE_NAME }}` syntax.

Resolved values are passed to child processes with an `_XYLEM_` prefix. For
example, `GREETING` is available as `_XYLEM_GREETING`. Resolution is local to
each input and module command: Xylem does not write resolved values into its
own process environment, so concurrently active inputs cannot overwrite one
another's variables.

## Options

Configuration options include `acquisition_limits`, `allowed`, `batch`,
`dry_run`, `failure_policy`, `gpu_monitor`, `offline`, `resume`, and
`skip_verify_requirements`. Enabling a
Boolean CLI flag overrides a false or absent configuration value. When
supplied, `--allowed`, `--input`, and `--artifact-output` replace their
configured values.

`failure_policy` accepts `fail_fast` or `continue` and defaults to `fail_fast`.
Fail-fast stops admitting new inputs after a failure while allowing work that is
already active to settle. Continue keeps admitting independent inputs after a
failure. Either policy preserves configured result order and sequential module
order within each input.

Executable `script` and `binary` modules may declare a positive
`timeout_seconds`. Omission applies no module timeout. Configuration modules do
not execute and reject this field. The timeout bounds the managed module
process and its descendants; it excludes source acquisition and environment
preparation. Under a sandbox policy, the shorter of the module timeout and the
sandbox wall-time limit applies.

Checksum verification can only be disabled by the operator-only
`--skip-verify-checksum` flag. This emergency path applies to remote binaries,
emits a warning, records the observed digest, and cannot satisfy an offline
cache lookup because no trusted identity exists before download.

`gpu_monitor` enables run-wide NVIDIA GPU telemetry at a fixed one-second
interval. Xylem invokes `nvidia-smi` on the host, journals raw utilization and
frame-buffer memory readings, and reports compact per-GPU averages and peaks.
The readings are device-wide and may include activity from unrelated processes
on a shared host. Monitoring is represented in dry-run plans without invoking
`nvidia-smi`.

`resume` evaluates each module at top-level input granularity. A module is
skipped only when all declared output globs resolve to non-empty files and its
explicit completion marker contains the current canonical identity. Missing,
empty, partial, or stale output state runs normally.

`batch` is an optional positive integer that caps concurrently active top-level
inputs. Its effective limit is `min(batch, --threads)`; when omitted, input
execution retains the `--threads` limit. Admission is rolling, so another input
may start as soon as one finishes. For example:

```json
{
  "options": {
    "batch": 2
  }
}
```

The scheduling unit is each configured input value, or each immediate child of
a configured input directory. A module remains sequential within one input.
Xylem cannot count or limit image pairs, threads, or other work that a child
module processes internally.

`allowed` is an optional list of exact `HOST` or `HOST:PORT` authorities:

```json
{
  "options": {
    "allowed": ["code.ornl.gov", "artifacts.example.org:8443"]
  }
}
```

Host matching is case-insensitive and does not support wildcards or domain
suffix matching. A host without a port permits the default port for the URL's
scheme; a non-default port must be listed explicitly. An absent or empty list
denies remote module acquisition. The allowlist is enforced before remote work
and across HTTP redirects. HTTPS is the default remote transport;
`--allow-insecure-http` is an operator-only capability for local development
and cannot be enabled by a workflow.

`acquisition_limits` may tighten maximum document, binary, Git tree, Git
staging, and process-output sizes; HTTP/Git timeouts; redirect count; and Git
entry count. Nested workflows can only narrow inherited hosts and ceilings or
enable offline mode.

Remote binaries and direct configuration documents require a lowercase
`sha256:` digest. Git-backed scripts and configurations require a full
immutable commit ID. Xylem acquires those identities into a verified immutable
cache and copies content into the run workspace before execution. Offline mode
uses only matching verified cache entries; a miss never falls through to the
network. Dry-run validates the acquisition plan and may inspect the cache, but
does not connect, invoke Git, create a destination, change permissions, or
populate the cache. A cached remote configuration wrapper can therefore be
flattened during dry-run; a cache miss is reported without a request.

OCI module references use `oci://registry/repository@sha256:<64 lowercase hex>`.
Local OCI layouts use `oci-layout:<path>@sha256:<64 lowercase hex>`. Relative
paths follow `file:` module rules: top-level executable modules use the working
directory; configuration wrappers and nested modules use their source file.
Both forms are supported for script and configuration modules. Local layouts
need no allowed host and can be acquired while offline.
The digest pins the image manifest; its config and file-layer sizes are checked
against `git_staging`, and the extracted tree is checked against `git_tree` and
`git_entries` before it enters the verified cache. Script modules execute from
the artifact root. Configuration modules read `config.json` from that root.
Xylem checks the exact-host allowlist for every registry, bearer-token, and
redirect request. Cross-host token and blob endpoints must each be listed;
HTTPS requests cannot redirect to HTTP. Registry credentials follow the local
Docker configuration and credential-helper settings. Xylem verifies the pinned
manifest and blobs before ORAS extracts a local OCI layout, so `oras` must be
installed on the host. Offline registry use requires a verified cache hit.
OCI modules accept version 2 image manifests with titled file layers.

See the repository's
[module specification](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md)
for the normative Xylem 1.x module contract.
