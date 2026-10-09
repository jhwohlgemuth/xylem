#!/bin/sh
set -eu

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
environment_file=${EARTHDATA_ENV_FILE:-${repository}/.env}

if [ -z "${EARTHDATA_API_TOKEN:-}" ]; then
  if [ ! -f "${environment_file}" ]; then
    printf 'EARTHDATA_API_TOKEN is unset and %s does not exist\n' "${environment_file}" >&2
    exit 1
  fi
  EARTHDATA_API_TOKEN=$(awk 'index($0, "EARTHDATA_API_TOKEN=") == 1 { print substr($0, length("EARTHDATA_API_TOKEN=") + 1); exit }' "${environment_file}")
  case "${EARTHDATA_API_TOKEN}" in
    \"*\") EARTHDATA_API_TOKEN=${EARTHDATA_API_TOKEN#\"}; EARTHDATA_API_TOKEN=${EARTHDATA_API_TOKEN%\"} ;;
    \'*\') EARTHDATA_API_TOKEN=${EARTHDATA_API_TOKEN#\'}; EARTHDATA_API_TOKEN=${EARTHDATA_API_TOKEN%\'} ;;
  esac
  export EARTHDATA_API_TOKEN
fi
if [ -z "${EARTHDATA_API_TOKEN}" ]; then
  printf 'EARTHDATA_API_TOKEN is empty\n' >&2
  exit 1
fi

MEASUREMENT_MANIFEST=${MEASUREMENT_MANIFEST:-manifest-module-o.json}
RESULTS_DIRECTORY=${RESULTS_DIRECTORY:-results-module-o}
export MEASUREMENT_MANIFEST RESULTS_DIRECTORY
exec "${repository}/tools/memory-spike/docker-measure.sh"
