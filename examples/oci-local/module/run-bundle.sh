#!/bin/sh
set -eu

name="$1"
output="$2"
bundle="bundle@sha256:98970c614c6c50785f7fc7b220e2b966249e95d6705f60cbc9b451923a74f805"
workspace=".oci-bundle"
mkdir -p "$workspace"
trap 'rm -rf "$workspace"' EXIT HUP INT TERM

oras pull --no-tty --oci-layout --output "$workspace" "$bundle"
mkdir -p "$(dirname "$output")"
sh "$workspace/run.sh" "$name" "$output"
