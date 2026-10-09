# Xylem sandbox runner

This directory defines the baseline `linux/amd64` OCI workload used by the rootless Docker and Podman sandbox provider. 
The Debian base, package archive, runtime identity, and installed tools are pinned so identical source inputs can produce the same manifest digest.

Release pipelines publish the image to `savannah.ornl.gov/research-enablement/xylem/runner` and `ghcr.io/jhwohlgemuth/xylem`.
Savannah uses the GitLab registry credentials; GHCR requires a protected, masked `GHCR_TOKEN` with package write access.
Sandbox policies must use either canonical digest reference emitted in the `sandbox-runner.env` job artifact:
`XYLEM_SANDBOX_WORKLOAD` and `XYLEM_SANDBOX_WORKLOAD_SAVANNAH` select Savannah, while `XYLEM_SANDBOX_WORKLOAD_GHCR` selects GHCR.

```yaml
workload:
  default: savannah.ornl.gov/research-enablement/xylem/runner@sha256:...
  allowed:
    - savannah.ornl.gov/research-enablement/xylem/runner@sha256:...
    - ghcr.io/jhwohlgemuth/xylem@sha256:...
```

Tags exist only for registry discovery. Xylem accepts and pulls workloads only by digest.

The mandatory native rootless Docker and Podman conformance jobs consume the
digest-pinned reference from the protected `XYLEM_SANDBOX_WORKLOAD` CI
variable. Their dedicated Linux amd64 runners must expose the matching runtime
and must not use a remote engine, Docker Desktop, or a Podman machine.

The [`ap`](ap/) variant adds the pinned dependencies needed to prepare the
measured Module A plus Module P workflow. It is a separate workload because the
baseline conformance image deliberately remains small.
