#!/bin/sh
set -eu

greeting="$1"
name="$(cat "${_XYLEM_INPUT:?}")"
output="${OUTPUT_DIRECTORY:?}/greeting.txt"
printf '%s, %s!\n' "$greeting" "$name" > "$output"
