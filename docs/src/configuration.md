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

The supported providers are rootless OCI through Docker or Podman and
OpenShell through a Docker or Podman compute driver. An OpenShell policy uses
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
container for every admitted input. Preparation verifies workflow requirements
inside the pinned workload and supports local or HTTP(S) generic scripts,
binaries, and Python modules. HTTP(S) script modules are acquired as Git
repositories; HTTP(S) binaries are downloaded without redirects and verified
against their configured checksum unless checksum verification is explicitly
disabled. Python modules may prepare `environment.yml` with conda or mamba, or
`pixi.toml` with pixi, when the selected workload provides the corresponding
tool.

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

Execution-plan schema v12 records artifact publication and lifecycle intent, the budget,
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
the same read-only selection snapshot as execution. Execution-plan schema v12
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
relative to the declaring configuration. HTTP(S) `.json`, `.yaml`, and `.yml`
locations are read directly; Git locations load repository-root `config.json`
and may use the structured form
`{"scheme":"https","uri":"https://example.org/workflow.git","revision":"v1.2.3"}`.

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
example, `GREETING` is available as `_XYLEM_GREETING`.

## Options

Configuration options include `batch`, `dry_run`, `gpu_monitor`, `offline`, `allowed`,
`resume`, `skip_verify_checksum`, and `skip_verify_requirements`. Enabling a
Boolean CLI flag overrides a false or absent configuration value. When
supplied, `--allowed`, `--input`, and `--artifact-output` replace their
configured values.

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
scheme; a non-default port must be listed explicitly. When this option is
absent or empty, remote hosts remain unrestricted for backward compatibility.
The allowlist is enforced before remote work and across HTTP redirects.

Unless `offline` is active, Xylem checks an HTTP(S) binary or Git repository
for reachability before downloading or cloning it. The execution plan records
the acquisition action without performing it. Dry-run does not create a run
directory, probe the network, change permissions, spawn a process, or write an
environment variable.

Nested workflows may only tighten the inherited allowlist and may enable
offline mode. A remote configuration wrapper fails without making a request in
dry-run mode; local wrappers still produce a complete flattened plan.

See the repository's
[module specification](https://code.ornl.gov/research-enablement/xylem/-/blob/main/MODULE_SPEC.md)
for the normative Xylem 1.x module contract.
