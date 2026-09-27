#!/bin/sh
set -eu

input=$1
output=$2
printf 'docker: %s\n' "$(cat "$input")" > "$output"
cat "$output"
