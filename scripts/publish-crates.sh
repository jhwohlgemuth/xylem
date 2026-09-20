#!/usr/bin/env bash
set -euo pipefail

: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required}"

version="$(awk -F ' = ' '$1 == "version" { gsub(/["\r]/, "", $2); print $2; exit }' Cargo.toml)"
crates=(xylem-lib xylem-cli)
crates_api="https://crates.io/api/v1/crates"

if [[ ! "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    echo "Workspace version must use canonical MAJOR.MINOR.PATCH semantic versioning: ${version}" >&2
    exit 64
fi
if [[ -n "${CI_COMMIT_TAG:-}" && "${CI_COMMIT_TAG}" != "v${version}" ]]; then
    echo "Tag ${CI_COMMIT_TAG} does not match workspace version ${version}" >&2
    exit 65
fi

published() {
    curl --fail --silent --show-error --user-agent "xylem-ci/${version}" "${crates_api}/$1/${version}" >/dev/null
}

wait_until_published() {
    local crate="$1"
    local _
    for _ in $(seq 1 30); do
        if published "${crate}"; then
            return 0
        fi
        sleep 10
    done
    echo "${crate} ${version} was not available from crates.io after 5 minutes" >&2
    return 1
}

for crate in "${crates[@]}"; do
    if published "${crate}"; then
        echo "${crate} ${version} is already published"
    else
        cargo publish --locked --package "${crate}"
    fi
    wait_until_published "${crate}"
done
