#!/bin/sh
set -eu

context=${DOCKER_CONTEXT:-vergil}
image=xylem-memory-spike-probe:local
repository=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
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
  --file "${repository}/tools/memory-spike/Dockerfile.probe" \
  --platform "${platform}" \
  --tag "${image}" \
  "${repository}" >&2

docker --context "${context}" run \
  --cgroupns private \
  --memory 256m \
  --network none \
  --pids-limit 64 \
  --platform "${platform}" \
  --privileged \
  --read-only \
  --rm \
  --tmpfs /tmp:rw,nosuid,nodev,noexec,size=16m \
  "${image}"
