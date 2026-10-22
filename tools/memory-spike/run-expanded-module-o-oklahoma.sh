#!/bin/sh
set -eu

fail() {
  printf 'expanded A/P/O measurement: %s\n' "$*" >&2
  exit 1
}

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
available_kib=$(awk '$1 == "MemAvailable:" { print $2; exit }' /proc/meminfo 2>/dev/null || true)
context=${DOCKER_CONTEXT:-default}
memory_limit=${MEMORY_LIMIT:-96g}
minimum_available_bytes=${MINIMUM_AVAILABLE_BYTES:-85899345920}
results_directory=${RESULTS_DIRECTORY:-results-module-o-v2-$(date -u +%Y%m%dT%H%M%SZ)}
workload_root=${WORKLOAD_ROOT:-${HOME}/xylem-memory}

[ "$(uname -s)" = Linux ] || fail 'run this script on Oklahoma, not through a remote Docker context'
[ "$(uname -m)" = x86_64 ] || fail 'native x86_64 is required'
[ -r /sys/fs/cgroup/cgroup.controllers ] || fail 'cgroup v2 is required'
[ -n "${available_kib}" ] || fail 'MemAvailable is unavailable'
case "${minimum_available_bytes}" in
  *[!0-9]*|'') fail 'MINIMUM_AVAILABLE_BYTES must be an unsigned integer' ;;
esac
available_bytes=$((available_kib * 1024))
[ "${available_bytes}" -ge "${minimum_available_bytes}" ] || \
  fail "available memory ${available_bytes} is below required ${minimum_available_bytes} bytes"
security=$(docker --context "${context}" info --format '{{json .SecurityOptions}}')
case "${security}" in
  *rootless*) fail "Docker context ${context} is rootless; use Oklahoma's rootful default context" ;;
esac

printf 'Starting expanded A/P/O measurement: context=%s memory=%s results=%s/%s\n' \
  "${context}" "${memory_limit}" "${workload_root}" "${results_directory}"
printf 'Expected elapsed time: approximately 7 hours. The results directory must remain unique.\n'

DOCKER_CONTEXT=${context}
MEASUREMENT_MANIFEST=manifest-module-o.json
MEMORY_LIMIT=${memory_limit}
RESULTS_DIRECTORY=${results_directory}
WORKLOAD_ROOT=${workload_root}
export DOCKER_CONTEXT MEASUREMENT_MANIFEST MEMORY_LIMIT RESULTS_DIRECTORY WORKLOAD_ROOT
exec "${repository}/tools/memory-spike/docker-measure-module-o.sh"
