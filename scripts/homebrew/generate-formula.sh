#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
OUT_PATH="${2:-Formula/xylem.rb}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEMPLATE_PATH="${SCRIPT_DIR}/formula.template.rb"

if [[ -z "${VERSION}" ]]; then
  echo "Usage: $0 <version-without-v-prefix> [output-path]" >&2
  exit 64
fi
if [[ ! "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
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

SOURCE_URL="https://code.ornl.gov/research-enablement/xylem/-/archive/v${VERSION}/xylem-v${VERSION}.tar.gz"

hash_file() {
  local file_path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file_path}" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file_path}" | awk '{print $1}'
    return
  fi
  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "${file_path}" | awk '{print $NF}'
    return
  fi
  echo "No SHA-256 utility found (need one of: sha256sum, shasum, openssl)" >&2
  exit 69
}

source_archive="$(mktemp)"
rendered_formula="$(mktemp)"
trap 'rm -f "${source_archive}" "${rendered_formula}"' EXIT

curl -A "xylem-homebrew-formula/1.0" --retry 3 --retry-delay 1 -LsSf "${SOURCE_URL}" -o "${source_archive}"
SHA256="$(hash_file "${source_archive}")"

sed \
  -e "s|__VERSION__|${VERSION}|g" \
  -e "s|__SHA256__|${SHA256}|g" \
  "${TEMPLATE_PATH}" > "${rendered_formula}"

mkdir -p "$(dirname "${OUT_PATH}")"
mv "${rendered_formula}" "${OUT_PATH}"
chmod 0644 "${OUT_PATH}"

echo "Wrote ${OUT_PATH}"
echo "Source URL: ${SOURCE_URL}"
echo "sha256: ${SHA256}"
