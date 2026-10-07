#!/bin/sh
set -eu

cache_volume=${CACHE_VOLUME:-xylem-memory-cache}
context=${DOCKER_CONTEXT:-oklahoma}
image=xylem-memory-spike-measure:local
manifest=${MEASUREMENT_MANIFEST:-manifest.json}
memory_limit=${MEMORY_LIMIT:-64g}
repository=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
results_directory=${RESULTS_DIRECTORY:-results}
workload_root=${WORKLOAD_ROOT:-/home/o9w/xylem-memory}
results_root=${workload_root}/${results_directory}

case "${manifest}" in
  *[!A-Za-z0-9._-]*|'')
    printf 'MEASUREMENT_MANIFEST must be a filename under tools/memory-spike/real\n' >&2
    exit 1
    ;;
esac
case "${results_directory}" in
  *[!A-Za-z0-9._-]*|'')
    printf 'RESULTS_DIRECTORY must be a directory name under WORKLOAD_ROOT\n' >&2
    exit 1
    ;;
esac
if [ "${manifest}" = manifest-module-o.json ] && [ -z "${EARTHDATA_API_TOKEN:-}" ]; then
  printf 'EARTHDATA_API_TOKEN is required for the Module O measurement\n' >&2
  exit 1
fi

server=$(docker --context "${context}" info --format '{{.OSType}}|{{.Architecture}}|{{.CgroupVersion}}|{{.OperatingSystem}}')
server_os=${server%%|*}
server=${server#*|}
server_architecture=${server%%|*}
server=${server#*|}
server_cgroup_version=${server%%|*}
server_operating_system=${server#*|}

if [ "${server_os}" != linux ]; then
  printf 'Docker context %s must target Linux; found %s\n' "${context}" "${server_os}" >&2
  exit 1
fi
case "${server_architecture}" in
  aarch64|arm64) platform=linux/arm64 ;;
  amd64|x86_64) platform=linux/amd64 ;;
  *)
    printf 'Docker context %s must target arm64 or amd64; found %s\n' "${context}" "${server_architecture}" >&2
    exit 1
    ;;
esac
if [ "${server_cgroup_version}" != 2 ]; then
  printf 'Docker context %s must use cgroup v2; found %s\n' "${context}" "${server_cgroup_version}" >&2
  exit 1
fi
case "${server_operating_system}" in
  *Ubuntu*) ;;
  *)
    printf 'Docker context %s must target Ubuntu; found %s\n' "${context}" "${server_operating_system}" >&2
    exit 1
    ;;
esac

printf 'Building %s on Docker context %s (%s, %s, cgroup v%s)\n' \
  "${image}" "${context}" "${server_operating_system}" "${server_architecture}" "${server_cgroup_version}" >&2
docker --context "${context}" build \
  --file "${repository}/tools/memory-spike/Dockerfile.measure" \
  --platform "${platform}" \
  --tag "${image}" \
  "${repository}" >&2

docker --context "${context}" volume inspect "${cache_volume}" >/dev/null 2>&1 || \
  docker --context "${context}" volume create "${cache_volume}" >/dev/null

docker --context "${context}" run --rm \
  --entrypoint sh \
  --mount "type=bind,source=${workload_root},target=/workload" \
  "${image}" \
  -ceu "test -d /workload/corpus && test -d /workload/modules/data && mkdir -p /workload/${results_directory} && test ! -e /workload/${results_directory}/observations.jsonl"

set -- docker --context "${context}" run \
  --cgroupns private \
  --env XYLEM_RUN_STORE=/measurement/runs \
  --memory "${memory_limit}" \
  --memory-swap "${memory_limit}" \
  --mount "type=bind,source=${workload_root},target=/workload,readonly" \
  --mount "type=volume,source=${cache_volume},target=/workload/modules/data" \
  --mount "type=bind,source=${results_root},target=/measurement" \
  --name xylem-memory-measure \
  --pids-limit 4096 \
  --platform "${platform}" \
  --privileged \
  --read-only \
  --rm \
  --shm-size 1g \
  --tmpfs /tmp:rw,nosuid,nodev,size=8g
if [ "${manifest}" = manifest-module-o.json ]; then
  set -- "$@" --env EARTHDATA_API_TOKEN
fi
set -- "$@" \
  "${image}" \
  xylem-memory-spike measure \
  --manifest "/opt/xylem-memory/real/${manifest}" \
  --cgroup-root /sys/fs/cgroup \
  --xylem /usr/local/bin/xylem \
  --work-root /measurement/work \
  --observations /measurement/observations.jsonl
"$@"

docker --context "${context}" run --rm \
  --entrypoint xylem-memory-spike \
  --mount "type=bind,source=${results_root},target=/measurement" \
  --network none \
  --read-only \
  "${image}" \
  analyze \
  --manifest "/opt/xylem-memory/real/${manifest}" \
  --observations /measurement/observations.jsonl \
  --json /measurement/analysis.json \
  --markdown /measurement/report.md

docker --context "${context}" run --rm \
  --entrypoint cat \
  --mount "type=bind,source=${results_root},target=/measurement,readonly" \
  --network none \
  --read-only \
  "${image}" \
  /measurement/report.md
