#!/bin/sh
set -eu

stage="$1"
input="$2"

case "$stage" in
  prepare)
    sleep 1
    printf 'prepare: accepted %s\n' "$input"
    ;;
  validate)
    case "$input" in
      bravo|delta)
        printf 'validate: intentionally rejected %s\n' "$input" >&2
        exit 7
        ;;
      *)
        printf 'validate: approved %s\n' "$input"
        ;;
    esac
    ;;
  publish)
    printf 'publish: completed %s\n' "$input"
    ;;
  *)
    printf 'unknown stage: %s\n' "$stage" >&2
    exit 64
    ;;
esac
