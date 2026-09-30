#!/bin/sh
set -eu

name="$1"
output="$2"
mkdir -p "$(dirname "$output")"
printf 'Hello, %s!\n' "$name" > "$output"
printf 'Created %s\n' "$output"
