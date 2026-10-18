#!/bin/sh
set -eu

input=$1
output=$2
printf 'oci: %s\n' "$(cat "$input")" > "$output"
cat "$output"
