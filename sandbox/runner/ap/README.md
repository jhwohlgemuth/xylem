# Module A + P sandbox workload

This `linux/amd64` workload adds the dependencies used by the measured Module
A plus Module P workflow to Xylem's rootless OCI bootstrap contract. Its Conda
environment intentionally matches `tools/memory-spike/environment.measure.yml`.
The pinned micromamba base is the amd64 manifest used for this qualification
slice.

Build and probe it without publishing:

```bash
docker build \
  --file sandbox/runner/ap/Containerfile \
  --platform linux/amd64 \
  --tag xylem-ap-sandbox-runner:local \
  sandbox/runner/ap

docker run --rm --entrypoint /bin/sh xylem-ap-sandbox-runner:local \
  -c 'python -c "from osgeo import gdal; import numba, numpy, sklearn"'
```

The deployment build must publish an OCI manifest and use the registry's
canonical `NAME@sha256:DIGEST` result as `XYLEM_SANDBOX_WORKLOAD`. Tags are not
accepted by Xylem. The final image must remain Linux/amd64, run as numeric user
and group 65532, declare no entrypoint, and pass
`scripts/check-rootless-sandbox.sh` on the target rootless runtime.

Because the pinned micromamba base declares environment variables, an operator
policy for this workload must allow these names in addition to workflow
variables:

```text
CONDA_OVERRIDE_CUDA
ENV_NAME
LANG
LC_ALL
MAMBA_EXE
MAMBA_ROOT_PREFIX
MAMBA_USER
MAMBA_USER_GID
MAMBA_USER_ID
PATH
PROJ_DATA
PROJ_LIB
PYTHONDONTWRITEBYTECODE
```

This image prepares the dependency boundary only. The current sandbox's sole
writable execution path is a memory-backed tmpfs, while the measured holdouts
produce 11.6 GB and 18.1 GB of disk output. Running those outputs in tmpfs would
count them as resident memory and would not be comparable to the accepted A/P
estimator. Do not use such a run as an activation gate until Xylem provides a
bounded disk-backed private output path.
