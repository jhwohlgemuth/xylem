#!/bin/sh
set -eu

input=$1
output=$2
printf 'script: %s\n' "$(cat "$input")" > "$output"
cat "$output"
