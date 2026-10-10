# Geospatial sandbox workload

This reusable `linux/amd64` workload adds GDAL, Numba, NumPy, and scikit-learn
to Xylem's rootless OCI bootstrap contract. Its Conda environment intentionally
matches `tools/memory-spike/environment.measure.yml`.
The pinned micromamba base is the amd64 manifest used for this qualification
slice.

Build and probe it without publishing:

```bash
docker build \
  --file sandbox/runner/geospatial/Containerfile \
  --platform linux/amd64 \
  --tag xylem-geospatial:local \
  sandbox/runner/geospatial

docker run --rm --entrypoint /bin/sh xylem-geospatial:local \
  -c 'python -c "from osgeo import gdal; import numba, numpy, sklearn"'
```

The deployment build must publish an OCI manifest and use the registry's
canonical `NAME@sha256:DIGEST` result as `XYLEM_SANDBOX_GEOSPATIAL_WORKLOAD`.
The published names are `savannah.ornl.gov/research-enablement/xylem/geospatial`
and `ghcr.io/jhwohlgemuth/xylem-geospatial`. Tags are not
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

Sandbox outputs use Xylem's bounded private disk staging and whole-run atomic
publication path; the final destination is never mounted into the workload.
