#!/bin/sh
set -eu

if [ "$#" -eq 0 ]; then
  printf 'usage: xylem-memory-docker-measure COMMAND [ARGUMENT ...]\n' >&2
  exit 2
fi

cgroup_root=/sys/fs/cgroup
control_cgroup="${cgroup_root}/xylem-memory-control"

mkdir "${control_cgroup}"
printf '%s\n' "$$" > "${control_cgroup}/cgroup.procs"
printf '+memory\n' > "${cgroup_root}/cgroup.subtree_control"

exec "$@"
