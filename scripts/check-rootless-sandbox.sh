#!/bin/sh
set -eu

fail() {
    printf 'rootless sandbox preflight: %s\n' "$*" >&2
    exit 1
}

runtime=${1:-${XYLEM_SANDBOX_RUNTIME:-}}
workload=${2:-${XYLEM_SANDBOX_WORKLOAD:-}}
minimum_available_bytes=${3:-${XYLEM_SANDBOX_MIN_AVAILABLE_BYTES:-}}

[ -n "$runtime" ] || fail 'runtime is required: docker or podman'
[ -n "$workload" ] || fail 'digest-pinned workload is required'
[ -n "$minimum_available_bytes" ] || fail 'minimum available bytes is required'
case "$minimum_available_bytes" in
    *[!0-9]*) fail 'minimum available bytes must be an unsigned integer' ;;
esac
case "$workload" in
    *@sha256:*) ;;
    *) fail 'workload must use NAME@sha256:DIGEST form' ;;
esac
digest=${workload##*@sha256:}
[ "${#digest}" -eq 64 ] || fail 'workload digest must contain 64 hexadecimal characters'
case "$digest" in
    *[!0-9a-f]*) fail 'workload digest must contain lowercase hexadecimal characters only' ;;
esac
[ "$(uname -s)" = Linux ] || fail 'native Linux is required'
[ "$(uname -m)" = x86_64 ] || fail 'native x86_64 is required'
[ -r /sys/fs/cgroup/cgroup.controllers ] || fail 'cgroup v2 is required'
grep -qw memory /sys/fs/cgroup/cgroup.controllers || fail 'the cgroup v2 memory controller is unavailable'
available_kib=$(awk '$1 == "MemAvailable:" { print $2; exit }' /proc/meminfo)
[ -n "$available_kib" ] || fail 'MemAvailable is unavailable'
available_bytes=$((available_kib * 1024))
[ "$available_bytes" -ge "$minimum_available_bytes" ] || fail "available memory ${available_bytes} is below required ${minimum_available_bytes} bytes"

case "$runtime" in
    docker)
        security=$($runtime info --format '{{json .SecurityOptions}}')
        printf '%s\n' "$security" | grep -qi rootless || fail 'Docker does not report rootless security mode'
        context=$($runtime context show)
        endpoint=$($runtime context inspect --format '{{json .Endpoints.docker.Host}}' "$context")
        case "$endpoint" in
            '"unix://'*) ;;
            *) fail "Docker endpoint ${endpoint} is not a local Unix socket" ;;
        esac
        ;;
    podman)
        [ "$($runtime info --format '{{.Host.Security.Rootless}}')" = true ] || fail 'Podman is not rootless'
        [ "$($runtime info --format '{{.Host.ServiceIsRemote}}')" = false ] || fail 'Podman is using a remote service'
        ;;
    *) fail "unsupported runtime ${runtime}; expected docker or podman" ;;
esac

image_os=$($runtime image inspect --format '{{.Os}}' "$workload")
image_architecture=$($runtime image inspect --format '{{.Architecture}}' "$workload")
[ "$image_os" = linux ] || fail "workload operating system is ${image_os}, expected linux"
[ "$image_architecture" = amd64 ] || fail "workload architecture is ${image_architecture}, expected amd64"
# shellcheck disable=SC2016
$runtime run \
    --entrypoint /bin/sh \
    --memory 67108864 \
    --memory-swap 67108864 \
    --network none \
    --pids-limit 32 \
    --pull never \
    --read-only \
    --rm \
    --user 65532:65532 \
    "$workload" \
    -c 'test -r /sys/fs/cgroup/cgroup.controllers && test -r /sys/fs/cgroup/memory.max && test "$(cat /sys/fs/cgroup/memory.max)" != max' \
    >/dev/null

printf 'rootless sandbox preflight: PASS runtime=%s workload=%s available_bytes=%s\n' "$runtime" "$workload" "$available_bytes"
