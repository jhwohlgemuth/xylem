#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
OUT_PATH="${2:-Formula/xylem.rb}"
SHA256="${3:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEMPLATE_PATH="${SCRIPT_DIR}/formula.template.rb"
SOURCE_URL="https://static.crates.io/crates/xylem-cli/xylem-cli-${VERSION}.crate"

if [[ -z "${VERSION}" ]]; then
  echo "Usage: $0 <version-without-v-prefix> [output-path] [sha256]" >&2
  exit 64
fi
if [[ ! "${VERSION}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "Version must use MAJOR.MINOR.PATCH without a v prefix: ${VERSION}" >&2
  exit 64
fi
if [[ ! -f "${TEMPLATE_PATH}" ]]; then
  echo "Formula template not found at ${TEMPLATE_PATH}" >&2
  exit 66
fi

WORKSPACE_VERSION="$(awk -F ' = ' '$1 == "version" { gsub(/[\"]/, "", $2); print $2; exit }' "${REPOSITORY_ROOT}/Cargo.toml")"
if [[ "${VERSION}" != "${WORKSPACE_VERSION}" ]]; then
  echo "Requested version ${VERSION} does not match workspace version ${WORKSPACE_VERSION}" >&2
  exit 65
fi

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "No SHA-256 utility found (need sha256sum or shasum)" >&2
    exit 69
  fi
}

TEMP_DIRECTORY="$(mktemp -d "${XYLEM_TEMP_ROOT:-/tmp}/xylem-formula.XXXXXX")"
trap 'rm -rf "${TEMP_DIRECTORY}"' EXIT

if [[ -z "${SHA256}" ]]; then
  curl -A "xylem-homebrew-formula/1.0" --retry 3 --retry-delay 1 -LsSf "${SOURCE_URL}" -o "${TEMP_DIRECTORY}/source.tar.gz"
  SHA256="$(hash_file "${TEMP_DIRECTORY}/source.tar.gz")"
fi
if [[ ! "${SHA256}" =~ ^[0-9a-f]{64}$ ]]; then
  echo "SHA-256 must contain exactly 64 lowercase hexadecimal characters" >&2
  exit 64
fi

sed \
  -e "s|__VERSION__|${VERSION}|g" \
  -e "s|__SHA256__|${SHA256}|g" \
  "${TEMPLATE_PATH}" > "${TEMP_DIRECTORY}/formula.rb"

if grep -Eq '__[A-Z0-9_]+__' "${TEMP_DIRECTORY}/formula.rb"; then
  echo "Rendered formula contains unresolved template values" >&2
  exit 65
fi

mkdir -p "$(dirname "${OUT_PATH}")"
mv "${TEMP_DIRECTORY}/formula.rb" "${OUT_PATH}"
chmod 0644 "${OUT_PATH}"

echo "Wrote ${OUT_PATH}"
echo "Source URL: ${SOURCE_URL}"
echo "sha256: ${SHA256}"
