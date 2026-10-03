#!/bin/sh
set -eu

cgroup_root=/sys/fs/cgroup
control_cgroup="${cgroup_root}/xylem-memory-control"
output=/tmp/xylem-memory-probe.json

mkdir "${control_cgroup}"
printf '%s\n' "$$" > "${control_cgroup}/cgroup.procs"
printf '+memory\n' > "${cgroup_root}/cgroup.subtree_control"

/usr/local/bin/xylem-memory-spike probe \
  --cgroup-root "${cgroup_root}" \
  --probe /usr/local/bin/xylem-memory-probe \
  --parent-mib 32 \
  --child-mib 32 \
  --hold-millis 500 \
  --json "${output}"
cat "${output}"
