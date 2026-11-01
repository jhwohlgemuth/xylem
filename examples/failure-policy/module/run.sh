#!/bin/sh
set -eu

input="$1"

case "$input" in
  failure)
    printf 'intentionally failed %s\n' "$input" >&2
    exit 7
    ;;
  *)
    printf 'completed %s\n' "$input"
    ;;
esac
