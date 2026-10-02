#!/usr/bin/env bash
set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPOSITORY_ROOT}"

# The path is resolved after changing to the repository root.
# shellcheck disable=SC1091
source security/dependency-baseline.env

fail() {
  printf 'dependency policy: %s\n' "$1" >&2
  exit 1
}

read -r locked_packages duplicate_names < <(
  awk '
    /^name = / {
      name = $3
      gsub(/"/, "", name)
      counts[name]++
      total++
    }
    END {
      duplicates = 0
      for (name in counts) {
        if (counts[name] > 1) {
          duplicates++
        }
      }
      print total, duplicates
    }
  ' Cargo.lock
)

(( locked_packages <= MAX_LOCKED_PACKAGES )) || fail "Cargo.lock grew from ${MAX_LOCKED_PACKAGES} to ${locked_packages} packages"
(( duplicate_names <= MAX_DUPLICATE_PACKAGE_NAMES )) || fail "duplicate package names grew from ${MAX_DUPLICATE_PACKAGE_NAMES} to ${duplicate_names}"

for crate in ${REQUIRED_ACORN_CRATES}; do
  grep -Fq "${crate} = { git = \"https://code.ornl.gov/research-enablement/acorn.git\", rev = \"${ACORN_REVISION}\"" Cargo.toml \
    || fail "${crate} must remain pinned to ACORN revision ${ACORN_REVISION}"
done

forbidden_packages="$({
  awk '/^name = / { gsub(/"/, "", $3); print $3 }' Cargo.lock
} | grep -E "${FORBIDDEN_LOCKED_PACKAGE_PATTERN}" || true)"
[[ -z "${forbidden_packages}" ]] || fail "forbidden locked packages found: ${forbidden_packages//$'\n'/, }"

today="$(date -u +%F)"
while IFS=$'\t' read -r advisory owner review_by rationale; do
  [[ "${advisory}" =~ ^RUSTSEC-[0-9]{4}-[0-9]{4}$ ]] || fail "invalid advisory exception ID: ${advisory}"
  [[ "${owner}" == @* ]] || fail "${advisory} must have an owning account"
  [[ "${review_by}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || fail "${advisory} has an invalid review date"
  [[ -n "${rationale}" ]] || fail "${advisory} must have a rationale"
  deny_entry="$(grep -F "${advisory}" deny.toml)"
  [[ "${deny_entry}" == *"${owner}"* && "${deny_entry}" == *"${review_by}"* ]] \
    || fail "${advisory} owner or review date differs between policy files"
done < <(tail -n +2 security/advisory-exceptions.tsv)

expired_exceptions="$(awk -F '\t' -v today="${today}" 'NR > 1 && $3 < today { print $1 " (review was due " $3 ")" }' security/advisory-exceptions.tsv)"
[[ -z "${expired_exceptions}" ]] || fail "expired advisory exceptions: ${expired_exceptions//$'\n'/, }"

expected_exceptions="$(awk -F '\t' 'NR > 1 { print $1 }' security/advisory-exceptions.tsv | sort)"
configured_exceptions="$(sed -n '/^\[advisories\]/,/^\[/p' deny.toml | grep -Eo 'RUSTSEC-[0-9]{4}-[0-9]{4}' | sort -u)"
[[ "${configured_exceptions}" == "${expected_exceptions}" ]] || fail "deny.toml advisory exceptions do not match security/advisory-exceptions.tsv"

printf 'dependency policy: %s locked packages, %s duplicate names, %s reviewed advisory exceptions\n' \
  "${locked_packages}" \
  "${duplicate_names}" \
  "$(printf '%s\n' "${expected_exceptions}" | wc -l | tr -d ' ')"
