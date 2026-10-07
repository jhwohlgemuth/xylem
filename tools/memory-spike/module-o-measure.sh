#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  printf 'usage: xylem-memory-module-o SOURCE_DIRECTORY OUTPUT_DIRECTORY\n' >&2
  exit 2
fi

source_directory=$1
output_directory=$2
expected=$(find "${source_directory}" -type f -name '*_MODA_MODP.TIF' -size +0c | wc -l)
if [ "${expected}" -eq 0 ]; then
  printf 'Module O received no non-empty Module P outputs under %s\n' "${source_directory}" >&2
  exit 1
fi

python -m lib --source_directory "${source_directory}" --output_directory "${output_directory}"

actual=$(find "${output_directory}" -type f -name '*_MODA_MODP_MODO.TIF' -size +0c | wc -l)
if [ "${actual}" -ne "${expected}" ]; then
  printf 'Module O produced %s of %s expected non-empty outputs\n' "${actual}" "${expected}" >&2
  exit 1
fi
