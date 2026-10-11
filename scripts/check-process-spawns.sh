#!/bin/sh
set -eu

unexpected="$({
  rg -l --glob '*.rs' 'std::process::[^;]*Command|std::process::Command::new' crates/xylem-lib/src crates/xylem-cli/src || true
} | sort -u | grep -Ev '(^|/)tests/|^crates/xylem-lib/src/(container\.rs|sandbox/lifecycle\.rs)$' || true)"

if [ -n "$unexpected" ]; then
  printf '%s\n' 'Direct process spawning is restricted to reviewed lifecycle boundaries:' >&2
  printf '%s\n' "$unexpected" >&2
  exit 1
fi
