#!/bin/sh
set -eu

name="$1"
output="$2"
mkdir -p "$(dirname "$output")"
printf 'Hello from the local OCI artifact, %s!\n' "$name" > "$output"
printf 'Created %s\n' "$output"
