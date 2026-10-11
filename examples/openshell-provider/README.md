# OpenShell provider

This example exercises Xylem's experimental public OpenShell lifecycle with a
local, authenticated OpenShell 0.1.x gateway. The provider creates separate
preparation and execution sandboxes, transfers the checked-in module through a
validated archive, uploads the input only to execution, runs the module with an
exact argument array, downloads `greeting.txt`, and verifies cleanup.

Xylem prints a warning when this provider is selected: OpenShell 0.1.x does not
guarantee offline workload acquisition or portable per-sandbox PID and scratch
limits. The commented `.xylem.jsonc` file defines the workflow,
`sandbox-policy.yaml` selects OpenShell, and the two files under `policies/`
are the generated phase policies used by the qualification test.

Prerequisites:

- `openshell` 0.1.2 on `PATH`;
- an authenticated local gateway with exactly one Docker or Podman compute
  driver; and
- a compatible, digest-pinned Xylem workload containing `curl`, `python`,
  `sh`, and `tar` and using UID/GID 65532.

Replace the example workload in `sandbox-policy.yaml` with that digest, select
the gateway's compute driver, and run:

```shell
cd examples/openshell-provider
cargo run -p xylem-cli -- --sandbox-policy sandbox-policy.yaml run .xylem.jsonc
```

To collect qualification evidence instead, set the driver and workload
environment variables and run the ignored test:

```shell
export XYLEM_OPENSHELL_DRIVER=docker
export XYLEM_OPENSHELL_WORKLOAD='registry.example/xylem@sha256:<64-hex-digest>'
export XYLEM_SANDBOX_EVIDENCE_DIR="$PWD/sandbox-conformance-evidence/openshell/docker"

cargo test -p xylem-lib \
  sandbox::openshell::tests::test_openshell_provider_feasibility \
  -- --ignored --exact
```

Use `podman` for both driver values when qualifying the rootless Podman path.
The evidence directory receives the effective phase policies and
`openshell-feasibility.json`. A successful lifecycle can still produce a
`blocked` verdict while mandatory portable resource checks remain unresolved.
