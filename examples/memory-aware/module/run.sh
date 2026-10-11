#!/bin/sh
set -eu

input="$1"
output="$2"
mkdir -p "${output%/*}"
bytes=$(wc -c < "$input")
printf '%s bytes\n' "$bytes" > "$output"
printf 'Counted %s into %s\n' "$input" "$output"
